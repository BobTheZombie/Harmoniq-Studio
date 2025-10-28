use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};

use crate::device::{MidiBackend, MidiCallback, MidiDeviceId, MidiEvent, MidiMessage, MidiSource};
use crate::MidiTimestamp;

/// Backend implemented using the `midir` crate.
pub struct MidirBackend {
    next_id: MidiDeviceId,
    connections: HashMap<MidiDeviceId, MidiInputConnection<()>>,
    epoch: Instant,
}

impl Default for MidirBackend {
    fn default() -> Self {
        Self {
            next_id: 1,
            connections: HashMap::new(),
            epoch: Instant::now(),
        }
    }
}

impl MidirBackend {
    fn allocate_id(&mut self) -> MidiDeviceId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl MidiBackend for MidirBackend {
    fn enumerate(&self) -> anyhow::Result<Vec<String>> {
        let input = MidiInput::new("harmoniq-midi").context("initialise midir for enumeration")?;
        let mut names = Vec::new();
        for (index, port) in input.ports().into_iter().enumerate() {
            let name = input
                .port_name(&port)
                .unwrap_or_else(|_| format!("Port {index}"));
            names.push(name);
        }
        Ok(names)
    }

    fn open_input(
        &mut self,
        port_index: usize,
        cb: MidiCallback,
        user: *mut c_void,
    ) -> anyhow::Result<MidiDeviceId> {
        let mut input = MidiInput::new("harmoniq-midi").context("initialise midir for input")?;
        input.ignore(Ignore::None);
        let ports: Vec<MidiInputPort> = input.ports();
        let Some(port) = ports.get(port_index) else {
            anyhow::bail!("midi port index out of range");
        };
        let name = input
            .port_name(port)
            .unwrap_or_else(|_| format!("Port {port_index}"));
        let name_arc: Arc<str> = Arc::from(name);
        let id = self.allocate_id();
        let epoch = self.epoch;
        let name_for_cb = Arc::clone(&name_arc);
        let connection = input
            .connect(
                port,
                "harmoniq-midi-conn",
                move |_timestamp, message, _| {
                    if message.is_empty() {
                        return;
                    }
                    let msg = if message.len() == 3 {
                        MidiMessage::Raw([
                            message[0],
                            *message.get(1).unwrap_or(&0),
                            *message.get(2).unwrap_or(&0),
                        ])
                    } else {
                        MidiMessage::SysEx(message.to_vec())
                    };
                    let event = MidiEvent {
                        ts: MidiTimestamp {
                            nanos_monotonic: epoch.elapsed().as_nanos() as u64,
                        },
                        msg,
                    };
                    cb(
                        MidiSource::Device {
                            id,
                            name: Arc::clone(&name_for_cb),
                        },
                        event,
                        user,
                    );
                },
                (),
            )
            .map_err(|err| anyhow::anyhow!("failed to connect midi input: {err}"))?;
        self.connections.insert(id, connection);
        Ok(id)
    }

    fn close_input(&mut self, id: MidiDeviceId) {
        self.connections.remove(&id);
    }
}
