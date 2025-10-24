use std::env;
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use harmoniq_engine::{EngineCommand, EngineCommandQueue, MidiEvent, MidiTimestamp};
use midir::{Ignore, MidiInput, MidiInputConnection};
use ringbuf::{HeapConsumer, HeapRb};
use tracing::{info, warn};

const MIDI_QUEUE_CAPACITY: usize = 1024;
const MIDI_DISPATCH_BATCH: usize = 64;
const MIDI_IDLE_SLEEP: Duration = Duration::from_micros(200);

pub struct MidiConnection {
    stop: Arc<AtomicBool>,
    dispatcher: Option<JoinHandle<()>>,
    _connection: MidiInputConnection<()>,
}

impl MidiConnection {
    fn new(
        stop: Arc<AtomicBool>,
        dispatcher: JoinHandle<()>,
        connection: MidiInputConnection<()>,
    ) -> Self {
        Self {
            stop,
            dispatcher: Some(dispatcher),
            _connection: connection,
        }
    }
}

impl Drop for MidiConnection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.dispatcher.take() {
            if let Err(err) = handle.join() {
                warn!(?err, "failed to join MIDI dispatcher thread");
            }
        }
    }
}

pub fn list_midi_inputs() -> anyhow::Result<Vec<String>> {
    let mut input =
        MidiInput::new("harmoniq-midi-list").context("failed to initialise MIDI subsystem")?;
    input.ignore(Ignore::None);
    let ports = input.ports();
    let mut result = Vec::new();
    for port in &ports {
        let name = input
            .port_name(port)
            .unwrap_or_else(|_| "Unknown".to_string());
        result.push(name);
    }
    Ok(result)
}

pub fn open_midi_input(
    requested: Option<String>,
    command_queue: EngineCommandQueue,
) -> anyhow::Result<Option<MidiConnection>> {
    let Some(requested_name) = requested else {
        return Ok(None);
    };

    let mut input = MidiInput::new("harmoniq-midi").context("failed to open MIDI input")?;
    input.ignore(Ignore::None);

    let ports = input.ports();
    if ports.is_empty() {
        anyhow::bail!("no MIDI inputs available");
    }

    let requested_lower = requested_name.trim().to_lowercase();
    let target_port = if matches!(requested_lower.as_str(), "" | "auto" | "default") {
        ports.get(0)
    } else {
        ports.iter().find(|port| {
            input
                .port_name(port)
                .map(|name| name.to_lowercase().contains(&requested_lower))
                .unwrap_or(false)
        })
    }
    .ok_or_else(|| anyhow!("could not find MIDI input matching '{requested_name}'"))?;

    let port_name = input
        .port_name(target_port)
        .unwrap_or_else(|_| "Unknown".to_string());

    let (producer, consumer) = HeapRb::new(MIDI_QUEUE_CAPACITY).split();
    let stop = Arc::new(AtomicBool::new(false));
    let mode = detect_channel_mode();
    let dispatcher = spawn_dispatcher(consumer, command_queue.clone(), mode, Arc::clone(&stop))?;

    let start = Instant::now();
    let stop_flag = Arc::clone(&stop);
    let mut producer = producer;
    let connection = input
        .connect(
            target_port,
            "harmoniq-midi-connection",
            move |_timestamp, message, _| {
                if stop_flag.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(event) = QueuedMidiEvent::from_message(
                    MidiTimestamp::from_duration(start.elapsed()),
                    message,
                ) {
                    if producer.push(event).is_err() {
                        warn!("MIDI queue full; dropping event");
                    }
                } else {
                    warn!(?message, "unsupported MIDI message received");
                }
            },
            (),
        )
        .map_err(|err| anyhow!("failed to create MIDI connection: {err}"))?;

    info!(port = %port_name, "Connected MIDI input");
    Ok(Some(MidiConnection::new(stop, dispatcher, connection)))
}

fn spawn_dispatcher(
    mut consumer: HeapConsumer<QueuedMidiEvent>,
    queue: EngineCommandQueue,
    mode: MidiChannelMode,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("harmoniq-midi-dispatch".into())
        .spawn(move || {
            let mut staging = Vec::with_capacity(MIDI_DISPATCH_BATCH);
            let mut translated = Vec::with_capacity(MIDI_DISPATCH_BATCH);
            while !stop.load(Ordering::Acquire) {
                staging.clear();
                drain_queue(&mut consumer, &mut staging, MIDI_DISPATCH_BATCH);
                if staging.is_empty() {
                    thread::sleep(MIDI_IDLE_SLEEP);
                    continue;
                }

                translated.clear();
                for event in staging.drain(..) {
                    if let Some(parsed) = parse_midi_event(&event, mode) {
                        translated.push(parsed);
                    }
                }

                if translated.is_empty() {
                    continue;
                }

                if let Err(err) = queue.try_send(EngineCommand::SubmitMidi(translated.clone())) {
                    warn!(?err, "failed to enqueue MIDI event batch");
                }
            }

            // Drain remaining events on shutdown.
            staging.clear();
            translated.clear();
            drain_queue(&mut consumer, &mut staging, usize::MAX);
            for event in staging.drain(..) {
                if let Some(parsed) = parse_midi_event(&event, mode) {
                    translated.push(parsed);
                }
            }
            if !translated.is_empty() {
                let _ = queue.try_send(EngineCommand::SubmitMidi(translated));
            }
        })
        .map_err(|err| anyhow!("failed to spawn MIDI dispatcher thread: {err}"))
}

