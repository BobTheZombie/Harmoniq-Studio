use std::f32::consts::{PI, TAU};
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

use anyhow::{anyhow, Context};
use harmoniq_engine::{
    AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, MidiEvent, MidiProcessor,
    PluginDescriptor,
};
use harmoniq_plugin_sdk::{
    NativePlugin, ParameterDefinition, ParameterId, ParameterKind, ParameterLayout, ParameterSet,
    ParameterValue, PluginFactory, PluginParameterError,
};
use rand::{Rng, SeedableRng};
use symphonia::core::audio::{SampleBuffer as SymphoniaSampleBuffer, SignalSpec};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const MAX_CHANNELS: usize = 2;

/// Simplified ADSR envelope shared by several built-in instruments.
#[derive(Debug, Clone)]
struct AdsrEnvelope {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    sample_rate: f32,
    state: EnvelopeState,
    value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Default for AdsrEnvelope {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
            sample_rate: 44_100.0,
            state: EnvelopeState::Idle,
            value: 0.0,
        }
    }
}

impl AdsrEnvelope {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    fn set_params(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack = attack.max(0.0001);
        self.decay = decay.max(0.0001);
        self.sustain = sustain.clamp(0.0, 1.0);
        self.release = release.max(0.0001);
    }

    fn trigger(&mut self) {
        self.state = EnvelopeState::Attack;
        self.value = 0.0;
    }

    fn release(&mut self) {
        if self.state != EnvelopeState::Idle {
            self.state = EnvelopeState::Release;
        }
    }

    fn next(&mut self) -> f32 {
        match self.state {
            EnvelopeState::Idle => {
                self.value = 0.0;
            }
            EnvelopeState::Attack => {
                let increment = 1.0 / (self.attack * self.sample_rate);
                self.value += increment;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.state = EnvelopeState::Decay;
                }
            }
            EnvelopeState::Decay => {
                let decrement = (1.0 - self.sustain) / (self.decay * self.sample_rate);
                self.value -= decrement;
                if self.value <= self.sustain {
                    self.value = self.sustain;
                    self.state = EnvelopeState::Sustain;
                }
            }
            EnvelopeState::Sustain => {
                self.value = self.sustain;
            }
            EnvelopeState::Release => {
                let decrement = 1.0 / (self.release * self.sample_rate);
                self.value -= decrement;
                if self.value <= 0.0 {
                    self.value = 0.0;
                    self.state = EnvelopeState::Idle;
                }
            }
        }
        self.value.clamp(0.0, 1.0)
    }

    fn is_active(&self) -> bool {
        self.state != EnvelopeState::Idle || self.value > 0.0
    }
}

fn midi_note_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

fn fill_buffer(buffer: &mut AudioBuffer, mut render: impl FnMut() -> f32) {
    let samples = buffer.len();
    let mut values = Vec::with_capacity(samples);
    for _ in 0..samples {
        values.push(render());
    }
    for channel in buffer.channels_mut() {
        for (sample, value) in channel.iter_mut().zip(values.iter()) {
            *sample = *value;
        }
    }
}

// --- Analog Synth -------------------------------------------------------------------------

const ANALOG_LEVEL: &str = "analog.level";
const ANALOG_ATTACK: &str = "analog.attack";
const ANALOG_DECAY: &str = "analog.decay";
const ANALOG_SUSTAIN: &str = "analog.sustain";
const ANALOG_RELEASE: &str = "analog.release";
const ANALOG_SAW_MIX: &str = "analog.saw_mix";
const ANALOG_SQUARE_MIX: &str = "analog.square_mix";
const ANALOG_CUTOFF: &str = "analog.cutoff";

#[derive(Debug, Clone)]
pub struct AnalogSynth {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    filter_state: f32,
    envelope: AdsrEnvelope,
    parameters: ParameterSet,
}

impl Default for AnalogSynth {
    fn default() -> Self {
        let parameters = ParameterSet::new(analog_layout());
        let mut synth = Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 220.0,
            velocity: 0.0,
            filter_state: 0.0,
            envelope: AdsrEnvelope::default(),
            parameters,
        };
        synth.sync_envelope();
        synth
    }
}

impl AnalogSynth {
    fn sync_envelope(&mut self) {
        let attack = self
            .parameters
            .get(&ParameterId::from(ANALOG_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.01);
        let decay = self
            .parameters
            .get(&ParameterId::from(ANALOG_DECAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.2);
        let sustain = self
            .parameters
            .get(&ParameterId::from(ANALOG_SUSTAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.7);
        let release = self
            .parameters
            .get(&ParameterId::from(ANALOG_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3);
        self.envelope.set_params(attack, decay, sustain, release);
    }

    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(ANALOG_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.7);
        let saw_mix = self
            .parameters
            .get(&ParameterId::from(ANALOG_SAW_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.7);
        let square_mix = self
            .parameters
            .get(&ParameterId::from(ANALOG_SQUARE_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3);
        let cutoff = self
            .parameters
            .get(&ParameterId::from(ANALOG_CUTOFF))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(2_000.0);

        let increment = 2.0 * PI * self.frequency / self.sample_rate;
        self.phase = (self.phase + increment).rem_euclid(2.0 * PI);
        let saw = 1.0 - (self.phase / PI);
        let square = if self.phase < PI { 1.0 } else { -1.0 };
        let mix = saw * saw_mix + square * square_mix;

        let cutoff_norm = (2.0 * PI * cutoff / self.sample_rate).clamp(0.0, 0.99);
        self.filter_state += cutoff_norm * (mix - self.filter_state);
        let env = self.envelope.next();
        if !self.envelope.is_active() {
            self.velocity = 0.0;
        }
        self.filter_state * env * self.velocity * level
    }
}

impl AudioProcessor for AnalogSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.analog", "Analog Synth", "Harmoniq Labs").with_description(
            "Basic subtractive synthesizer with ADSR envelope and single pole filter",
        )
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope.set_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if self.velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for AnalogSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = midi_note_to_freq(*note);
                    self.velocity = *velocity as f32 / 127.0;
                    self.envelope.trigger();
                }
                MidiEvent::NoteOff { .. } => {
                    self.envelope.release();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for AnalogSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        match id.as_str() {
            ANALOG_ATTACK | ANALOG_DECAY | ANALOG_SUSTAIN | ANALOG_RELEASE => {
                self.sync_envelope();
            }
            ANALOG_LEVEL | ANALOG_SAW_MIX | ANALOG_SQUARE_MIX | ANALOG_CUTOFF => {
                let _ = value;
            }
            _ => {}
        }
        Ok(())
    }
}

fn analog_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            ANALOG_LEVEL,
            "Level",
            ParameterKind::continuous(0.0..=1.0, 0.7),
        ),
        ParameterDefinition::new(
            ANALOG_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=2.0, 0.01),
        ),
        ParameterDefinition::new(
            ANALOG_DECAY,
            "Decay",
            ParameterKind::continuous(0.01..=3.0, 0.2),
        ),
        ParameterDefinition::new(
            ANALOG_SUSTAIN,
            "Sustain",
            ParameterKind::continuous(0.0..=1.0, 0.7),
        ),
        ParameterDefinition::new(
            ANALOG_RELEASE,
            "Release",
            ParameterKind::continuous(0.01..=4.0, 0.3),
        ),
        ParameterDefinition::new(
            ANALOG_SAW_MIX,
            "Saw Mix",
            ParameterKind::continuous(0.0..=1.0, 0.7),
        ),
        ParameterDefinition::new(
            ANALOG_SQUARE_MIX,
            "Square Mix",
            ParameterKind::continuous(0.0..=1.0, 0.3),
        ),
        ParameterDefinition::new(
            ANALOG_CUTOFF,
            "Cutoff",
            ParameterKind::continuous(80.0..=6_000.0, 2_000.0),
        )
        .with_unit("Hz"),
    ])
}

