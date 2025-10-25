use nih_plug::prelude::*;
use nih_plug::prelude::{formatters, AtomicF32};
use nih_plug::util;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self,
        plot::{Line, Plot, PlotPoints},
        RichText, Vec2,
    },
    resizable_window::ResizableWindow,
    widgets, EguiState,
};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod dsp;

use dsp::fx_chorus::StereoChorus;
use dsp::fx_reverb::PlateReverb;
use dsp::osc::{Lfo, LfoWaveform};
use dsp::voice::{EnvelopeSettings, Voice, VoiceParams};

const MAX_VOICES: usize = 8;
const PITCH_BEND_RANGE: f32 = 12.0;
const OSCILLOSCOPE_SAMPLES: usize = 256;

pub struct WestCoastWhineSynth {
    params: Arc<WestCoastParams>,
    voices: Vec<Voice>,
    voice_notes: [Option<u8>; MAX_VOICES],
    voice_age: [u64; MAX_VOICES],
    note_counter: u64,
    sample_rate: f32,
    pitch_bend: f32,
    chorus: StereoChorus,
    reverb: PlateReverb,
    lfo: Lfo,
    note_stack: NoteStack,
    oscilloscope: Arc<OscilloscopeState>,
}

#[derive(Default)]
struct NoteStack {
    notes: [(u8, f32); MAX_VOICES],
    len: usize,
}

struct RemoveResult {
    was_top: bool,
    new_top: Option<(u8, f32)>,
}

impl NoteStack {
    fn push(&mut self, note: u8, velocity: f32) {
        if self.contains(note) {
            return;
        }
        if self.len < MAX_VOICES {
            self.notes[self.len] = (note, velocity);
            self.len += 1;
        }
    }

    fn remove(&mut self, note: u8) -> Option<RemoveResult> {
        for idx in 0..self.len {
            if self.notes[idx].0 == note {
                let was_top = idx + 1 == self.len;
                for j in idx..self.len.saturating_sub(1) {
                    self.notes[j] = self.notes[j + 1];
                }
                if self.len > 0 {
                    self.len -= 1;
                }
                let new_top = if self.len > 0 {
                    Some(self.notes[self.len - 1])
                } else {
                    None
                };
                return Some(RemoveResult { was_top, new_top });
            }
        }
        None
    }