fn drain_queue(
    consumer: &mut HeapConsumer<QueuedMidiEvent>,
    staging: &mut Vec<QueuedMidiEvent>,
    limit: usize,
) {
    while staging.len() < limit {
        match consumer.pop() {
            Some(event) => staging.push(event),
            None => break,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MidiChannelMode {
    Omni,
    MpeLower {
        master: u8,
        members: RangeInclusive<u8>,
    },
    MpeUpper {
        master: u8,
        members: RangeInclusive<u8>,
    },
}

impl MidiChannelMode {
    fn remap(self, channel: u8) -> u8 {
        match self {
            MidiChannelMode::Omni => channel,
            MidiChannelMode::MpeLower {
                master,
                ref members,
            } => {
                if channel == master {
                    0
                } else if members.contains(&channel) {
                    channel.saturating_sub(*members.start()) + 1
                } else {
                    channel
                }
            }
            MidiChannelMode::MpeUpper {
                master,
                ref members,
            } => {
                if channel == master {
                    0
                } else if members.contains(&channel) {
                    channel.saturating_sub(*members.start()) + 1
                } else {
                    channel
                }
            }
        }
    }
}

fn detect_channel_mode() -> MidiChannelMode {
    match env::var("HARMONIQ_MPE_MODE") {
        Ok(mode) if mode.eq_ignore_ascii_case("lower") => MidiChannelMode::MpeLower {
            master: 0,
            members: 1..=15,
        },
        Ok(mode) if mode.eq_ignore_ascii_case("upper") => MidiChannelMode::MpeUpper {
            master: 15,
            members: 0..=14,
        },
        Ok(mode) if mode.eq_ignore_ascii_case("mpe") => MidiChannelMode::MpeLower {
            master: 0,
            members: 1..=15,
        },
        _ => MidiChannelMode::Omni,
    }
}

#[derive(Clone, Debug)]
struct QueuedMidiEvent {
    timestamp: MidiTimestamp,
    data: [u8; 3],
    len: u8,
}

impl QueuedMidiEvent {
    fn from_message(timestamp: MidiTimestamp, message: &[u8]) -> Option<Self> {
        if message.is_empty() {
            return None;
        }
        let len = message.len().min(3);
        let mut data = [0u8; 3];
        data[..len].copy_from_slice(&message[..len]);
        Some(Self {
            timestamp,
            data,
            len: len as u8,
        })
    }

    fn channel(&self) -> u8 {
        self.data[0] & 0x0F
    }

    fn status(&self) -> u8 {
        self.data[0] & 0xF0
    }
}

fn parse_midi_event(event: &QueuedMidiEvent, mode: MidiChannelMode) -> Option<MidiEvent> {
    let channel = mode.remap(event.channel());
    match event.status() {
        0x80 => {
            if event.len < 2 {
                return None;
            }
            Some(MidiEvent::NoteOff {
                channel,
                note: event.data[1],
                timestamp: event.timestamp,
            })
        }
        0x90 => {
            if event.len < 3 {
                return None;
            }
            let velocity = event.data[2];
            if velocity == 0 {
                Some(MidiEvent::NoteOff {
                    channel,
                    note: event.data[1],
                    timestamp: event.timestamp,
                })
            } else {
                Some(MidiEvent::NoteOn {
                    channel,
                    note: event.data[1],
                    velocity,
                    timestamp: event.timestamp,
                })
            }
        }
        0xB0 => {
            if event.len < 3 {
                return None;
            }
            Some(MidiEvent::ControlChange {
                channel,
                control: event.data[1],
                value: event.data[2],
                timestamp: event.timestamp,
            })
        }
        0xE0 => {
            if event.len < 3 {
                return None;
            }
            Some(MidiEvent::PitchBend {
                channel,
                lsb: event.data[1],
                msb: event.data[2],
                timestamp: event.timestamp,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_events(events: &[QueuedMidiEvent], mode: MidiChannelMode) -> Vec<MidiEvent> {
        let ring = HeapRb::new(events.len().max(1));
        let (mut producer, mut consumer) = ring.split();
        for event in events {
            producer.push(event.clone()).unwrap();
        }
        let mut staging = Vec::new();
        drain_queue(&mut consumer, &mut staging, usize::MAX);
        staging
            .into_iter()
            .filter_map(|event| parse_midi_event(&event, mode))
            .collect()
    }

    #[test]
    fn note_on_zero_velocity_maps_to_note_off() {
        let event = QueuedMidiEvent {
            timestamp: MidiTimestamp::from_micros(0),
            data: [0x90, 60, 0],
            len: 3,
        };
        let result = collect_events(&[event], MidiChannelMode::Omni);
        assert!(matches!(result.as_slice(), [MidiEvent::NoteOff { .. }]));
    }

    #[test]
    fn loopback_jitter_under_half_millisecond() {
        let scheduled: Vec<_> = [0u64, 250, 540, 810, 1100, 1430]
            .into_iter()
            .map(MidiTimestamp::from_micros)
            .collect();
        let events: Vec<_> = scheduled
            .iter()
            .enumerate()
            .map(|(index, timestamp)| QueuedMidiEvent {
                timestamp: *timestamp,
                data: [0x90, 60 + index as u8, 100],
                len: 3,
            })
            .collect();

        let observed = collect_events(&events, MidiChannelMode::Omni);
        assert_eq!(observed.len(), scheduled.len());

        let mut worst = Duration::from_micros(0);
        for (observed, expected) in observed.iter().zip(scheduled.iter()) {
            let diff = observed.timestamp().abs_diff(*expected);
            if diff > worst {
                worst = diff;
            }
        }

        assert!(worst <= Duration::from_micros(500));
    }
}
