use anyhow::{anyhow, Context};
use harmoniq_engine::{EngineCommand, EngineCommandQueue, MidiEvent};
use midir::{Ignore, MidiInput, MidiInputConnection};
use tracing::{info, warn};

pub struct MidiConnection {
    _connection: MidiInputConnection<()>,
}

impl MidiConnection {
    pub fn new(connection: MidiInputConnection<()>) -> Self {
        Self {
            _connection: connection,
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

    let queue = command_queue.clone();
    let connection = input
        .connect(
            target_port,
            "harmoniq-midi-connection",
            move |timestamp, message, _| {
                if let Some(event) = parse_midi_event(message) {
                    if let Err(err) = queue.try_send(EngineCommand::SubmitMidi(vec![event])) {
                        warn!(?err, "failed to enqueue MIDI event");
                    }
                } else {
                    warn!(?message, timestamp, "unsupported MIDI message received");
                }
            },
            (),
        )
        .map_err(|err| anyhow!("failed to create MIDI connection: {err}"))?;

    info!(port = %port_name, "Connected MIDI input");
    Ok(Some(MidiConnection::new(connection)))
}

fn parse_midi_event(message: &[u8]) -> Option<MidiEvent> {
    if message.is_empty() {
        return None;
    }

    let status = message[0] & 0xF0;
    let channel = message[0] & 0x0F;

    match status {
        0x80 => {
            if message.len() < 3 {
                return None;
            }
            Some(MidiEvent::NoteOff {
                channel,
                note: message[1],
            })
        }
        0x90 => {
            if message.len() < 3 {
                return None;
            }
            let velocity = message[2];
            if velocity == 0 {
                Some(MidiEvent::NoteOff {
                    channel,
                    note: message[1],
                })
            } else {
                Some(MidiEvent::NoteOn {
                    channel,
                    note: message[1],
                    velocity,
                })
            }
        }
        0xB0 => {
            if message.len() < 3 {
                return None;
            }
            Some(MidiEvent::ControlChange {
                channel,
                control: message[1],
                value: message[2],
            })
        }
        0xE0 => {
            if message.len() < 3 {
                return None;
            }
            Some(MidiEvent::PitchBend {
                channel,
                lsb: message[1],
                msb: message[2],
            })
        }
        _ => None,
    }
}
