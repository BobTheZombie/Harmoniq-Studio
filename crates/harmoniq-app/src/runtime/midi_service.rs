use std::ffi::c_void;
use std::sync::Arc;

use harmoniq_engine::sched::events::EventLane;
use harmoniq_midi::backend_midir::MidirBackend;
use harmoniq_midi::clock::MidiClock;
use harmoniq_midi::config::{self, MidiSettings};
use harmoniq_midi::device::{MidiDeviceManager, MidiEvent, MidiMessage, MidiSource};
use parking_lot::Mutex;
use tracing::trace;

/// Shared handle for emitting MIDI into the audio engine.
#[derive(Clone)]
pub struct MidiEventSender {
    lane: Arc<EventLane>,
}

impl MidiEventSender {
    /// Create a new sender from the engine lane.
    pub fn new(lane: Arc<EventLane>) -> Self {
        Self { lane }
    }

    /// Push a MIDI event into the lane.
    pub fn push(&self, bytes: [u8; 3], sample_offset: u32) {
        let _ = self.lane.push_midi(bytes, sample_offset);
    }

    /// Access the raw lane reference.
    pub fn lane(&self) -> Arc<EventLane> {
        Arc::clone(&self.lane)
    }
}

/// High-level MIDI service coordinating hardware inputs and routing.
pub struct MidiService {
    manager: Arc<Mutex<MidiDeviceManager<MidirBackend>>>,
    settings: MidiSettings,
    clock: MidiClock,
    sender: MidiEventSender,
}

impl MidiService {
    /// Create a new service.
    pub fn new(sample_rate: u32, lane: Arc<EventLane>) -> Self {
        let mut manager = MidiDeviceManager::new(MidirBackend::default());
        let settings = config::load();
        config::apply_settings(&mut manager, &settings);
        let manager = Arc::new(Mutex::new(manager));
        Self {
            manager,
            settings,
            clock: MidiClock::new(sample_rate),
            sender: MidiEventSender::new(lane),
        }
    }

    /// Handle a MIDI event emitted by the backend.
    pub fn handle_backend_event(
        &mut self,
        source: MidiSource,
        event: MidiEvent,
        _user: *mut c_void,
    ) {
        if let MidiMessage::Raw(bytes) = event.msg {
            let sample = self.clock.to_block_sample(event.ts.nanos_monotonic, 0, 128);
            self.sender.push(bytes, sample);
            trace!(?source, sample, "midi event queued");
        }
    }

    /// Access the current settings snapshot.
    pub fn settings(&self) -> &MidiSettings {
        &self.settings
    }

    /// Update settings and persist them.
    pub fn update_settings(&mut self, new_settings: MidiSettings) {
        if new_settings != self.settings {
            self.settings = new_settings;
            if let Some(mut mgr) = self.manager.try_lock() {
                config::apply_settings(&mut mgr, &self.settings);
            }
            config::save(&self.settings);
        }
    }

    /// Periodic maintenance.
    pub fn tick(&mut self) {
        if let Some(mgr) = self.manager.try_lock() {
            let mut snapshot = self.settings.clone();
            config::capture_settings(&mgr, &mut snapshot);
            if snapshot != self.settings {
                self.settings = snapshot;
            }
        }
    }

    /// Access the underlying manager.
    pub fn manager(&self) -> Arc<Mutex<MidiDeviceManager<MidirBackend>>> {
        Arc::clone(&self.manager)
    }

    /// Access the clock used for timestamp translation.
    pub fn clock(&mut self) -> &mut MidiClock {
        &mut self.clock
    }

    /// Access the sender delivering events to the engine.
    pub fn sender(&self) -> MidiEventSender {
        self.sender.clone()
    }
}

/// Dispatch function passed to the backend.
pub fn backend_dispatch(source: MidiSource, event: MidiEvent, user: *mut c_void) {
    if let Some(service) = unsafe { (user as *mut MidiService).as_mut() } {
        service.handle_backend_event(source, event, user);
    }
}
