use std::sync::Arc;

use anyhow::Context;
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};

/// Handle to an open MIDI output connection.
pub struct MidiOutputHandle {
    name: Arc<str>,
    connection: MidiOutputConnection<()>,
}

impl MidiOutputHandle {
    /// Create a new handle from an existing connection.
    pub fn new(name: impl Into<Arc<str>>, connection: MidiOutputConnection<()>) -> Self {
        Self {
            name: name.into(),
            connection,
        }
    }

    /// Name of the connected port.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Send a raw MIDI message over the port.
    pub fn send(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.connection
            .send(bytes)
            .map_err(|err| anyhow::anyhow!("failed to send MIDI message: {err}"))
    }
}

/// Platform MIDI output helper based on the `midir` crate.
#[derive(Default)]
pub struct MidiOutputManager {
    output: MidiOutput,
}

impl MidiOutputManager {
    /// Enumerate available output port names.
    pub fn enumerate(&self) -> anyhow::Result<Vec<String>> {
        let mut names = Vec::new();
        for (index, port) in self.output.ports().into_iter().enumerate() {
            let name = self
                .output
                .port_name(&port)
                .unwrap_or_else(|_| format!("Port {index}"));
            names.push(name);
        }
        Ok(names)
    }

    /// Open an output connection by index.
    pub fn open_port(&mut self, port_index: usize) -> anyhow::Result<MidiOutputHandle> {
        let ports: Vec<MidiOutputPort> = self.output.ports();
        let Some(port) = ports.get(port_index) else {
            anyhow::bail!("midi port index out of range");
        };
        let name = self
            .output
            .port_name(port)
            .unwrap_or_else(|_| format!("Port {port_index}"));
        let conn = self
            .output
            .connect(port, "harmoniq-midi-output")
            .context("failed to open MIDI output")?;
        Ok(MidiOutputHandle::new(name, conn))
    }
}
