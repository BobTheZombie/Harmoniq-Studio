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
pub struct Channel {
    pub id: ChannelId,
    pub name: String,
    pub kind: ChannelKind,
    pub target_plugin_uid: Option<String>,
    pub mute: bool,
    pub solo: bool,
    pub gain_db: f32,
    pub steps_per_bar: u32,
    pub steps: HashMap<PatternId, Vec<bool>>,
}

impl Channel {
    fn new(
        id: ChannelId,
        name: String,
        kind: ChannelKind,
        target_plugin_uid: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            kind,
            target_plugin_uid,
            mute: false,
            solo: false,
            gain_db: 0.0,
            steps_per_bar: 16,
            steps: HashMap::new(),
        }
    }
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

    pub fn insert(&mut self, mut note: MidiNote) -> NoteId {
        let id = self.next_id;
        self.next_id += 1;
        note.id = id;
        self.notes.insert(id, note);
        id
    }

    pub fn insert_many<I>(&mut self, notes: I) -> Vec<NoteId>
    where
        I: IntoIterator<Item = MidiNote>,
    {
        notes.into_iter().map(|note| self.insert(note)).collect()
    }

    pub fn remove_many(&mut self, note_ids: &[NoteId]) {
        for id in note_ids {
            self.notes.remove(id);
        }
    }

    pub fn set_note_velocity(&mut self, note_id: NoteId, velocity: u8) -> bool {
        if let Some(note) = self.notes.get_mut(&note_id) {
            note.velocity = velocity;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.notes.clear();
        self.next_id = 1;
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: PatternId,
    pub name: String,
    pub bars: u32,
    pub clip_per_channel: HashMap<ChannelId, MidiClip>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    pub current_pattern: PatternId,
}

impl Session {
    pub fn new_empty() -> Self {
        let mut session = Self {
            channels: Vec::new(),
            patterns: Vec::new(),
            current_pattern: 1,
        };
        session.patterns.push(Pattern {
            id: 1,
            name: "Pattern 1".to_string(),
            bars: 1,
            clip_per_channel: HashMap::new(),
        });
        session
    }

    fn next_channel_id(&self) -> ChannelId {
        self.channels
            .iter()
            .map(|channel| channel.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    fn next_pattern_id(&self) -> PatternId {
        self.patterns
            .iter()
            .map(|pattern| pattern.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    fn channel_mut(&mut self, channel_id: ChannelId) -> Option<&mut Channel> {
        self.channels
            .iter_mut()
            .find(|channel| channel.id == channel_id)
    }

    fn pattern_mut(&mut self, pattern_id: PatternId) -> Option<&mut Pattern> {
        self.patterns
            .iter_mut()
            .find(|pattern| pattern.id == pattern_id)
    }

    pub fn add_pattern(&mut self) -> PatternId {
        let id = self.next_pattern_id();
        let pattern = Pattern {
            id,
            name: format!("Pattern {}", id),
            bars: 1,
            clip_per_channel: HashMap::new(),
        };
        self.patterns.push(pattern);
        id
    }

    pub fn add_instrument_channel(
        &mut self,
        name: String,
        plugin_uid: Option<String>,
    ) -> ChannelId {
        let id = self.next_channel_id();
        let channel = Channel::new(id, name, ChannelKind::Instrument, plugin_uid);
        self.channels.push(channel);
        id
    }

    pub fn add_sample_channel(&mut self, name: String) -> ChannelId {
        let id = self.next_channel_id();
        let channel = Channel::new(id, name, ChannelKind::Sample, None);
        self.channels.push(channel);
        id
    }

    pub fn remove_channel(&mut self, channel_id: ChannelId) {
        self.channels.retain(|channel| channel.id != channel_id);
        for pattern in &mut self.patterns {
            pattern.clip_per_channel.remove(&channel_id);
        }
    }

    pub fn clone_channel(&mut self, channel_id: ChannelId) -> Option<ChannelId> {
        let next_id = self.next_channel_id();
        let cloned_channel = self
            .channels
            .iter()
            .find(|channel| channel.id == channel_id)
            .cloned()?;
        let mut new_channel = cloned_channel;
        new_channel.id = next_id;
        self.channels.push(new_channel);
        for pattern in &mut self.patterns {
            if let Some(clip) = pattern.clip_per_channel.get(&channel_id).cloned() {
                pattern.clip_per_channel.insert(next_id, clip);
            }
        }
        Some(next_id)
    }

    pub fn ensure_steps(&mut self, pattern_id: PatternId, channel_id: ChannelId) -> &mut Vec<bool> {
        let channel = self
            .channel_mut(channel_id)
            .expect("Channel should exist when ensuring steps");
        channel
            .steps
            .entry(pattern_id)
            .or_insert_with(|| vec![false; channel.steps_per_bar as usize])
    }

    pub fn clip_mut(&mut self, pattern_id: PatternId, channel_id: ChannelId) -> &mut MidiClip {
        let pattern = self
            .pattern_mut(pattern_id)
            .expect("Pattern should exist when requesting clip");
        pattern
            .clip_per_channel
            .entry(channel_id)
            .or_insert_with(|| MidiClip::new(480))
    }

    pub fn pattern_exists(&self, pattern_id: PatternId) -> bool {
        self.patterns.iter().any(|pattern| pattern.id == pattern_id)
    }

    pub fn channel_exists(&self, channel_id: ChannelId) -> bool {
        self.channels.iter().any(|channel| channel.id == channel_id)
    }
}
