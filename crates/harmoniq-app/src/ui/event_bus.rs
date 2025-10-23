use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::TimeSignature;

#[derive(Debug, Clone)]
pub enum TransportEvent {
    Play,
    Stop,
    Record(bool),
}

#[derive(Debug, Clone)]
pub enum LayoutEvent {
    ToggleBrowser,
    ResetWorkspace,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Transport(TransportEvent),
    SetTempo(f32),
    SetTimeSignature(TimeSignature),
    ToggleMetronome,
    TogglePatternMode,
    Layout(LayoutEvent),
    OpenFile(PathBuf),
    SaveProject,
    RequestRepaint,
    OpenAudioSettings,
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<Mutex<Vec<AppEvent>>>,
}

impl EventBus {
    pub fn publish(&self, event: AppEvent) {
        let mut events = self.inner.lock().unwrap();
        events.push(event);
    }

    pub fn drain(&self) -> Vec<AppEvent> {
        let mut events = self.inner.lock().unwrap();
        events.drain(..).collect()
    }
}
