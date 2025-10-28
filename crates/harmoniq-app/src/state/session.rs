use std::collections::{BTreeMap, HashMap};

pub type ChannelId = u32;
pub type PatternId = u32;
pub type NoteId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Instrument,
    Sample,
    Effect,
}

#[derive(Debug, Clone)]
pub struct MidiNote {
    pub id: NoteId,
    pub start_ticks: u32,
    pub length_ticks: u32,
    pub key: i8,
    pub velocity: u8,
}

#[derive(Debug, Clone)]
pub struct MidiClip {
    pub ppq: u32,
    pub notes: BTreeMap<NoteId, MidiNote>,
    pub next_id: NoteId,
}

impl MidiClip {
    pub fn new(ppq: u32) -> Self {
        Self {
            ppq,
            notes: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn insert_note(
        &mut self,
        start_ticks: u32,
        length_ticks: u32,
        key: i8,
        velocity: u8,
    ) -> NoteId {
        let id = self.next_id;
        self.next_id += 1;
        let note = MidiNote {
            id,
            start_ticks,
            length_ticks,
            key,
            velocity,
        };
        self.notes.insert(id, note);
        id
    }

    pub fn remove_note(&mut self, note_id: NoteId) {
        self.notes.remove(&note_id);
    }
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub id: ChannelId,
    pub name: String,
    pub kind: ChannelKind,
    pub target_plugin_uid: Option<String>,
    pub mute: bool,
    pub solo: bool,
    pub gain_db: f32,
    pub steps_per_bar: usize,
    pub steps: HashMap<PatternId, Vec<bool>>,
}

impl Channel {
    pub fn new(id: ChannelId, name: String, kind: ChannelKind) -> Self {
        Self {
            id,
            name,
            kind,
            target_plugin_uid: None,
            mute: false,
            solo: false,
            gain_db: 0.0,
            steps_per_bar: 16,
            steps: HashMap::new(),
        }
    }

    pub fn steps_for_pattern_mut(&mut self, pattern_id: PatternId) -> Option<&mut Vec<bool>> {
        self.steps.get_mut(&pattern_id)
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: PatternId,
    pub name: String,
    pub bars: u32,
    pub clip_per_channel: HashMap<ChannelId, MidiClip>,
}

impl Pattern {
    pub fn new(id: PatternId, name: String, bars: u32) -> Self {
        Self {
            id,
            name,
            bars,
            clip_per_channel: HashMap::new(),
        }
    }

    pub fn ensure_clip(&mut self, channel_id: ChannelId) -> &mut MidiClip {
        self.clip_per_channel
            .entry(channel_id)
            .or_insert_with(|| MidiClip::new(960))
    }

    pub fn clip_mut(&mut self, channel_id: ChannelId) -> Option<&mut MidiClip> {
        self.clip_per_channel.get_mut(&channel_id)
    }

    pub fn clip(&self, channel_id: ChannelId) -> Option<&MidiClip> {
        self.clip_per_channel.get(&channel_id)
    }

    pub fn total_16th_steps(&self) -> usize {
        (self.bars * 16) as usize
    }
}

#[derive(Debug, Default)]
pub struct Session {
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    next_channel_id: ChannelId,
    next_pattern_id: PatternId,
}

impl Session {
    pub fn new_empty() -> Self {
        let mut session = Self {
            channels: Vec::new(),
            patterns: Vec::new(),
            next_channel_id: 1,
            next_pattern_id: 1,
        };
        session.add_pattern("Pattern 1".into(), 1);
        session
    }

    pub fn add_pattern(&mut self, name: String, bars: u32) -> PatternId {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        let pattern = Pattern::new(id, name, bars);
        self.patterns.push(pattern);
        for channel in &mut self.channels {
            self.ensure_steps(channel.id, id);
        }
        id
    }

    pub fn add_instrument_channel(
        &mut self,
        name: String,
        plugin_uid: Option<String>,
    ) -> ChannelId {
        self.add_channel(name, ChannelKind::Instrument, plugin_uid)
    }

    pub fn add_sample_channel(&mut self, name: String, path: String) -> ChannelId {
        let mut channel = self.add_channel(name, ChannelKind::Sample, Some(path.clone()));
        if let Some(ch) = self.channel_mut(channel) {
            ch.target_plugin_uid = Some(path);
        }
        channel
    }

    fn add_channel(
        &mut self,
        name: String,
        kind: ChannelKind,
        plugin_uid: Option<String>,
    ) -> ChannelId {
        let id = self.next_channel_id;
        self.next_channel_id += 1;
        let mut channel = Channel::new(id, name, kind);
        channel.target_plugin_uid = plugin_uid;
        self.channels.push(channel);
        for pattern in &self.patterns {
            self.ensure_steps(id, pattern.id);
        }
        id
    }

    pub fn remove_channel(&mut self, channel_id: ChannelId) {
        self.channels.retain(|channel| channel.id != channel_id);
        for pattern in &mut self.patterns {
            pattern.clip_per_channel.remove(&channel_id);
        }
    }

    pub fn clone_channel(&mut self, channel_id: ChannelId) -> Option<ChannelId> {
        let original = self
            .channels
            .iter()
            .find(|channel| channel.id == channel_id)?
            .clone();
        let mut cloned = original.clone();
        cloned.id = self.next_channel_id;
        self.next_channel_id += 1;
        let new_id = cloned.id;
        self.channels.push(cloned);

        for pattern in &mut self.patterns {
            if let Some(clip) = pattern.clip_per_channel.get(&channel_id).cloned() {
                pattern.clip_per_channel.insert(new_id, clip);
            }
        }

        Some(new_id)
    }

    pub fn ensure_steps(&mut self, channel_id: ChannelId, pattern_id: PatternId) {
        let total_steps = self
            .pattern(pattern_id)
            .map(|pattern| pattern.total_16th_steps())
            .unwrap_or(16);
        if let Some(channel) = self.channel_mut(channel_id) {
            channel
                .steps
                .entry(pattern_id)
                .or_insert_with(|| vec![false; total_steps])
                .resize(total_steps, false);
        }
    }

    pub fn channel(&self, channel_id: ChannelId) -> Option<&Channel> {
        self.channels
            .iter()
            .find(|channel| channel.id == channel_id)
    }

    pub fn channel_mut(&mut self, channel_id: ChannelId) -> Option<&mut Channel> {
        self.channels
            .iter_mut()
            .find(|channel| channel.id == channel_id)
    }

    pub fn pattern(&self, pattern_id: PatternId) -> Option<&Pattern> {
        self.patterns
            .iter()
            .find(|pattern| pattern.id == pattern_id)
    }

    pub fn pattern_mut(&mut self, pattern_id: PatternId) -> Option<&mut Pattern> {
        self.patterns
            .iter_mut()
            .find(|pattern| pattern.id == pattern_id)
    }

    pub fn clip_mut(
        &mut self,
        channel_id: ChannelId,
        pattern_id: PatternId,
    ) -> Option<&mut MidiClip> {
        let pattern = self.pattern_mut(pattern_id)?;
        Some(pattern.ensure_clip(channel_id))
    }

    pub fn clip(&self, channel_id: ChannelId, pattern_id: PatternId) -> Option<&MidiClip> {
        self.pattern(pattern_id)?.clip(channel_id)
    }
}
