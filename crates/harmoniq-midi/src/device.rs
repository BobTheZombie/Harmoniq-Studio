use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::MidiTimestamp;

/// Unique identifier for a MIDI input device connection.
pub type MidiDeviceId = u64;

/// Configuration for a single MIDI input endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MidiInputConfig {
    /// Whether the device is enabled.
    pub enabled: bool,
    /// Friendly device name as reported by the backend.
    pub name: String,
    /// Backend port index.
    pub port_index: usize,
    /// Optional channel filter (1-16).
    pub channel_filter: Option<u8>,
    /// Enable MIDI Polyphonic Expression handling.
    pub mpe: bool,
    /// Enable channel/aftertouch forwarding.
    pub aftertouch: bool,
    /// Semitone transpose offset (-24..24).
    pub transpose: i8,
    /// Velocity curve preset index.
    pub velocity_curve: u8,
    /// Optional routing target (channel rack id).
    pub route_to_channel: Option<u32>,
}

impl Default for MidiInputConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: String::new(),
            port_index: 0,
            channel_filter: None,
            mpe: false,
            aftertouch: true,
            transpose: 0,
            velocity_curve: 2,
            route_to_channel: None,
        }
    }
}

/// MIDI message container.
#[derive(Clone, Debug)]
pub enum MidiMessage {
    /// Raw channel voice message.
    Raw([u8; 3]),
    /// System exclusive message payload.
    SysEx(Vec<u8>),
}

/// Event timestamped by the backend.
#[derive(Clone, Debug)]
pub struct MidiEvent {
    /// Timestamp relative to a monotonic clock.
    pub ts: MidiTimestamp,
    /// MIDI payload.
    pub msg: MidiMessage,
}

/// Description of the originating source for an event.
#[derive(Clone, Debug)]
pub enum MidiSource {
    /// Hardware device with identifier.
    Device { id: MidiDeviceId, name: Arc<str> },
    /// QWERTY keyboard emulation.
    Qwerty,
    /// Virtual device for tests.
    Virtual,
}

/// Backend abstraction for platform specific MIDI implementations.
pub trait MidiBackend: Send {
    /// Enumerate available input port names.
    fn enumerate(&self) -> anyhow::Result<Vec<String>>;

    /// Open an input port and start delivering events to the callback.
    fn open_input(
        &mut self,
        port_index: usize,
        cb: MidiCallback,
        user: *mut core::ffi::c_void,
    ) -> anyhow::Result<MidiDeviceId>;

    /// Close a previously opened input.
    fn close_input(&mut self, id: MidiDeviceId);
}

/// Callback signature for backend delivered events.
pub type MidiCallback = fn(MidiSource, MidiEvent, *mut core::ffi::c_void);

struct ManagedDevice {
    id: MidiDeviceId,
    name: Arc<str>,
    port_index: usize,
}

/// Device manager responsible for configuration persistence and backend coordination.
pub struct MidiDeviceManager<B: MidiBackend> {
    backend: B,
    cfg: Vec<MidiInputConfig>,
    open: HashMap<MidiDeviceId, ManagedDevice>,
    next_id: MidiDeviceId,
}

impl<B: MidiBackend> MidiDeviceManager<B> {
    /// Create a new manager from the provided backend implementation.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            cfg: Vec::new(),
            open: HashMap::new(),
            next_id: 1,
        }
    }

    /// Access the backend instance.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the backend instance.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Enumerate available input port names.
    pub fn list_ports(&self) -> anyhow::Result<Vec<String>> {
        self.backend.enumerate()
    }

    /// Retrieve persisted configuration.
    pub fn config(&self) -> &[MidiInputConfig] {
        &self.cfg
    }

    /// Replace configuration entries.
    pub fn set_config(&mut self, cfg: Vec<MidiInputConfig>) {
        self.cfg = cfg;
    }

    /// Ensure a connection for the given configuration entry.
    pub fn ensure_connection(
        &mut self,
        config_index: usize,
        cb: MidiCallback,
        user: *mut core::ffi::c_void,
    ) -> anyhow::Result<Option<MidiDeviceId>> {
        let Some(cfg) = self.cfg.get(config_index) else {
            anyhow::bail!("invalid midi config index");
        };
        if !cfg.enabled {
            return Ok(None);
        }

        if let Some(existing) = self
            .open
            .values()
            .find(|entry| entry.port_index == cfg.port_index)
        {
            return Ok(Some(existing.id));
        }

        let id = self
            .backend
            .open_input(cfg.port_index, cb, user)
            .context("failed to open midi input")?;
        let device = ManagedDevice {
            id,
            name: Arc::from(cfg.name.clone()),
            port_index: cfg.port_index,
        };
        self.open.insert(id, device);
        Ok(Some(id))
    }

    /// Close a connection by id.
    pub fn close(&mut self, id: MidiDeviceId) {
        self.backend.close_input(id);
        self.open.remove(&id);
    }

    /// Close all connections.
    pub fn close_all(&mut self) {
        let ids: Vec<_> = self.open.keys().copied().collect();
        for id in ids {
            self.close(id);
        }
    }

    /// Iterate over open connections.
    pub fn open_devices(&self) -> impl Iterator<Item = (&MidiDeviceId, &ManagedDevice)> {
        self.open.iter()
    }

    /// Allocate a new unique identifier.
    pub fn allocate_id(&mut self) -> MidiDeviceId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl<B: MidiBackend> Drop for MidiDeviceManager<B> {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Thread safe wrapper for a device manager.
pub type SharedMidiManager<B> = Arc<Mutex<MidiDeviceManager<B>>>;

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyBackend {
        pub opened: Vec<usize>;
    }

    impl Default for DummyBackend {
        fn default() -> Self {
            Self { opened: Vec::new() }
        }
    }

    impl MidiBackend for DummyBackend {
        fn enumerate(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["PortA".into(), "PortB".into()])
        }

        fn open_input(
            &mut self,
            port_index: usize,
            _cb: MidiCallback,
            _user: *mut core::ffi::c_void,
        ) -> anyhow::Result<MidiDeviceId> {
            self.opened.push(port_index);
            Ok(self.opened.len() as MidiDeviceId)
        }

        fn close_input(&mut self, id: MidiDeviceId) {
            if let Some(pos) = (id as usize).checked_sub(1) {
                if pos < self.opened.len() {
                    self.opened.remove(pos);
                }
            }
        }
    }

    #[test]
    fn ensure_connection_respects_enabled_flag() {
        let mut manager = MidiDeviceManager::new(DummyBackend::default());
        manager.set_config(vec![MidiInputConfig {
            enabled: false,
            name: "Test".into(),
            port_index: 0,
            channel_filter: None,
            mpe: false,
            aftertouch: false,
            transpose: 0,
            velocity_curve: 0,
            route_to_channel: None,
        }]);

        let id = manager
            .ensure_connection(0, |_src, _ev, _user| {}, core::ptr::null_mut())
            .unwrap();
        assert!(id.is_none());

        let mut cfg = manager.config().to_vec();
        cfg[0].enabled = true;
        manager.set_config(cfg);
        let id = manager
            .ensure_connection(0, |_src, _ev, _user| {}, core::ptr::null_mut())
            .unwrap();
        assert!(id.is_some());
    }
}
