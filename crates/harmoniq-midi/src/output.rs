use std::sync::Arc;

use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};

/// Handle to an open MIDI output connection.
pub struct MidiOutputHandle {
    name: Arc<str>,
    connection: MidiOutputConnection,
}

impl MidiOutputHandle {
    /// Create a new handle from an existing connection.
    pub fn new(name: impl Into<Arc<str>>, connection: MidiOutputConnection) -> Self {
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
pub struct MidiOutputManager;

impl MidiOutputManager {
    /// Initialize a new MIDI output manager.
    pub fn new() -> anyhow::Result<Self> {
        MidiOutput::new("harmoniq-midi").map(|_| Self)
    }

    /// Enumerate available output port names.
    pub fn enumerate(&self) -> anyhow::Result<Vec<String>> {
        let output = MidiOutput::new("harmoniq-midi")?;
        let mut names = Vec::new();
        for (index, port) in output.ports().into_iter().enumerate() {
            let name = output
                .port_name(&port)
                .unwrap_or_else(|_| format!("Port {index}"));
            names.push(name);
        }
        Ok(names)
    }

    /// Open an output connection by index.
    pub fn open_port(&self, port_index: usize) -> anyhow::Result<MidiOutputHandle> {
        let output = MidiOutput::new("harmoniq-midi")?;
        let ports: Vec<MidiOutputPort> = output.ports();
        let Some(port) = ports.get(port_index) else {
            anyhow::bail!("midi port index out of range");
        };
        let name = output
            .port_name(port)
            .unwrap_or_else(|_| format!("Port {port_index}"));
        let conn = output
            .connect(port, "harmoniq-midi-output")
            .map_err(|err| anyhow::anyhow!("failed to open MIDI output: {err}"))?;
        Ok(MidiOutputHandle::new(name, conn))
    }
}

impl Default for MidiOutputManager {
    fn default() -> Self {
        Self::new().expect("failed to initialize MIDI output")
    }
}