pub struct AnalogSynthFactory;

impl PluginFactory for AnalogSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.analog", "Analog Synth", "Harmoniq Labs")
            .with_description("Basic subtractive synthesizer")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(analog_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(AnalogSynth::default())
    }
}

// --- FM Synth ----------------------------------------------------------------------------

const FM_LEVEL: &str = "fm.level";
const FM_ATTACK: &str = "fm.attack";
const FM_DECAY: &str = "fm.decay";
const FM_SUSTAIN: &str = "fm.sustain";
const FM_RELEASE: &str = "fm.release";
const FM_MOD_RATIO: &str = "fm.mod_ratio";
const FM_MOD_INDEX: &str = "fm.mod_index";

#[derive(Debug, Clone)]
pub struct FmSynth {
    sample_rate: f32,
    carrier_phase: f32,
    modulator_phase: f32,
    frequency: f32,
    velocity: f32,
    envelope: AdsrEnvelope,
    parameters: ParameterSet,
}

impl Default for FmSynth {
    fn default() -> Self {
        let parameters = ParameterSet::new(fm_layout());
        let mut synth = Self {
            sample_rate: 44_100.0,
            carrier_phase: 0.0,
            modulator_phase: 0.0,
            frequency: 220.0,
            velocity: 0.0,
            envelope: AdsrEnvelope::default(),
            parameters,
        };
        synth.sync_envelope();
        synth
    }
}

impl FmSynth {
    fn sync_envelope(&mut self) {
        let attack = self
            .parameters
            .get(&ParameterId::from(FM_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.02);
        let decay = self
            .parameters
            .get(&ParameterId::from(FM_DECAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.2);
        let sustain = self
            .parameters
            .get(&ParameterId::from(FM_SUSTAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.6);
        let release = self
            .parameters
            .get(&ParameterId::from(FM_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3);
        self.envelope.set_params(attack, decay, sustain, release);
    }

    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(FM_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let ratio = self
            .parameters
            .get(&ParameterId::from(FM_MOD_RATIO))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(2.0);
        let index = self
            .parameters
            .get(&ParameterId::from(FM_MOD_INDEX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.5);

        let mod_increment = 2.0 * PI * self.frequency * ratio / self.sample_rate;
        self.modulator_phase = (self.modulator_phase + mod_increment).rem_euclid(2.0 * PI);
        let modulator = (self.modulator_phase).sin();

        let carrier_increment = 2.0 * PI * self.frequency / self.sample_rate;
        let instantaneous_phase = self.carrier_phase + modulator * index;
        let sample = instantaneous_phase.sin();
        self.carrier_phase = (self.carrier_phase + carrier_increment).rem_euclid(2.0 * PI);
        let env = self.envelope.next();
        if !self.envelope.is_active() {
            self.velocity = 0.0;
        }
        sample * env * self.velocity * level
    }
}

impl AudioProcessor for FmSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.fm", "FM Synth", "Harmoniq Labs")
            .with_description("Two-operator FM synthesizer")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope.set_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if self.velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for FmSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = midi_note_to_freq(*note);
                    self.velocity = *velocity as f32 / 127.0;
                    self.envelope.trigger();
                }
                MidiEvent::NoteOff { .. } => {
                    self.envelope.release();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for FmSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        _value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        if matches!(id.as_str(), FM_ATTACK | FM_DECAY | FM_SUSTAIN | FM_RELEASE) {
            self.sync_envelope();
        }
        Ok(())
    }
}

fn fm_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(FM_LEVEL, "Level", ParameterKind::continuous(0.0..=1.0, 0.8)),
        ParameterDefinition::new(
            FM_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=2.0, 0.02),
        ),
        ParameterDefinition::new(
            FM_DECAY,
            "Decay",
            ParameterKind::continuous(0.01..=3.0, 0.2),
        ),
        ParameterDefinition::new(
            FM_SUSTAIN,
            "Sustain",
            ParameterKind::continuous(0.0..=1.0, 0.6),
        ),
        ParameterDefinition::new(
            FM_RELEASE,
            "Release",
            ParameterKind::continuous(0.01..=4.0, 0.3),
        ),
        ParameterDefinition::new(
            FM_MOD_RATIO,
            "Mod Ratio",
            ParameterKind::continuous(0.25..=8.0, 2.0),
        ),
        ParameterDefinition::new(
            FM_MOD_INDEX,
            "Mod Index",
            ParameterKind::continuous(0.0..=10.0, 1.5),
        ),
    ])
}

pub struct FmSynthFactory;

impl PluginFactory for FmSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.fm", "FM Synth", "Harmoniq Labs")
            .with_description("Two-operator FM synthesizer")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(fm_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(FmSynth::default())
    }
}

// --- Wavetable Synth ---------------------------------------------------------------------

const WT_LEVEL: &str = "wt.level";
const WT_ATTACK: &str = "wt.attack";
const WT_RELEASE: &str = "wt.release";
const WT_TABLE: &str = "wt.table";

#[derive(Debug, Clone)]
struct Wavetable {
    _name: &'static str,
    data: Vec<f32>,
}

