use std::collections::HashMap;

pub type ChannelId = u32;
pub type PatternId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Instrument,
    Sample,
    Automation,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub on: bool,
    pub velocity: u8,
    pub pan: i8,
    pub shift_ticks: i16,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            on: false,
            velocity: 100,
            pan: 0,
            shift_ticks: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub id: ChannelId,
    pub name: String,
    pub kind: ChannelKind,
    pub plugin_uid: Option<String>,
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub steps_per_bar: u32,
    pub steps: HashMap<PatternId, Vec<Step>>,
}

impl Channel {
    pub fn new(id: ChannelId, name: String, kind: ChannelKind, plugin_uid: Option<String>) -> Self {
        Self {
            id,
            name,
            kind,
            plugin_uid,
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            steps_per_bar: 16,
            steps: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PatternMeta {
    pub id: PatternId,
    pub name: String,
    pub bars: u32,
}

#[derive(Default)]
pub struct RackState {
    pub patterns: Vec<PatternMeta>,
    pub current_pattern: PatternId,
    pub channels: Vec<Channel>,
}

impl RackState {
    pub fn new_default() -> Self {
        let mut state = Self::default();
        state.current_pattern = 1;
        state.patterns.push(PatternMeta {
            id: 1,
            name: "Pattern 1".to_string(),
            bars: 1,
        });
        state
    }

    pub fn add_pattern(&mut self) -> PatternId {
        let id = self.patterns.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        self.patterns.push(PatternMeta {
            id,
            name: format!("Pattern {id}"),
            bars: 1,
        });
        self.current_pattern = id;
        id
    }

    pub fn add_channel(
        &mut self,
        name: String,
        kind: ChannelKind,
        plugin_uid: Option<String>,
    ) -> ChannelId {
        let id = self.channels.iter().map(|ch| ch.id).max().unwrap_or(0) + 1;
        self.channels.push(Channel::new(id, name, kind, plugin_uid));
        id
    }

    pub fn remove_channel(&mut self, id: ChannelId) {
        self.channels.retain(|c| c.id != id);
    }

    pub fn steps_mut(&mut self, pat: PatternId, ch: ChannelId) -> &mut Vec<Step> {
        let steps_per_bar = self
            .channels
            .iter()
            .find(|c| c.id == ch)
            .map(|c| c.steps_per_bar as usize)
            .expect("channel must exist");

        let channel = self
            .channels
            .iter_mut()
            .find(|c| c.id == ch)
            .expect("channel must exist");

        let steps = channel
            .steps
            .entry(pat)
            .or_insert_with(|| vec![Step::default(); steps_per_bar]);

        if steps.len() != steps_per_bar {
            steps.resize(steps_per_bar, Step::default());
        }

        steps
    }
}
