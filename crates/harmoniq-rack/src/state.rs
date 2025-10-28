use std::collections::HashMap;

pub type ChannelId = u32;
pub type PatternId = u32;
pub type StepIndex = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Instrument,
    Sample,
    Effect,
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
    pub mute: bool,
    pub solo: bool,
    pub gain_db: f32,
    pub swing: f32,
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
            mute: false,
            solo: false,
            gain_db: 0.0,
            swing: 0.0,
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
        let next_id = self.patterns.iter().map(|pat| pat.id).max().unwrap_or(0) + 1;
        let name = format!("Pattern {}", next_id);
        self.patterns.push(PatternMeta {
            id: next_id,
            name,
            bars: 1,
        });
        next_id
    }

    pub fn add_channel(
        &mut self,
        name: String,
        kind: ChannelKind,
        plugin_uid: Option<String>,
    ) -> ChannelId {
        let next_id = self.channels.iter().map(|ch| ch.id).max().unwrap_or(0) + 1;
        let channel = Channel::new(next_id, name, kind, plugin_uid);
        self.channels.push(channel);
        next_id
    }

    pub fn steps_mut(&mut self, pat: PatternId, ch: ChannelId) -> &mut Vec<Step> {
        let pattern_bars = self
            .patterns
            .iter()
            .find(|pattern| pattern.id == pat)
            .map(|pattern| pattern.bars)
            .unwrap_or_else(|| {
                self.patterns.push(PatternMeta {
                    id: pat,
                    name: format!("Pattern {}", pat),
                    bars: 1,
                });
                1
            });

        let total_steps = {
            let steps_per_bar = self
                .channels
                .iter()
                .find(|channel| channel.id == ch)
                .map(|channel| channel.steps_per_bar)
                .unwrap_or(16);
            (pattern_bars * steps_per_bar) as usize
        };

        let channel = self
            .channels
            .iter_mut()
            .find(|channel| channel.id == ch)
            .expect("channel must exist");

        let steps = channel
            .steps
            .entry(pat)
            .or_insert_with(|| vec![Step::default(); total_steps]);
        if steps.len() != total_steps {
            steps.resize(total_steps, Step::default());
        }
        steps
    }
}