fn create_wavetable(name: &'static str, size: usize, generator: impl Fn(f32) -> f32) -> Wavetable {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        let phase = i as f32 / size as f32;
        data.push(generator(phase));
    }
    Wavetable { _name: name, data }
}

#[derive(Debug, Clone)]
pub struct WavetableSynth {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    envelope: AdsrEnvelope,
    tables: Vec<Wavetable>,
    parameters: ParameterSet,
}

impl Default for WavetableSynth {
    fn default() -> Self {
        let parameters = ParameterSet::new(wavetable_layout());
        let tables = vec![
            create_wavetable("Sine", 2048, |p| (2.0 * PI * p).sin()),
            create_wavetable("Saw", 2048, |p| 2.0 * p - 1.0),
            create_wavetable("Square", 2048, |p| if p < 0.5 { 1.0 } else { -1.0 }),
            create_wavetable("Triangle", 2048, |p| {
                2.0 * (2.0 * (p - (0.5 + p).floor())).abs() - 1.0
            }),
        ];
        let mut synth = Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 220.0,
            velocity: 0.0,
            envelope: AdsrEnvelope::default(),
            tables,
            parameters,
        };
        synth.sync_envelope();
        synth
    }
}

impl WavetableSynth {
    fn sync_envelope(&mut self) {
        let attack = self
            .parameters
            .get(&ParameterId::from(WT_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.01);
        let release = self
            .parameters
            .get(&ParameterId::from(WT_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5);
        self.envelope.set_params(attack, 0.01, 1.0, release);
    }

    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(WT_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let table_index = self
            .parameters
            .get(&ParameterId::from(WT_TABLE))
            .and_then(ParameterValue::as_choice)
            .unwrap_or(0)
            .min(self.tables.len().saturating_sub(1));
        let table = &self.tables[table_index];
        let phase_inc = self.frequency / self.sample_rate;
        self.phase = (self.phase + phase_inc).fract();
        let position = self.phase * table.data.len() as f32;
        let idx = position.floor() as usize;
        let frac = position - idx as f32;
        let next_idx = (idx + 1) % table.data.len();
        let sample = table.data[idx] * (1.0 - frac) + table.data[next_idx] * frac;
        let env = self.envelope.next();
        if !self.envelope.is_active() {
            self.velocity = 0.0;
        }
        sample * env * self.velocity * level
    }
}

impl AudioProcessor for WavetableSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.wavetable", "Wavetable Synth", "Harmoniq Labs")
            .with_description("Table driven synthesizer with morphing")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope.set_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if self.velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for WavetableSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = midi_note_to_freq(*note);
                    self.velocity = *velocity as f32 / 127.0;
                    self.envelope.trigger();
                }
                MidiEvent::NoteOff { .. } => {
                    self.envelope.release();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for WavetableSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        _value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        if matches!(id.as_str(), WT_ATTACK | WT_RELEASE) {
            self.sync_envelope();
        }
        Ok(())
    }
}

fn wavetable_layout() -> ParameterLayout {
    let options = vec![
        "Sine".to_string(),
        "Saw".to_string(),
        "Square".to_string(),
        "Triangle".to_string(),
    ];
    ParameterLayout::new(vec![
        ParameterDefinition::new(WT_LEVEL, "Level", ParameterKind::continuous(0.0..=1.0, 0.8)),
        ParameterDefinition::new(
            WT_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=2.0, 0.01),
        ),
        ParameterDefinition::new(
            WT_RELEASE,
            "Release",
            ParameterKind::continuous(0.01..=4.0, 0.5),
        ),
        ParameterDefinition::new(
            WT_TABLE,
            "Table",
            ParameterKind::Choice {
                options,
                default: 0,
            },
        ),
    ])
}

pub struct WavetableSynthFactory;

impl PluginFactory for WavetableSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.wavetable", "Wavetable Synth", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(wavetable_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(WavetableSynth::default())
    }
}

// --- Sample Loading ----------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SampleBuffer {
    sample_rate: u32,
    channels: Vec<Vec<f32>>,
}

impl SampleBuffer {
    pub fn new(sample_rate: u32, channel_count: usize) -> Self {
        Self {
            sample_rate,
            channels: vec![Vec::new(); channel_count],
        }
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn len(&self) -> usize {
        self.channels.iter().map(|c| c.len()).max().unwrap_or(0)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn is_empty(&self) -> bool {
        self.channels.iter().all(|c| c.is_empty())
    }

    pub fn mixed_sample(&self, index: usize) -> f32 {
        let mut total = 0.0;
        let mut count = 0;
        for channel in &self.channels {
            if let Some(sample) = channel.get(index) {
                total += *sample;
                count += 1;
            }
        }
        if count == 0 {
            0.0
        } else {
            (total / count as f32).clamp(-1.0, 1.0)
        }
    }

    fn append_interleaved(&mut self, data: &[f32], source_channels: usize) {
        if source_channels == 0 {
            return;
        }
        let channels = source_channels.min(MAX_CHANNELS);
        self.ensure_channels_by_count(channels);
        for frame in data.chunks(source_channels) {
            for ch in 0..channels {
                if let Some(sample) = frame.get(ch) {
                    self.channels[ch].push(sample.clamp(-1.0, 1.0));
                }
            }
        }
    }

    fn ensure_channels(&mut self, spec: SignalSpec) {
        let count = spec.channels.count().min(MAX_CHANNELS);
        self.ensure_channels_by_count(count);
    }

    fn ensure_channels_by_count(&mut self, count: usize) {
        if self.channels.len() < count {
            self.channels.resize(count, Vec::new());
        }
    }
}

fn hint_for_path(path: Option<&Path>) -> Hint {
    let mut hint = Hint::new();
    if let Some(ext) = path
        .and_then(|p| p.extension())
        .and_then(|ext| ext.to_str())
    {
        hint.with_extension(ext);
    }
    hint
}

fn decode_sample_stream(
    display_name: &str,
    mss: MediaSourceStream,
    hint: Hint,
) -> anyhow::Result<SampleBuffer> {
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|err| anyhow!("failed to probe {display_name}: {err}"))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default track for {display_name}"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|err| anyhow!("failed to create decoder: {err}"))?;

    let mut sample_buffer = SampleBuffer::new(
        track
            .codec_params
            .sample_rate
            .ok_or_else(|| anyhow!("missing sample rate for {display_name}"))?,
        track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(1)
            .min(MAX_CHANNELS),
    );
    let mut scratch: Option<SymphoniaSampleBuffer<f32>> = None;

    loop {
        match format.next_packet() {
            Ok(packet) => match decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let channels = spec.channels.count();
                    if scratch.is_none() {
                        scratch = Some(SymphoniaSampleBuffer::<f32>::new(
                            decoded.capacity() as u64,
                            spec,
                        ));
                    }
                    if let Some(buf) = scratch.as_mut() {
                        buf.copy_interleaved_ref(decoded);
                        sample_buffer.ensure_channels(spec);
                        sample_buffer.append_interleaved(buf.samples(), channels);
                    }
                }
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    decoder.reset();
                }
                Err(err) => return Err(anyhow!("decode error for {display_name}: {err}")),
            },
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
            }
            Err(err) => return Err(anyhow!("format error for {display_name}: {err}")),
        }
    }

    Ok(sample_buffer)
}

