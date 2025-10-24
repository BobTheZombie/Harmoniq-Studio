use std::collections::VecDeque;

/// Light-weight cache holding the latest parameter/state blobs received from plugins.
#[derive(Debug, Default)]
pub struct PluginDataCache {
    state: Option<Vec<u8>>,
    preset: Option<Vec<u8>>,
    history: VecDeque<PdcEvent>,
    capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdcEvent {
    State(Vec<u8>),
    Preset(Vec<u8>),
}

impl PluginDataCache {
    pub fn new() -> Self {
        Self {
            state: None,
            preset: None,
            history: VecDeque::new(),
            capacity: 16,
        }
    }

    pub fn record_state(&mut self, data: Vec<u8>) {
        self.state = Some(data.clone());
        self.push_event(PdcEvent::State(data));
    }

    pub fn record_preset(&mut self, data: Vec<u8>) {
        self.preset = Some(data.clone());
        self.push_event(PdcEvent::Preset(data));
    }

    pub fn latest_state(&self) -> Option<&[u8]> {
        self.state.as_deref()
    }

    pub fn latest_preset(&self) -> Option<&[u8]> {
        self.preset.as_deref()
    }

    pub fn history(&self) -> impl Iterator<Item = &PdcEvent> {
        self.history.iter()
    }

    fn push_event(&mut self, event: PdcEvent) {
        if self.history.len() == self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(event);
    }
}
