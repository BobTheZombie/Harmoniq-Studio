use std::env;
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use harmoniq_engine::{EngineCommand, EngineCommandQueue, MidiEvent, MidiTimestamp};
use harmoniq_midi::{config as midi_config, device::MidiInputConfig};
use midir::{Ignore, MidiInput, MidiInputConnection};
use ringbuf::{HeapConsumer, HeapRb};
use tracing::{info, warn};
use winit::keyboard::{KeyCode, ModifiersState};

pub mod qwerty;
pub use qwerty::QwertyKeyboardInput;

const MIDI_QUEUE_CAPACITY: usize = 1024;
const MIDI_DISPATCH_BATCH: usize = 64;
const MIDI_IDLE_SLEEP: Duration = Duration::from_micros(200);

pub trait MidiInputDevice: Send {
    fn name(&self) -> &str;
    fn enabled(&self) -> bool;
    fn set_enabled(&mut self, on: bool);
    fn push_key_event(&mut self, key: KeyCode, pressed: bool, mods: ModifiersState, time: Instant);
    fn drain_events<'a>(&'a mut self, out: &mut dyn FnMut(MidiEvent, Instant));
    fn panic(&mut self, time: Instant);
}

pub struct MidiConnection {
    stop: Arc<AtomicBool>,
    dispatcher: Option<JoinHandle<()>>,
    _connections: Vec<MidiInputConnection<()>>,
}