pub fn load_sample_from_file(path: impl AsRef<Path>) -> anyhow::Result<SampleBuffer> {
    let path_ref = path.as_ref();
    let display_name = path_ref.display().to_string();
    let file = File::open(path_ref).with_context(|| format!("failed to open {:?}", path_ref))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let hint = hint_for_path(Some(path_ref));
    decode_sample_stream(&display_name, mss, hint)
}

pub async fn load_sample_from_file_async(path: impl AsRef<Path>) -> anyhow::Result<SampleBuffer> {
    use async_fs::read;

    let path_buf = path.as_ref().to_path_buf();
    let display_name = path_buf.display().to_string();
    let bytes = read(&path_buf)
        .await
        .with_context(|| format!("failed to read {:?}", path_buf))?;
    let cursor = Cursor::new(bytes);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let hint = hint_for_path(Some(&path_buf));
    decode_sample_stream(&display_name, mss, hint)
}

// --- Sampler / Drum Machine --------------------------------------------------------------

const SAMPLER_LEVEL: &str = "sampler.level";
const SAMPLER_START: &str = "sampler.start";

#[derive(Debug, Clone)]
pub struct Sampler {
    sample_rate: f32,
    position: usize,
    active: bool,
    sample: SampleBuffer,
    parameters: ParameterSet,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            sample_rate: 44_100.0,
            position: 0,
            active: false,
            sample: SampleBuffer::default(),
            parameters: ParameterSet::new(sampler_layout()),
        }
    }
}

impl Sampler {
    pub fn load_sample(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.sample = load_sample_from_file(path)?;
        Ok(())
    }

    pub async fn load_sample_async(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.sample = load_sample_from_file_async(path).await?;
        Ok(())
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active || self.sample.is_empty() {
            return 0.0;
        }
        let start_offset = self
            .parameters
            .get(&ParameterId::from(SAMPLER_START))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.0)
            * self.sample.len() as f32;
        let start = start_offset as usize;
        if self.position + start >= self.sample.len() {
            self.active = false;
            self.position = 0;
            return 0.0;
        }
        let level = self
            .parameters
            .get(&ParameterId::from(SAMPLER_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0);
        let value = self.sample.mixed_sample(self.position + start);
        self.position += 1;
        value * level
    }
}

impl AudioProcessor for Sampler {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sampler", "Sampler", "Harmoniq Labs")
            .with_description("Single shot sample player with WAV/MP3/FLAC support")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for Sampler {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            if let MidiEvent::NoteOn { velocity, .. } = event {
                if *velocity > 0 {
                    self.position = 0;
                    self.active = true;
                }
            }
        }
        Ok(())
    }
}

impl NativePlugin for Sampler {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }
}

fn sampler_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            SAMPLER_LEVEL,
            "Level",
            ParameterKind::continuous(0.0..=2.0, 1.0),
        ),
        ParameterDefinition::new(
            SAMPLER_START,
            "Start Offset",
            ParameterKind::continuous(0.0..=0.9, 0.0),
        ),
    ])
}

pub struct SamplerFactory;

impl PluginFactory for SamplerFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sampler", "Sampler", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(sampler_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(Sampler::default())
    }
}

// --- Granular Synth ----------------------------------------------------------------------

const GRANULAR_LEVEL: &str = "granular.level";
const GRANULAR_GRAIN: &str = "granular.grain";
const GRANULAR_DENSITY: &str = "granular.density";

#[derive(Debug, Clone)]
pub struct GranularSynth {
    sample_rate: f32,
    sample: SampleBuffer,
    active_grains: Vec<Grain>,
    parameters: ParameterSet,
    rng: rand::rngs::StdRng,
}

#[derive(Debug, Clone)]
struct Grain {
    start: usize,
    elapsed: usize,
    duration: usize,
    gain: f32,
}

impl Default for GranularSynth {
    fn default() -> Self {
        Self {
            sample_rate: 44_100.0,
            sample: SampleBuffer::default(),
            active_grains: Vec::new(),
            parameters: ParameterSet::new(granular_layout()),
            rng: rand::rngs::StdRng::from_entropy(),
        }
    }
}

impl GranularSynth {
    pub fn load_sample(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.sample = load_sample_from_file(path)?;
        Ok(())
    }

    pub async fn load_sample_async(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.sample = load_sample_from_file_async(path).await?;
        Ok(())
    }

