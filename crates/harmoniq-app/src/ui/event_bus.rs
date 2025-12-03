use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread::{self, ThreadId};

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
    PreviewStockSound(String),
    Layout(LayoutEvent),
    OpenFile(PathBuf),
    SaveProject,
    RequestRepaint,
    OpenAudioSettings,
    OpenChannelPianoRoll { channel_index: usize },
    OpenPianoRollPattern { pattern_id: u32, clip_name: String },
}

#[derive(Debug)]
struct EventBusInner {
    ui_thread: ThreadId,
    events: RefCell<Vec<AppEvent>>,
}

#[derive(Clone)]
pub struct EventBus {
    inner: Rc<EventBusInner>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(EventBusInner {
                ui_thread: thread::current().id(),
                events: RefCell::new(Vec::new()),
            }),
        }
    }

    fn assert_ui_thread(&self) {
        let current = thread::current().id();
        assert_eq!(
            current, self.inner.ui_thread,
            "UI event accessed from a non-UI thread"
        );
    }

    pub fn publish(&self, event: AppEvent) {
        self.assert_ui_thread();
        self.inner.events.borrow_mut().push(event);
    }

    pub fn drain(&self) -> Vec<AppEvent> {
        self.assert_ui_thread();
        self.inner.events.borrow_mut().drain(..).collect()
    }
}