impl MidiConnection {
    fn new(
        stop: Arc<AtomicBool>,
        dispatcher: JoinHandle<()>,
        connections: Vec<MidiInputConnection<()>>,
    ) -> Self {
        Self {
            stop,
            dispatcher: Some(dispatcher),
            _connections: connections,
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
    let mut input = MidiInput::new("harmoniq-midi").context("failed to open MIDI input")?;
    input.ignore(Ignore::None);

    let ports = input.ports();
    if ports.is_empty() {
        anyhow::bail!("no MIDI inputs available");
    }

    let mut configs: Vec<MidiInputConfig> = midi_config::load()
        .inputs
        .into_iter()
        .filter(|cfg| cfg.enabled)
        .collect();

    if configs.is_empty() {
        let Some(requested_name) = requested else {
            return Ok(None);
        };
        let requested_lower = requested_name.trim().to_lowercase();
        if let Some((port_index, port)) = ports.iter().enumerate().find(|(_idx, port)| {
            input
                .port_name(port)
                .map(|name| name.to_lowercase().contains(&requested_lower))
                .unwrap_or(false)
        }) {
            let name = input
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            configs.push(MidiInputConfig {
                enabled: true,
                name,
                port_index,
                ..MidiInputConfig::default()
            });
        } else {
            anyhow::bail!("could not find MIDI input matching '{requested_name}'");
        }
    }

    let (producer, consumer) = HeapRb::new(MIDI_QUEUE_CAPACITY).split();
    let producer = Arc::new(Mutex::new(producer));
    let stop = Arc::new(AtomicBool::new(false));
    let mode = detect_channel_mode();
    let dispatcher = spawn_dispatcher(consumer, command_queue.clone(), mode, Arc::clone(&stop))?;

    let start = Instant::now();
    let stop_flag = Arc::clone(&stop);
    let mut connections = Vec::new();
    for (index, mut cfg) in configs.into_iter().enumerate() {
        let Some(port) = ports.get(cfg.port_index) else {
            warn!(
                index = cfg.port_index,
                "configured MIDI port not available; skipping"
            );
            continue;
        };

        if cfg.name.is_empty() {
            cfg.name = input
                .port_name(port)
                .unwrap_or_else(|_| format!("Input #{}", cfg.port_index + 1));
        }

        let stop_flag = Arc::clone(&stop);
        let producer = Arc::clone(&producer);
        let callback_cfg = cfg.clone();
        let connection = input
            .connect(
                port,
                &format!("harmoniq-midi-connection-{index}"),
                move |_timestamp, message, _| {
                    if stop_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(event) = QueuedMidiEvent::from_message_with_config(
                        MidiTimestamp::from_duration(start.elapsed()),
                        message,
                        &callback_cfg,
                    ) {
                        if let Ok(mut guard) = producer.lock() {
                            if guard.push(event).is_err() {
                                warn!("MIDI queue full; dropping event");
                            }
                        }
                    }
                },
                (),
            )
            .map_err(|err| anyhow!("failed to create MIDI connection: {err}"))?;

        info!(port = %cfg.name, "Connected MIDI input");
        connections.push(connection);
    }

    if connections.is_empty() {
        warn!("No MIDI inputs connected after applying configuration");
        return Ok(None);
    }

    Ok(Some(MidiConnection::new(stop, dispatcher, connections)))
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
                    if let Some(parsed) = parse_midi_event(&event, &mode) {
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
                if let Some(parsed) = parse_midi_event(&event, &mode) {
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

#[derive(Clone, Debug)]
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
    fn remap(&self, channel: u8) -> u8 {
        match self {
            MidiChannelMode::Omni => channel,
            MidiChannelMode::MpeLower { master, members } => {
                if channel == *master {
                    0
                } else if members.contains(&channel) {
                    channel.saturating_sub(*members.start()) + 1
                } else {
                    channel
                }
            }
            MidiChannelMode::MpeUpper { master, members } => {
                if channel == *master {
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
    channel_mode_override: Option<MidiChannelMode>,
}

impl QueuedMidiEvent {
    fn from_message_with_config(
        timestamp: MidiTimestamp,
        message: &[u8],
        config: &MidiInputConfig,
    ) -> Option<Self> {
        if message.is_empty() {
            return None;
        }

        let len = message.len().min(3);
        let mut data = [0u8; 3];
        data[..len].copy_from_slice(&message[..len]);

        let status = data[0] & 0xF0;
        let channel = data[0] & 0x0F;

        if !config.mpe {
            if let Some(filter) = config.channel_filter {
                if filter != 0 && channel != filter.saturating_sub(1) {
                    return None;
                }
            }
        }

        if matches!(status, 0x80 | 0x90) && len >= 2 {
            if let Some(note) = Self::transpose_note(data[1], config.transpose) {
                data[1] = note;
            } else {
                return None;
            }
        }

        if status == 0x90 && len >= 3 {
            data[2] = Self::apply_velocity_curve(data[2], config.velocity_curve);
        }

        if let Some(route) = config.route_to_channel {
            let target = route.saturating_sub(1).min(15) as u8;
            data[0] = (data[0] & 0xF0) | target;
        }

        Some(Self {
            timestamp,
            data,
            len: len as u8,
            channel_mode_override: config.mpe.then_some(MidiChannelMode::Omni),
        })
    }

    fn channel(&self) -> u8 {
        self.data[0] & 0x0F
    }

    fn status(&self) -> u8 {
        self.data[0] & 0xF0
    }
    fn transpose_note(note: u8, transpose: i8) -> Option<u8> {
        let shifted = note as i16 + transpose as i16;
        (0..=127).contains(&shifted).then_some(shifted as u8)
    }

    fn apply_velocity_curve(velocity: u8, curve: u8) -> u8 {
        const SCALES: [f32; 5] = [0.6, 0.8, 1.0, 1.2, 1.4];
        let scale = SCALES[curve.min((SCALES.len() - 1) as u8) as usize];
        (velocity as f32 * scale).round().clamp(0.0, 127.0) as u8
    }
}

fn parse_midi_event(event: &QueuedMidiEvent, mode: &MidiChannelMode) -> Option<MidiEvent> {
    let channel_mode = event.channel_mode_override.as_ref().unwrap_or(mode);
    let channel = channel_mode.remap(event.channel());
    match event.status() {
        0x80 => {
            if event.len < 2 {
                return None;
            }
            Some(MidiEvent::NoteOff {
                channel,
                note: event.data[1],
                sample_offset: 0,
                timestamp: Some(event.timestamp),
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
                    sample_offset: 0,
                    timestamp: Some(event.timestamp),
                })
            } else {
                Some(MidiEvent::NoteOn {
                    channel,
                    note: event.data[1],
                    velocity,
                    sample_offset: 0,
                    timestamp: Some(event.timestamp),
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
                sample_offset: 0,
                timestamp: Some(event.timestamp),
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
                sample_offset: 0,
                timestamp: Some(event.timestamp),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> MidiInputConfig {
        MidiInputConfig {
            enabled: true,
            name: "Test".into(),
            port_index: 0,
            ..MidiInputConfig::default()
        }
    }

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
            .filter_map(|event| parse_midi_event(&event, &mode))
            .collect()
    }

    #[test]
    fn note_on_zero_velocity_maps_to_note_off() {
        let event = QueuedMidiEvent {
            timestamp: MidiTimestamp::from_micros(0),
            data: [0x90, 60, 0],
            len: 3,
            channel_mode_override: None,
        };
        let result = collect_events(&[event], MidiChannelMode::Omni);
        assert!(matches!(result.as_slice(), [MidiEvent::NoteOff { .. }]));
    }

    #[test]
    fn channel_filter_drops_mismatched_events() {
        let mut cfg = base_config();
        cfg.channel_filter = Some(2);
        let ts = MidiTimestamp::from_micros(0);
        assert!(QueuedMidiEvent::from_message_with_config(ts, &[0x90, 60, 100], &cfg).is_none());
        assert!(QueuedMidiEvent::from_message_with_config(ts, &[0x91, 60, 100], &cfg).is_some());
    }

    #[test]
    fn transpose_velocity_and_routing_applied() {
        let mut cfg = base_config();
        cfg.transpose = 1;
        cfg.velocity_curve = 0;
        cfg.route_to_channel = Some(3);

        let ts = MidiTimestamp::from_micros(0);
        let event = QueuedMidiEvent::from_message_with_config(ts, &[0x90, 60, 100], &cfg)
            .expect("event should be produced");
        assert_eq!(event.data[0], 0x92); // routed to channel 3 (0-indexed 2)
        assert_eq!(event.data[1], 61); // transposed
        assert_eq!(event.data[2], 60); // soft curve applied
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
                channel_mode_override: None,
            })
            .collect();

        let observed = collect_events(&events, MidiChannelMode::Omni);
        assert_eq!(observed.len(), scheduled.len());

        let mut worst = Duration::from_micros(0);
        for (observed, expected) in observed.iter().zip(scheduled.iter()) {
            let diff = observed
                .timestamp()
                .unwrap_or_else(|| MidiTimestamp::from_micros(0))
                .abs_diff(*expected);
            if diff > worst {
                worst = diff;
            }
        }

        assert!(worst <= Duration::from_micros(500));
    }
}