    fn spawn_grain(&mut self) {
        if self.sample.is_empty() {
            return;
        }
        let grain_size = self
            .parameters
            .get(&ParameterId::from(GRANULAR_GRAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.1);
        let sr = self.sample.sample_rate().max(1) as f32;
        let length = (grain_size * sr) as usize;
        if length == 0 {
            return;
        }
        let max_start = self.sample.len().saturating_sub(length).max(1);
        let start = self.rng.gen_range(0..max_start);
        self.active_grains.push(Grain {
            start,
            elapsed: 0,
            duration: length,
            gain: 1.0,
        });
    }

    fn process_sample(&mut self) -> f32 {
        if self.sample.is_empty() {
            return 0.0;
        }
        let mut value = 0.0;
        let mut i = 0;
        while i < self.active_grains.len() {
            let grain = &mut self.active_grains[i];
            if grain.elapsed >= grain.duration {
                self.active_grains.swap_remove(i);
                continue;
            }
            let position = grain.start + grain.elapsed;
            if position >= self.sample.len() {
                self.active_grains.swap_remove(i);
                continue;
            }
            let sample = self.sample.mixed_sample(position);
            let progress = grain.elapsed as f32 / grain.duration as f32;
            let env = 0.5 - 0.5 * (2.0 * PI * progress).cos();
            value += sample * env * grain.gain;
            grain.elapsed += 1;
            i += 1;
        }
        value
    }
}

impl AudioProcessor for GranularSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.granular", "Granular Synth", "Harmoniq Labs")
            .with_description("Randomized grain based texture generator")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let density = self
            .parameters
            .get(&ParameterId::from(GRANULAR_DENSITY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.4);
        let level = self
            .parameters
            .get(&ParameterId::from(GRANULAR_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let samples = buffer.len();
        for frame in 0..samples {
            if self.rng.gen::<f32>() < density {
                self.spawn_grain();
            }
            let sample = self.process_sample() * level;
            for channel in buffer.channels_mut() {
                if let Some(slot) = channel.get_mut(frame) {
                    *slot = sample;
                }
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for GranularSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }
}

fn granular_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            GRANULAR_LEVEL,
            "Level",
            ParameterKind::continuous(0.0..=2.0, 0.8),
        ),
        ParameterDefinition::new(
            GRANULAR_GRAIN,
            "Grain Size",
            ParameterKind::continuous(0.01..=0.5, 0.1),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            GRANULAR_DENSITY,
            "Density",
            ParameterKind::continuous(0.0..=1.0, 0.4),
        ),
    ])
}

pub struct GranularSynthFactory;

impl PluginFactory for GranularSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.granular", "Granular Synth", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(granular_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(GranularSynth::default())
    }
}

// --- Additive Synth ----------------------------------------------------------------------

const ADDITIVE_LEVEL: &str = "additive.level";
const ADDITIVE_PARTIAL_BASE: &str = "additive.partial";

#[derive(Debug, Clone)]
pub struct AdditiveSynth {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    parameters: ParameterSet,
}

impl Default for AdditiveSynth {
    fn default() -> Self {
        Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 110.0,
            velocity: 0.0,
            parameters: ParameterSet::new(additive_layout()),
        }
    }
}

impl AdditiveSynth {
    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(ADDITIVE_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let mut sample = 0.0;
        for partial in 1..=8 {
            let id = ParameterId::new(format!("{ADDITIVE_PARTIAL_BASE}{partial}"));
            let gain = self
                .parameters
                .get(&id)
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(0.0);
            let phase = self.phase * partial as f32;
            sample += (2.0 * PI * phase).sin() * gain;
        }
        self.phase = (self.phase + self.frequency / self.sample_rate).fract();
        sample * level * self.velocity
    }
}

impl AudioProcessor for AdditiveSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.additive", "Additive Synth", "Harmoniq Labs")
            .with_description("8 partial harmonic resynthesis engine")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if self.velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for AdditiveSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = midi_note_to_freq(*note);
                    self.velocity = *velocity as f32 / 127.0;
                }
                MidiEvent::NoteOff { .. } => {
                    self.velocity = 0.0;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for AdditiveSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }
}

fn additive_layout() -> ParameterLayout {
    let mut params = vec![ParameterDefinition::new(
        ADDITIVE_LEVEL,
        "Level",
        ParameterKind::continuous(0.0..=1.5, 0.8),
    )];
    for partial in 1..=8 {
        params.push(ParameterDefinition::new(
            ParameterId::new(format!("{ADDITIVE_PARTIAL_BASE}{partial}")),
            format!("Partial {partial}"),
            ParameterKind::continuous(0.0..=1.0, if partial == 1 { 1.0 } else { 0.0 }),
        ));
    }
    ParameterLayout::new(params)
}

pub struct AdditiveSynthFactory;

impl PluginFactory for AdditiveSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.additive", "Additive Synth", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(additive_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(AdditiveSynth::default())
    }
}

// --- Organ / Piano Engine ----------------------------------------------------------------

const OP_LEVEL: &str = "op.level";
const OP_MODE: &str = "op.mode";
const OP_ATTACK: &str = "op.attack";
const OP_RELEASE: &str = "op.release";

#[derive(Debug, Clone)]
pub struct OrganPianoEngine {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    envelope: AdsrEnvelope,
    parameters: ParameterSet,
}

impl Default for OrganPianoEngine {
    fn default() -> Self {
        let mut envelope = AdsrEnvelope::default();
        envelope.set_params(0.01, 0.1, 0.8, 0.3);
        Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 110.0,
            velocity: 0.0,
            envelope,
            parameters: ParameterSet::new(organ_piano_layout()),
        }
    }
}

impl OrganPianoEngine {
    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(OP_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let mode = self
            .parameters
            .get(&ParameterId::from(OP_MODE))
            .and_then(ParameterValue::as_choice)
            .unwrap_or(0);
        let mut sample = 0.0;
        if mode == 0 {
            // Organ drawbar style.
            for harmonic in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].iter() {
                sample += (2.0 * PI * self.phase * harmonic).sin() / harmonic;
            }
        } else {
            // Simple piano like tone using damped sine with slight inharmonicity.
            let mut sum = 0.0;
            for n in 1..=4 {
                let freq = self.frequency * (n as f32 * 1.01);
                sum += (2.0 * PI * self.phase * freq / self.frequency).sin() / n as f32;
            }
            sample = sum;
        }
        self.phase = (self.phase + self.frequency / self.sample_rate).fract();
        let env = self.envelope.next();
        if !self.envelope.is_active() {
            self.velocity = 0.0;
        }
        sample * env * self.velocity * level
    }
}

impl AudioProcessor for OrganPianoEngine {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.organ_piano", "Organ/Piano", "Harmoniq Labs")
            .with_description("Hybrid organ and piano engine")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope.set_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if self.velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for OrganPianoEngine {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = midi_note_to_freq(*note);
                    self.velocity = *velocity as f32 / 127.0;
                    self.envelope.trigger();
                }
                MidiEvent::NoteOff { .. } => {
                    self.envelope.release();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for OrganPianoEngine {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        _value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        if matches!(id.as_str(), OP_ATTACK | OP_RELEASE) {
            let attack = self
                .parameters
                .get(&ParameterId::from(OP_ATTACK))
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(0.01);
            let release = self
                .parameters
                .get(&ParameterId::from(OP_RELEASE))
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(0.3);
            self.envelope.set_params(attack, 0.1, 0.8, release);
        }
        Ok(())
    }
}

fn organ_piano_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(OP_LEVEL, "Level", ParameterKind::continuous(0.0..=1.5, 0.8)),
        ParameterDefinition::new(
            OP_MODE,
            "Mode",
            ParameterKind::Choice {
                options: vec!["Organ".into(), "Piano".into()],
                default: 0,
            },
        ),
        ParameterDefinition::new(
            OP_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=0.5, 0.01),
        ),
        ParameterDefinition::new(
            OP_RELEASE,
            "Release",
            ParameterKind::continuous(0.05..=2.0, 0.4),
        ),
    ])
}

