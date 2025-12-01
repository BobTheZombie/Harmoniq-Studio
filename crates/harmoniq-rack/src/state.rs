use std::collections::HashMap;

pub type ChannelId = u32;
pub type PatternId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginFormat {
    Vst3,
    Clap,
}

#[derive(Clone, Debug)]
pub struct MidiRouting {
    pub input_channel: Option<u8>,
    pub output_channel: Option<u8>,
}

impl Default for MidiRouting {
    fn default() -> Self {
        Self {
            input_channel: None,
            output_channel: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstrumentSlot {
    pub id: u32,
    pub name: String,
    pub plugin_uid: String,
    pub format: PluginFormat,
    pub bypass: bool,
    pub oversampling: Option<u32>,
    pub key_range: (u8, u8),
    pub velocity_range: (u8, u8),
    pub midi_routing: MidiRouting,
}

impl InstrumentSlot {
    pub fn new(id: u32, name: impl Into<String>, plugin_uid: String, format: PluginFormat) -> Self {
        Self {
            id,
            name: name.into(),
            plugin_uid,
            format,
            bypass: false,
            oversampling: None,
            key_range: (0, 127),
            velocity_range: (1, 127),
            midi_routing: MidiRouting::default(),
        }
    }

    pub fn bypass(&mut self, bypass: bool) {
        self.bypass = bypass;
    }

    pub fn set_zones(&mut self, key_range: (u8, u8), velocity_range: (u8, u8)) {
        self.key_range = key_range;
        self.velocity_range = velocity_range;
    }

    pub fn set_oversampling(&mut self, factor: Option<u32>) {
        self.oversampling = factor;
    }
}

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
    pub instrument_chain: Vec<InstrumentSlot>,
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
            instrument_chain: Vec::new(),
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

    pub fn add_instrument_slot(
        &mut self,
        channel: ChannelId,
        name: impl Into<String>,
        plugin_uid: String,
        format: PluginFormat,
    ) -> Option<u32> {
        let channel = self.channels.iter_mut().find(|c| c.id == channel)?;
        let next_id = channel
            .instrument_chain
            .iter()
            .map(|slot| slot.id)
            .max()
            .unwrap_or(0)
            + 1;
        channel
            .instrument_chain
            .push(InstrumentSlot::new(next_id, name, plugin_uid, format));
        Some(next_id)
    }

    pub fn configure_instrument_slot(
        &mut self,
        channel: ChannelId,
        slot_id: u32,
        bypass: Option<bool>,
        oversampling: Option<Option<u32>>,
        key_range: Option<(u8, u8)>,
        velocity_range: Option<(u8, u8)>,
        midi_routing: Option<MidiRouting>,
    ) {
        if let Some(slot) = self
            .channels
            .iter_mut()
            .find(|c| c.id == channel)
            .and_then(|c| c.instrument_chain.iter_mut().find(|s| s.id == slot_id))
        {
            if let Some(bypass) = bypass {
                slot.bypass(bypass);
            }
            if let Some(oversampling) = oversampling {
                slot.set_oversampling(oversampling);
            }
            if let Some(key_range) = key_range {
                slot.key_range = key_range;
            }
            if let Some(velocity_range) = velocity_range {
                slot.velocity_range = velocity_range;
            }
            if let Some(midi_routing) = midi_routing {
                slot.midi_routing = midi_routing;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_chain_supports_zones_and_bypass() {
        let mut rack = RackState::new_default();
        let channel = rack.add_channel("Keys".into(), ChannelKind::Instrument, None);
        let slot_id = rack
            .add_instrument_slot(channel, "Piano", "uid://piano".into(), PluginFormat::Clap)
            .unwrap();

        rack.configure_instrument_slot(
            channel,
            slot_id,
            Some(true),
            Some(Some(2)),
            Some((21, 108)),
            Some((10, 120)),
            Some(MidiRouting {
                input_channel: Some(1),
                output_channel: Some(1),
            }),
        );

        let channel_ref = rack.channels.iter().find(|c| c.id == channel).unwrap();
        let slot = channel_ref
            .instrument_chain
            .iter()
            .find(|s| s.id == slot_id)
            .unwrap();
        assert!(slot.bypass);
        assert_eq!(slot.oversampling, Some(2));
        assert_eq!(slot.key_range, (21, 108));
        assert_eq!(slot.velocity_range, (10, 120));
        assert_eq!(slot.midi_routing.input_channel, Some(1));
    }
}