    fn top(&self) -> Option<(u8, f32)> {
        if self.len > 0 {
            Some(self.notes[self.len - 1])
        } else {
            None
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn contains(&self, note: u8) -> bool {
        self.notes[..self.len].iter().any(|&(n, _)| n == note)
    }
}

struct OscilloscopeState {
    samples: Vec<AtomicF32>,
    write_index: AtomicUsize,
}

impl OscilloscopeState {
    fn new(size: usize) -> Self {
        let mut samples = Vec::with_capacity(size);
        for _ in 0..size {
            samples.push(AtomicF32::new(0.0));
        }
        Self {
            samples,
            write_index: AtomicUsize::new(0),
        }
    }

    fn push(&self, sample: f32) {
        let idx = self.write_index.fetch_add(1, Ordering::Relaxed) % self.samples.len();
        self.samples[idx].store(sample, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Vec<f32> {
        self.samples
            .iter()
            .map(|sample| sample.load(Ordering::Relaxed))
            .collect()
    }
}

#[derive(Enum, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
}

impl Default for LfoShape {
    fn default() -> Self {
        LfoShape::Sine
    }
}

impl LfoShape {
    fn to_waveform(self) -> LfoWaveform {
        match self {
            LfoShape::Sine => LfoWaveform::Sine,
            LfoShape::Triangle => LfoWaveform::Triangle,
        }
    }
}

#[derive(Params)]
pub struct WestCoastParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[nested(group = "Oscillators")]
    pub oscillators: OscParams,

    #[nested(group = "Filter")]
    pub filter: FilterParams,

    #[nested(group = "Envelopes")]
    pub envelopes: EnvelopeParams,

    #[nested(group = "Modulation")]
    pub modulation: ModParams,

    #[nested(group = "FX")]
    pub fx: FxParams,

    #[nested(group = "Master")]
    pub master: MasterParams,

    #[id = "voices"]
    pub voices: IntParam,
}

#[derive(Params)]
pub struct OscParams {
    #[id = "blend"]
    pub blend: FloatParam,
    #[id = "sub_level"]
    pub sub_level: FloatParam,
    #[id = "detune_cents"]
    pub detune_cents: FloatParam,
    #[id = "glide_time"]
    pub glide_time: FloatParam,
}

#[derive(Params)]
pub struct FilterParams {
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,
    #[id = "env_amt"]
    pub env_amount: FloatParam,
    #[id = "env_depth"]
    pub env_depth: FloatParam,
    #[id = "vel_cutoff"]
    pub velocity_to_cutoff: BoolParam,
}

#[derive(Params)]
pub struct EnvelopeParams {
    #[nested(group = "Amp Envelope")]
    pub amp: AmpEnvelopeParams,
    #[nested(group = "Filter Envelope")]
    pub filter: FilterEnvelopeParams,
}

#[derive(Params)]
pub struct AmpEnvelopeParams {
    #[id = "amp_attack"]
    pub attack: FloatParam,
    #[id = "amp_decay"]
    pub decay: FloatParam,
    #[id = "amp_sustain"]
    pub sustain: FloatParam,
    #[id = "amp_release"]
    pub release: FloatParam,
}

#[derive(Params)]
pub struct FilterEnvelopeParams {
    #[id = "f_attack"]
    pub attack: FloatParam,
    #[id = "f_decay"]
    pub decay: FloatParam,
    #[id = "f_sustain"]
    pub sustain: FloatParam,
    #[id = "f_release"]
    pub release: FloatParam,
}

#[derive(Params)]
pub struct ModParams {
    #[id = "lfo_wave"]
    pub waveform: EnumParam<LfoShape>,
    #[id = "lfo_rate"]
    pub rate: FloatParam,
    #[id = "lfo_pitch"]
    pub pitch_amount: FloatParam,
    #[id = "lfo_cutoff"]
    pub cutoff_amount: FloatParam,
    #[id = "lfo_amp"]
    pub amp_amount: FloatParam,
}

#[derive(Params)]
pub struct FxParams {
    #[id = "chorus_rate"]
    pub chorus_rate: FloatParam,
    #[id = "chorus_depth"]
    pub chorus_depth: FloatParam,
    #[id = "chorus_mix"]
    pub chorus_mix: FloatParam,
    #[id = "reverb_mix"]
    pub reverb_mix: FloatParam,
}

#[derive(Params)]
pub struct MasterParams {
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl Default for WestCoastParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(600, 480),
            oscillators: OscParams {
                blend: FloatParam::new(
                    "Sine/Saw Blend",
                    0.25,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                )
                .with_smoother(SmoothingStyle::Linear(10.0))
                .with_unit("")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
                sub_level: FloatParam::new(
                    "Sub Level",
                    0.15,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                )
                .with_smoother(SmoothingStyle::Linear(10.0)),
                detune_cents: FloatParam::new(
                    "Detune",
                    1.5,
                    FloatRange::Linear {
                        min: -10.0,
                        max: 10.0,
                    },
                )
                .with_unit(" ct"),
                glide_time: FloatParam::new(
                    "Glide",
                    0.150,
                    FloatRange::Linear {
                        min: 0.010,
                        max: 0.300,
                    },
                )
                .with_unit(" s"),
            },
            filter: FilterParams {
                cutoff: FloatParam::new(
                    "Cutoff",
                    2_500.0,
                    FloatRange::Skewed {
                        min: 100.0,
                        max: 20_000.0,
                        factor: FloatRange::skew_factor(-0.5),
                    },
                )
                .with_smoother(SmoothingStyle::Logarithmic(30.0))
                .with_unit(" Hz"),
                resonance: FloatParam::new(
                    "Resonance",
                    0.2,
                    FloatRange::Linear { min: 0.0, max: 0.8 },
                ),
                env_amount: FloatParam::new(
                    "Env Amount",
                    0.4,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                env_depth: FloatParam::new(
                    "Env Depth",
                    1.0,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                velocity_to_cutoff: BoolParam::new("Velocity -> Cutoff", true),
            },
            envelopes: EnvelopeParams {
                amp: AmpEnvelopeParams {
                    attack: FloatParam::new(
                        "Attack",
                        0.005,
                        FloatRange::Linear {
                            min: 0.001,
                            max: 0.100,
                        },
                    )
                    .with_unit(" s"),
                    decay: FloatParam::new(
                        "Decay",
                        0.150,
                        FloatRange::Linear {
                            min: 0.010,
                            max: 0.400,
                        },
                    )
                    .with_unit(" s"),
                    sustain: FloatParam::new(
                        "Sustain",
                        0.8,
                        FloatRange::Linear { min: 0.0, max: 1.0 },
                    ),
                    release: FloatParam::new(
                        "Release",
                        0.250,
                        FloatRange::Linear {
                            min: 0.020,
                            max: 0.800,
                        },
                    )
                    .with_unit(" s"),
                },
                filter: FilterEnvelopeParams {
                    attack: FloatParam::new(
                        "Attack",
                        0.010,
                        FloatRange::Linear {
                            min: 0.001,
                            max: 0.100,
                        },
                    )
                    .with_unit(" s"),
                    decay: FloatParam::new(
                        "Decay",
                        0.200,
                        FloatRange::Linear {
                            min: 0.050,
                            max: 0.400,
                        },
                    )
                    .with_unit(" s"),
                    sustain: FloatParam::new(
                        "Sustain",
                        0.0,
                        FloatRange::Linear { min: 0.0, max: 1.0 },
                    ),
                    release: FloatParam::new(
                        "Release",
                        0.250,
                        FloatRange::Linear {
                            min: 0.050,
                            max: 0.800,
                        },
                    )
                    .with_unit(" s"),
                },
            },
            modulation: ModParams {
                waveform: EnumParam::new("Waveform", LfoShape::Sine),
                rate: FloatParam::new(
                    "Rate",
                    0.8,
                    FloatRange::Linear {
                        min: 0.1,
                        max: 12.0,
                    },
                )
                .with_unit(" Hz"),
                pitch_amount: FloatParam::new(
                    "Pitch Amount",
                    0.0,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                cutoff_amount: FloatParam::new(
                    "Cutoff Amount",
                    0.15,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                amp_amount: FloatParam::new(
                    "Amp Amount",
                    0.0,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
            },
            fx: FxParams {
                chorus_rate: FloatParam::new(
                    "Chorus Rate",
                    0.4,
                    FloatRange::Linear {
                        min: 0.05,
                        max: 5.0,
                    },
                )
                .with_unit(" Hz"),
                chorus_depth: FloatParam::new(
                    "Chorus Depth",
                    0.3,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                chorus_mix: FloatParam::new(
                    "Chorus Mix",
                    0.3,
                    FloatRange::Linear { min: 0.0, max: 1.0 },
                ),
                reverb_mix: FloatParam::new(
                    "Reverb Mix",
                    0.15,
                    FloatRange::Linear { min: 0.0, max: 0.6 },
                ),
            },
            master: MasterParams {
                output_gain: FloatParam::new(
                    "Output Gain",
                    -6.0,
                    FloatRange::Linear {
                        min: -24.0,
                        max: 6.0,
                    },
                )
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            },
            voices: IntParam::new(
                "Voices",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_VOICES as i32,
                },
            ),
        }
    }
}

impl Default for WestCoastWhineSynth {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        let params = Arc::new(WestCoastParams::default());
        let voices = (0..MAX_VOICES).map(|_| Voice::new(sample_rate)).collect();

        Self {
            params,
            voices,
            voice_notes: [None; MAX_VOICES],
            voice_age: [0; MAX_VOICES],
            note_counter: 0,
            sample_rate,
            pitch_bend: 0.0,
            chorus: StereoChorus::new(),
            reverb: PlateReverb::new(),
            lfo: Lfo::new(),
            note_stack: NoteStack::default(),
            oscilloscope: Arc::new(OscilloscopeState::new(OSCILLOSCOPE_SAMPLES)),
        }
    }
}

impl WestCoastWhineSynth {
    fn amp_envelope_settings(&self) -> EnvelopeSettings {
        EnvelopeSettings {
            attack: self.params.envelopes.amp.attack.value(),
            decay: self.params.envelopes.amp.decay.value(),
            sustain: self.params.envelopes.amp.sustain.value(),
            release: self.params.envelopes.amp.release.value(),
        }
    }

    fn filter_envelope_settings(&self) -> EnvelopeSettings {
        EnvelopeSettings {
            attack: self.params.envelopes.filter.attack.value(),
            decay: self.params.envelopes.filter.decay.value(),
            sustain: self.params.envelopes.filter.sustain.value(),
            release: self.params.envelopes.filter.release.value(),
        }
    }

    fn handle_note_on(&mut self, note: u8, velocity: f32, voices_allowed: usize) {
        let freq = Self::note_to_hz(note);
        let cutoff = self.params.filter.cutoff.value();
        let resonance = self.params.filter.resonance.value();
        let glide = self.params.oscillators.glide_time.value();
        let amp_env = self.amp_envelope_settings();
        let filter_env = self.filter_envelope_settings();

        if voices_allowed <= 1 {
            self.note_stack.push(note, velocity);
            let voice = &mut self.voices[0];
            if voice.active {
                voice.legato_note(
                    note, velocity, freq, cutoff, resonance, glide, amp_env, filter_env,
                );
            } else {
                voice.note_on(
                    note, velocity, freq, cutoff, resonance, glide, amp_env, filter_env,
                );
            }
            self.voice_notes[0] = Some(note);
            self.voice_age[0] = self.note_counter;
            self.note_counter = self.note_counter.wrapping_add(1);
            return;
        }

        let mut target_voice = None;
        for idx in 0..voices_allowed.min(self.voices.len()) {
            if !self.voices[idx].active {
                target_voice = Some(idx);
                break;
            }
        }

        let voice_index = target_voice.unwrap_or_else(|| {
            let mut oldest_age = u64::MAX;
            let mut oldest_index = 0;
            for idx in 0..voices_allowed.min(self.voices.len()) {
                if self.voice_age[idx] < oldest_age {
                    oldest_age = self.voice_age[idx];
                    oldest_index = idx;
                }
            }
            oldest_index
        });

        self.voices[voice_index].note_on(
            note, velocity, freq, cutoff, resonance, glide, amp_env, filter_env,
        );
        self.voice_notes[voice_index] = Some(note);
        self.voice_age[voice_index] = self.note_counter;
        self.note_counter = self.note_counter.wrapping_add(1);
    }

    fn handle_note_off(&mut self, note: u8, voices_allowed: usize) {
        if voices_allowed <= 1 {
            if let Some(result) = self.note_stack.remove(note) {
                if result.was_top {
                    if let Some((prev_note, prev_velocity)) = result.new_top {
                        let freq = Self::note_to_hz(prev_note);
                        let cutoff = self.params.filter.cutoff.value();
                        let resonance = self.params.filter.resonance.value();
                        let glide = self.params.oscillators.glide_time.value();
                        let amp_env = self.amp_envelope_settings();
                        let filter_env = self.filter_envelope_settings();
                        self.voices[0].legato_note(
                            prev_note,
                            prev_velocity,
                            freq,
                            cutoff,
                            resonance,
                            glide,
                            amp_env,
                            filter_env,
                        );
                        self.voice_notes[0] = Some(prev_note);
                    } else {
                        self.voices[0].note_off();
                        self.voice_notes[0] = None;
                    }
                }
            }
            return;
        }

        for idx in 0..voices_allowed.min(self.voices.len()) {
            if self.voice_notes[idx] == Some(note) {
                self.voices[idx].note_off();
                self.voice_notes[idx] = None;
            }
        }
    }

    fn render_block(
        &mut self,
        buffer: &mut Buffer,
        start: usize,
        end: usize,
        voices_allowed: usize,
    ) {
        let outputs = buffer.as_slice();
        let num_channels = outputs.len();
        let voices_allowed = voices_allowed.min(MAX_VOICES).max(1);

        for idx in voices_allowed..self.voices.len() {
            if self.voices[idx].active {
                self.voices[idx].reset();
            }
            self.voice_notes[idx] = None;
        }

        let lfo_waveform = self.params.modulation.waveform.value().to_waveform();

        for sample_idx in start..end {
            let blend = self.params.oscillators.blend.smoothed.next();
            let sub_level = self.params.oscillators.sub_level.smoothed.next();
            let detune = self.params.oscillators.detune_cents.smoothed.next();
            let filter_cutoff = self.params.filter.cutoff.smoothed.next();
            let resonance = self.params.filter.resonance.smoothed.next();
            let filter_env_amount = self.params.filter.env_amount.smoothed.next();
            let filter_env_depth = self.params.filter.env_depth.smoothed.next();
            let lfo_rate = self.params.modulation.rate.smoothed.next();
            let lfo_pitch_amount = self.params.modulation.pitch_amount.smoothed.next();
            let lfo_cutoff_amount = self.params.modulation.cutoff_amount.smoothed.next();
            let lfo_amp_amount = self.params.modulation.amp_amount.smoothed.next();
            let chorus_mix = self.params.fx.chorus_mix.smoothed.next();
            let chorus_rate = self.params.fx.chorus_rate.smoothed.next();
            let chorus_depth = self.params.fx.chorus_depth.smoothed.next();
            let reverb_mix = self.params.fx.reverb_mix.smoothed.next();
            let output_gain = util::db_to_gain(self.params.master.output_gain.smoothed.next());
            let velocity_to_cutoff = self.params.filter.velocity_to_cutoff.value();

            let lfo_value = self.lfo.next(lfo_waveform, lfo_rate, self.sample_rate);

            let voice_params = VoiceParams {
                blend,
                sub_level,
                detune_cents: detune,
                filter_cutoff,
                filter_resonance: resonance,
                filter_env_amount,
                velocity_to_cutoff,
                filter_env_depth,
                lfo_value,
                lfo_pitch_amount,
                lfo_cutoff_amount,
                lfo_amp_amount,
                pitch_bend_semitones: self.pitch_bend,
                velocity_amp_scale: 1.0,
            };

            let mut sample = 0.0f32;
            for idx in 0..voices_allowed {
                sample += self.voices[idx].render(&voice_params);
            }

            sample = sample.tanh();
            let (mut left, mut right) = (sample, sample);
            let (c_left, c_right) =
                self.chorus
                    .process(left, right, chorus_rate, chorus_depth, chorus_mix);
            let (r_left, r_right) = self.reverb.process(c_left, c_right, reverb_mix);

            left = r_left * output_gain;
            right = r_right * output_gain;

            self.oscilloscope.push((left + right) * 0.5);

            if num_channels >= 2 {
                outputs[0][sample_idx] = left;
                outputs[1][sample_idx] = right;
            } else if num_channels == 1 {
                outputs[0][sample_idx] = (left + right) * 0.5;
            }
        }
    }

    fn note_to_hz(note: u8) -> f32 {
        440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
    }
}

impl Plugin for WestCoastWhineSynth {
    const NAME: &'static str = "WestCoast Whine Synth";
    const VENDOR: &'static str = "Harmoniq Studio";
    const URL: &'static str = "https://harmoniq.studio";
    const EMAIL: &'static str = "support@harmoniq.studio";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCsAndNotes;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        for voice in &mut self.voices {
            voice.set_sample_rate(self.sample_rate);
        }
        self.chorus.prepare(self.sample_rate);
        self.reverb.prepare(self.sample_rate);
        context.set_current_voice_capacity(self.params.voices.value() as u32);
        true
    }

    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.reset();
        }
        self.voice_notes = [None; MAX_VOICES];
        self.note_stack.clear();
        self.pitch_bend = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let num_samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut block_start = 0usize;
        let mut voices_allowed = self.params.voices.value().clamp(1, MAX_VOICES as i32) as usize;
        context.set_current_voice_capacity(voices_allowed as u32);

        while block_start < num_samples {
            let next_event_timing = next_event
                .as_ref()
                .map(|event| event.timing() as usize)
                .unwrap_or(num_samples);
            let block_end = next_event_timing.min(num_samples);

            self.render_block(buffer, block_start, block_end, voices_allowed);
            block_start = block_end;

            while let Some(event) = next_event.take() {
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        self.handle_note_on(note, velocity, voices_allowed);
                    }
                    NoteEvent::NoteOff { note, .. }
                    | NoteEvent::Choke { note, .. }
                    | NoteEvent::VoiceTerminated { note, .. } => {
                        self.handle_note_off(note, voices_allowed);
                    }
                    NoteEvent::MidiPitchBend { value, .. } => {
                        self.pitch_bend = (value - 0.5) * 2.0 * PITCH_BEND_RANGE;
                    }
                    _ => {}
                }

                next_event = context.next_event();
                voices_allowed = self.params.voices.value().clamp(1, MAX_VOICES as i32) as usize;
                context.set_current_voice_capacity(voices_allowed as u32);

                if next_event
                    .as_ref()
                    .map(|event| event.timing() as usize)
                    .unwrap_or(num_samples)
                    > block_start
                {
                    break;
                }
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let oscilloscope = self.oscilloscope.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            GuiState { oscilloscope },
            |_ctx, _state| {},
            move |egui_ctx, setter, state| {
                ResizableWindow::new("westcoast-whine")
                    .default_size(Vec2::new(620.0, 520.0))
                    .show(egui_ctx, params.editor_state.as_ref(), |ui| {
                        ui.heading(RichText::new("WestCoast Whine Synth").size(20.0));
                        ui.separator();

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.group(|ui| {
                                        ui.label("Oscillators");
                                        let blend_label = format!(
                                            "Blend {:>3}%",
                                            (params.oscillators.blend.value() * 100.0).round()
                                        );
                                        ui.label(RichText::new(blend_label));
                                        ui.add(
                                            widgets::ParamSlider::for_param(
                                                &params.oscillators.blend,
                                                setter,
                                            )
                                            .with_width(140.0),
                                        )
                                        .on_hover_text("Blend between sine and saw oscillators");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.oscillators.sub_level,
                                            setter,
                                        ))
                                        .on_hover_text("Sub oscillator level, one octave below");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.oscillators.detune_cents,
                                            setter,
                                        ))
                                        .on_hover_text("Fine detune for warmth");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.oscillators.glide_time,
                                            setter,
                                        ))
                                        .on_hover_text("Glide time for legato portamento");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.voices,
                                            setter,
                                        ))
                                        .on_hover_text("Number of simultaneous voices");
                                    });

                                    ui.group(|ui| {
                                        ui.label("Filter");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.filter.cutoff,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.filter.resonance,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.filter.env_amount,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.filter.env_depth,
                                            setter,
                                        ));
                                        let mut vel = params.filter.velocity_to_cutoff.value();
                                        if ui
                                            .checkbox(&mut vel, "Velocity -> Cutoff")
                                            .on_hover_text(
                                                "Enable velocity affecting filter cutoff",
                                            )
                                            .changed()
                                        {
                                            setter.begin_set_parameter(
                                                &params.filter.velocity_to_cutoff,
                                            );
                                            setter.set_parameter(
                                                &params.filter.velocity_to_cutoff,
                                                vel,
                                            );
                                            setter.end_set_parameter(
                                                &params.filter.velocity_to_cutoff,
                                            );
                                        }
                                    });
                                });

                                ui.vertical(|ui| {
                                    ui.group(|ui| {
                                        ui.label("Envelopes");
                                        ui.label("Amp");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.amp.attack,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.amp.decay,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.amp.sustain,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.amp.release,
                                            setter,
                                        ));

                                        ui.separator();
                                        ui.label("Filter");
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.filter.attack,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.filter.decay,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.filter.sustain,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.envelopes.filter.release,
                                            setter,
                                        ));
                                    });

                                    ui.group(|ui| {
                                        ui.label("Modulation");
                                        let mut waveform = params.modulation.waveform.value();
                                        egui::ComboBox::from_label("LFO Shape")
                                            .selected_text(waveform.to_string())
                                            .show_ui(ui, |ui| {
                                                for variant in LfoShape::variants() {
                                                    ui.selectable_value(
                                                        &mut waveform,
                                                        *variant,
                                                        variant.to_string(),
                                                    );
                                                }
                                            });
                                        if waveform != params.modulation.waveform.value() {
                                            setter.begin_set_parameter(&params.modulation.waveform);
                                            setter.set_parameter(
                                                &params.modulation.waveform,
                                                waveform,
                                            );
                                            setter.end_set_parameter(&params.modulation.waveform);
                                        }
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.modulation.rate,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.modulation.pitch_amount,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.modulation.cutoff_amount,
                                            setter,
                                        ));
                                        ui.add(widgets::ParamSlider::for_param(
                                            &params.modulation.amp_amount,
                                            setter,
                                        ));
                                    });
                                });
                            });

                            ui.horizontal(|ui| {
                                ui.group(|ui| {
                                    ui.label("FX");
                                    ui.add(widgets::ParamSlider::for_param(
                                        &params.fx.chorus_rate,
                                        setter,
                                    ));
                                    ui.add(widgets::ParamSlider::for_param(
                                        &params.fx.chorus_depth,
                                        setter,
                                    ));
                                    ui.add(widgets::ParamSlider::for_param(
                                        &params.fx.chorus_mix,
                                        setter,
                                    ));
                                    ui.add(widgets::ParamSlider::for_param(
                                        &params.fx.reverb_mix,
                                        setter,
                                    ));
                                });

                                ui.group(|ui| {
                                    ui.label("Master");
                                    ui.add(widgets::ParamSlider::for_param(
                                        &params.master.output_gain,
                                        setter,
                                    ));
                                    let samples = state.oscilloscope.snapshot();
                                    let points = PlotPoints::from_iter(
                                        samples
                                            .iter()
                                            .enumerate()
                                            .map(|(i, sample)| [i as f64, *sample as f64]),
                                    );
                                    Plot::new("oscilloscope")
                                        .view_aspect(3.0)
                                        .include_y(-1.0)
                                        .include_y(1.0)
                                        .show(ui, |plot_ui| {
                                            plot_ui.line(Line::new(points));
                                        });
                                });
                            });
                        });
                    });
            },
        )
    }
}

struct GuiState {
    oscilloscope: Arc<OscilloscopeState>,
}

impl ClapPlugin for WestCoastWhineSynth {
    const CLAP_ID: &'static str = "com.harmoniq.westcoast_whine";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Polyphonic West Coast whine lead synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://harmoniq.studio");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://harmoniq.studio/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synth,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for WestCoastWhineSynth {
    const VST3_CLASS_ID: [u8; 16] = *b"WestCoastWhinePlg";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(WestCoastWhineSynth);
nih_export_vst3!(WestCoastWhineSynth);