pub struct OrganPianoFactory;

impl PluginFactory for OrganPianoFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.organ_piano", "Organ/Piano", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(organ_piano_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(OrganPianoEngine::default())
    }
}

// --- Bass Synth --------------------------------------------------------------------------

const BASS_LEVEL: &str = "bass.level";
const BASS_OSC_MIX: &str = "bass.osc_mix";
const BASS_SUB_LEVEL: &str = "bass.sub_level";
const BASS_DETUNE: &str = "bass.detune";
const BASS_FILTER: &str = "bass.filter";
const BASS_RESONANCE: &str = "bass.resonance";
const BASS_ENV_AMOUNT: &str = "bass.env_amount";
const BASS_ATTACK: &str = "bass.attack";
const BASS_DECAY: &str = "bass.decay";
const BASS_SUSTAIN: &str = "bass.sustain";
const BASS_RELEASE: &str = "bass.release";
const BASS_GLIDE: &str = "bass.glide";

#[derive(Debug, Clone)]
struct LadderFilter {
    sample_rate: f32,
    stages: [f32; 4],
}

impl Default for LadderFilter {
    fn default() -> Self {
        Self {
            sample_rate: 44_100.0,
            stages: [0.0; 4],
        }
    }
}

impl LadderFilter {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    fn process(&mut self, input: f32, cutoff: f32, resonance: f32) -> f32 {
        let freq = cutoff.clamp(20.0, self.sample_rate * 0.45);
        let g = (2.0 * PI * freq / self.sample_rate).clamp(0.0, 1.0);
        let res = resonance.clamp(0.0, 1.0) * 4.0;
        let mut x = (input - self.stages[3] * res).tanh();
        for stage in &mut self.stages {
            *stage += g * (x - *stage);
            x = (*stage).tanh();
        }
        self.stages[3]
    }

    fn reset(&mut self) {
        self.stages = [0.0; 4];
    }
}

#[derive(Debug, Clone)]
pub struct BassSynth {
    sample_rate: f32,
    osc1_phase: f32,
    osc2_phase: f32,
    sub_phase: f32,
    current_freq: f32,
    target_freq: f32,
    note_velocity: f32,
    amp_envelope: AdsrEnvelope,
    filter_envelope: AdsrEnvelope,
    filter: LadderFilter,
    parameters: ParameterSet,
}

impl Default for BassSynth {
    fn default() -> Self {
        let parameters = ParameterSet::new(bass_layout());
        let mut synth = Self {
            sample_rate: 44_100.0,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            sub_phase: 0.0,
            current_freq: 55.0,
            target_freq: 55.0,
            note_velocity: 0.0,
            amp_envelope: AdsrEnvelope::default(),
            filter_envelope: AdsrEnvelope::default(),
            filter: LadderFilter::default(),
            parameters,
        };
        synth.sync_envelopes();
        synth
    }
}

impl BassSynth {
    fn sync_envelopes(&mut self) {
        let attack = self
            .parameters
            .get(&ParameterId::from(BASS_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.005);
        let decay = self
            .parameters
            .get(&ParameterId::from(BASS_DECAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.18);
        let sustain = self
            .parameters
            .get(&ParameterId::from(BASS_SUSTAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.6);
        let release = self
            .parameters
            .get(&ParameterId::from(BASS_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.15);

        self.amp_envelope
            .set_params(attack, decay, sustain, release);
        self.filter_envelope
            .set_params(attack, decay, sustain, release);
    }

    fn update_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.amp_envelope.set_sample_rate(sample_rate);
        self.filter_envelope.set_sample_rate(sample_rate);
        self.filter.set_sample_rate(sample_rate);
    }

    fn advance_phase(phase: &mut f32, freq: f32, sample_rate: f32) -> f32 {
        let increment = 2.0 * PI * freq / sample_rate;
        *phase = (*phase + increment).rem_euclid(2.0 * PI);
        *phase
    }

    fn render_sample(&mut self) -> f32 {
        let level = self
            .parameters
            .get(&ParameterId::from(BASS_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.9);
        let osc_mix = self
            .parameters
            .get(&ParameterId::from(BASS_OSC_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.4);
        let sub_level = self
            .parameters
            .get(&ParameterId::from(BASS_SUB_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.6);
        let detune = self
            .parameters
            .get(&ParameterId::from(BASS_DETUNE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.0);
        let cutoff = self
            .parameters
            .get(&ParameterId::from(BASS_FILTER))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(220.0);
        let resonance = self
            .parameters
            .get(&ParameterId::from(BASS_RESONANCE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3);
        let env_amount = self
            .parameters
            .get(&ParameterId::from(BASS_ENV_AMOUNT))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5);
        let glide = self
            .parameters
            .get(&ParameterId::from(BASS_GLIDE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.02);

        if glide <= 0.0001 {
            self.current_freq = self.target_freq;
        } else {
            let coeff = (-1.0 / (self.sample_rate * glide.max(0.0001))).exp();
            self.current_freq = self.target_freq + (self.current_freq - self.target_freq) * coeff;
        }

        let osc2_freq = self.current_freq * 2.0_f32.powf(detune / 12.0);
        let sub_freq = (self.current_freq / 2.0).max(20.0);

        let phase1 = Self::advance_phase(&mut self.osc1_phase, self.current_freq, self.sample_rate);
        let phase2 = Self::advance_phase(&mut self.osc2_phase, osc2_freq, self.sample_rate);
        let phase_sub = Self::advance_phase(&mut self.sub_phase, sub_freq, self.sample_rate);

        let saw = (phase1 / PI) - 1.0;
        let square = if phase2 < PI { 1.0 } else { -1.0 };
        let sub = if phase_sub < PI { 1.0 } else { -1.0 };

        let osc_mix = osc_mix.clamp(0.0, 1.0);
        let primary = saw * (1.0 - osc_mix) + square * osc_mix;
        let combined = (primary + sub * sub_level).tanh();

        let env_value = self.amp_envelope.next();
        let filter_env_value = self.filter_envelope.next();

        let dynamic_cutoff =
            cutoff + env_amount * filter_env_value * (self.sample_rate * 0.45 - cutoff);
        let filtered = self.filter.process(combined, dynamic_cutoff, resonance);

        (filtered * level * env_value * self.note_velocity).clamp(-1.0, 1.0)
    }
}

impl AudioProcessor for BassSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.bass", "Mini Moog Bass", "Harmoniq Labs")
            .with_description("Monophonic Mini Moog inspired bass synthesizer")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.update_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        self.sync_envelopes();
        if !self.amp_envelope.is_active() && self.note_velocity == 0.0 {
            buffer.clear();
            return Ok(());
        }
        fill_buffer(buffer, || self.render_sample());
        if !self.amp_envelope.is_active() {
            self.note_velocity = 0.0;
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for BassSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    let freq = midi_note_to_freq(*note);
                    self.target_freq = freq;
                    if self.note_velocity == 0.0 {
                        self.current_freq = freq;
                        self.filter.reset();
                    }
                    self.note_velocity = (*velocity as f32 / 127.0).clamp(0.0, 1.0);
                    self.amp_envelope.trigger();
                    self.filter_envelope.trigger();
                }
                MidiEvent::NoteOff { .. } => {
                    self.amp_envelope.release();
                    self.filter_envelope.release();
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for BassSynth {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }
}

fn bass_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            BASS_LEVEL,
            "Output Level",
            ParameterKind::continuous(0.0..=1.5, 0.9),
        ),
        ParameterDefinition::new(
            BASS_OSC_MIX,
            "Oscillator Mix",
            ParameterKind::continuous(0.0..=1.0, 0.4),
        ),
        ParameterDefinition::new(
            BASS_FILTER,
            "Filter Cutoff",
            ParameterKind::continuous(20.0..=5_000.0, 220.0),
        )
        .with_unit("Hz"),
        ParameterDefinition::new(
            BASS_SUB_LEVEL,
            "Sub Level",
            ParameterKind::continuous(0.0..=1.0, 0.6),
        ),
        ParameterDefinition::new(
            BASS_DETUNE,
            "Osc2 Detune",
            ParameterKind::continuous(-12.0..=12.0, 0.0),
        )
        .with_unit("st"),
        ParameterDefinition::new(
            BASS_RESONANCE,
            "Resonance",
            ParameterKind::continuous(0.0..=1.0, 0.3),
        ),
        ParameterDefinition::new(
            BASS_ENV_AMOUNT,
            "Filter Env Amt",
            ParameterKind::continuous(0.0..=1.0, 0.5),
        ),
        ParameterDefinition::new(
            BASS_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=0.5, 0.005),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            BASS_DECAY,
            "Decay",
            ParameterKind::continuous(0.01..=2.0, 0.18),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            BASS_SUSTAIN,
            "Sustain",
            ParameterKind::continuous(0.0..=1.0, 0.6),
        ),
        ParameterDefinition::new(
            BASS_RELEASE,
            "Release",
            ParameterKind::continuous(0.01..=1.5, 0.15),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            BASS_GLIDE,
            "Glide",
            ParameterKind::continuous(0.0..=0.5, 0.02),
        )
        .with_unit("s"),
    ])
}

pub struct BassSynthFactory;

impl PluginFactory for BassSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.bass", "Mini Moog Bass", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(bass_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(BassSynth::default())
    }
}

// --- West Coast Lead ----------------------------------------------------------------------

const WESTCOAST_LEVEL: &str = "westcoast.level";
const WESTCOAST_GLIDE: &str = "westcoast.glide";
const WESTCOAST_ATTACK: &str = "westcoast.attack";
const WESTCOAST_DECAY: &str = "westcoast.decay";
const WESTCOAST_SUSTAIN: &str = "westcoast.sustain";
const WESTCOAST_RELEASE: &str = "westcoast.release";
const WESTCOAST_VIBRATO_RATE: &str = "westcoast.vibrato_rate";
const WESTCOAST_VIBRATO_DEPTH: &str = "westcoast.vibrato_depth";
const WESTCOAST_WARMTH: &str = "westcoast.warmth";

const PITCH_BEND_RANGE: f32 = 2.0;

#[derive(Debug, Clone)]
pub struct WestCoastLead {
    sample_rate: f32,
    phase: f32,
    vibrato_phase: f32,
    current_freq: f32,
    target_freq: f32,
    velocity: f32,
    pitch_bend: f32,
    glide_time: f32,
    vibrato_rate: f32,
    vibrato_depth: f32,
    warmth: f32,
    level: f32,
    extra_vibrato: f32,
    envelope: AdsrEnvelope,
    active_notes: Vec<(u8, u8)>,
    parameters: ParameterSet,
}

impl Default for WestCoastLead {
    fn default() -> Self {
        let parameters = ParameterSet::new(westcoast_layout());
        let level = parameters
            .get(&ParameterId::from(WESTCOAST_LEVEL))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.9);
        let glide_time = parameters
            .get(&ParameterId::from(WESTCOAST_GLIDE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.12);
        let vibrato_rate = parameters
            .get(&ParameterId::from(WESTCOAST_VIBRATO_RATE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(4.8);
        let vibrato_depth = parameters
            .get(&ParameterId::from(WESTCOAST_VIBRATO_DEPTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.008);
        let warmth = parameters
            .get(&ParameterId::from(WESTCOAST_WARMTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3);
        let mut synth = Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            vibrato_phase: 0.0,
            current_freq: 440.0,
            target_freq: 440.0,
            velocity: 0.0,
            pitch_bend: 0.0,
            glide_time,
            vibrato_rate,
            vibrato_depth,
            warmth,
            level,
            extra_vibrato: 0.0,
            envelope: AdsrEnvelope::default(),
            active_notes: Vec::new(),
            parameters,
        };
        synth.envelope.set_sample_rate(44_100.0);
        synth.sync_envelope();
        synth
    }
}

impl WestCoastLead {
    fn sync_envelope(&mut self) {
        let attack = self
            .parameters
            .get(&ParameterId::from(WESTCOAST_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.02);
        let decay = self
            .parameters
            .get(&ParameterId::from(WESTCOAST_DECAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.18);
        let sustain = self
            .parameters
            .get(&ParameterId::from(WESTCOAST_SUSTAIN))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        let release = self
            .parameters
            .get(&ParameterId::from(WESTCOAST_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.26);
        self.envelope
            .set_params(attack, decay, sustain.clamp(0.0, 1.0), release);
    }

    fn update_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.envelope.set_sample_rate(self.sample_rate);
    }

    fn glide_factor(&self) -> f32 {
        if self.glide_time <= 0.0001 {
            1.0
        } else {
            (1.0 / (self.glide_time * self.sample_rate.max(1.0))).clamp(0.0, 1.0)
        }
    }

    fn render_sample(&mut self) -> f32 {
        let glide = self.glide_factor();
        self.current_freq += (self.target_freq - self.current_freq) * glide;

        let vibrato_increment = TAU * self.vibrato_rate.max(0.0) / self.sample_rate.max(1.0);
        self.vibrato_phase = (self.vibrato_phase + vibrato_increment).rem_euclid(TAU);
        let vibrato_depth = self.vibrato_depth * (1.0 + 0.8 * self.extra_vibrato);
        let vibrato = self.vibrato_phase.sin() * vibrato_depth;

        let pitch = self.current_freq * 2.0_f32.powf(self.pitch_bend * PITCH_BEND_RANGE / 12.0);
        let freq = (pitch * (1.0 + vibrato)).clamp(20.0, self.sample_rate * 0.45);

        let increment = TAU * freq / self.sample_rate.max(1.0);
        self.phase = (self.phase + increment).rem_euclid(TAU);
        let envelope = self.envelope.next();
        let raw = self.phase.sin();
        let shaped = raw * (1.0 - self.warmth) + raw.powi(3) * self.warmth;
        shaped * envelope * self.velocity * self.level
    }
}

impl AudioProcessor for WestCoastLead {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.westcoast", "West Coast Lead", "Harmoniq Labs")
            .with_description("Portamento sine lead inspired by classic West Coast rap")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.update_sample_rate(config.sample_rate);
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        if !self.envelope.is_active() && self.active_notes.is_empty() {
            for sample in buffer.iter_mut() {
                *sample = 0.0;
            }
            return Ok(());
        }

        fill_buffer(buffer, || self.render_sample());
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for WestCoastLead {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    let freq = midi_note_to_freq(*note);
                    self.target_freq = freq;
                    if self.active_notes.is_empty() {
                        self.current_freq = freq;
                        self.envelope.trigger();
                    }
                    self.velocity = (*velocity as f32 / 127.0).clamp(0.0, 1.0);
                    self.active_notes.push((*note, *velocity));
                }
                MidiEvent::NoteOff { note, .. } => {
                    if let Some(index) = self.active_notes.iter().rposition(|(n, _)| n == note) {
                        self.active_notes.remove(index);
                    }
                    if let Some((note, velocity)) = self.active_notes.last().copied() {
                        self.target_freq = midi_note_to_freq(note);
                        self.velocity = (velocity as f32 / 127.0).clamp(0.0, 1.0);
                    } else {
                        self.envelope.release();
                    }
                }
                MidiEvent::ControlChange { control, value, .. } if *control == 1 => {
                    self.extra_vibrato = (*value as f32 / 127.0).clamp(0.0, 1.0);
                }
                MidiEvent::PitchBend { lsb, msb, .. } => {
                    let value = ((*msb as u16) << 7) | (*lsb as u16);
                    self.pitch_bend = (value as f32 - 8_192.0) / 8_192.0;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for WestCoastLead {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        match id.as_str() {
            WESTCOAST_LEVEL => {
                if let Some(level) = value.as_continuous() {
                    self.level = level;
                }
            }
            WESTCOAST_GLIDE => {
                if let Some(glide) = value.as_continuous() {
                    self.glide_time = glide;
                }
            }
            WESTCOAST_VIBRATO_RATE => {
                if let Some(rate) = value.as_continuous() {
                    self.vibrato_rate = rate;
                }
            }
            WESTCOAST_VIBRATO_DEPTH => {
                if let Some(depth) = value.as_continuous() {
                    self.vibrato_depth = depth;
                }
            }
            WESTCOAST_WARMTH => {
                if let Some(warmth) = value.as_continuous() {
                    self.warmth = warmth.clamp(0.0, 1.0);
                }
            }
            WESTCOAST_ATTACK | WESTCOAST_DECAY | WESTCOAST_SUSTAIN | WESTCOAST_RELEASE => {
                self.sync_envelope();
            }
            _ => {}
        }
        Ok(())
    }
}

fn westcoast_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            WESTCOAST_LEVEL,
            "Output Level",
            ParameterKind::continuous(0.0..=1.2, 0.9),
        )
        .with_description("Final amplitude applied to the silky sine lead"),
        ParameterDefinition::new(
            WESTCOAST_GLIDE,
            "Glide",
            ParameterKind::continuous(0.0..=0.4, 0.12),
        )
        .with_unit("s")
        .with_description("Portamento time between notes for laid-back slides"),
        ParameterDefinition::new(
            WESTCOAST_ATTACK,
            "Attack",
            ParameterKind::continuous(0.001..=0.3, 0.02),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            WESTCOAST_DECAY,
            "Decay",
            ParameterKind::continuous(0.05..=1.0, 0.18),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            WESTCOAST_SUSTAIN,
            "Sustain",
            ParameterKind::continuous(0.3..=1.0, 0.8),
        ),
        ParameterDefinition::new(
            WESTCOAST_RELEASE,
            "Release",
            ParameterKind::continuous(0.05..=1.5, 0.26),
        )
        .with_unit("s"),
        ParameterDefinition::new(
            WESTCOAST_VIBRATO_RATE,
            "Vibrato Rate",
            ParameterKind::continuous(0.1..=8.0, 4.8),
        )
        .with_unit("Hz"),
        ParameterDefinition::new(
            WESTCOAST_VIBRATO_DEPTH,
            "Vibrato Depth",
            ParameterKind::continuous(0.0..=0.02, 0.008),
        )
        .with_description("Pitch modulation depth accentuated by the mod wheel"),
        ParameterDefinition::new(
            WESTCOAST_WARMTH,
            "Warmth",
            ParameterKind::continuous(0.0..=1.0, 0.3),
        )
        .with_description("Blend of pure sine and saturated harmonics"),
    ])
}

pub struct WestCoastLeadFactory;

impl PluginFactory for WestCoastLeadFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.westcoast", "West Coast Lead", "Harmoniq Labs")
            .with_description("Portamento sine lead inspired by classic West Coast rap")
    }

    fn parameter_layout(&self) -> std::sync::Arc<ParameterLayout> {
        std::sync::Arc::new(westcoast_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(WestCoastLead::default())
    }
}
