//! Harmoniq Sub808 – a simple 808-style sub-bass synth implemented as a CLAP plugin.
//! - Sine/sub oscillator with pitch thump envelope
//! - Amp decay envelope (808 style)
//! - Glide/portamento
//! - Drive (tanh) + post low-pass
//! - Mono/poly up to 8 voices, simple voice stealing
//! - EGUI editor via nih-plug
//!
//! **RT Safety:** No allocations in process(); all per-voice state is preallocated.

use nih_plug::prelude::*;
use std::sync::Arc;

#[cfg(feature = "editor")]
mod ui;
mod voice;
use voice::{SubVoice, MAX_VOICES};

#[derive(Params)]
struct Sub808Params {
    // AMP
    #[id = "level"]
    level: FloatParam, // output gain

    #[id = "decay"]
    decay_s: FloatParam, // seconds

    #[id = "velsens"]
    vel_sens: FloatParam, // 0..1

    // PITCH THUMP
    #[id = "thump"]
    thump_amt_st: FloatParam, // semitones at attack

    #[id = "thumpdec"]
    thump_decay_s: FloatParam, // seconds

    // GLIDE
    #[id = "glide"]
    glide_ms: FloatParam,

    // DRIVE + TONE
    #[id = "drive"]
    drive: FloatParam, // pre-gain into tanh

    #[id = "tone"]
    tone_hz: FloatParam, // 1-pole lowpass cutoff

    // VOICING
    #[id = "voices"]
    voices: IntParam, // 1..8

    #[id = "mono"]
    mono: BoolParam, // if true, retrigger same voice with glide
}

impl Default for Sub808Params {
    fn default() -> Self {
        Self {
            level: FloatParam::new(
                "Level",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(50.0)),
            decay_s: FloatParam::new(
                "Decay",
                0.6,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 3.0,
                    factor: 0.35,
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_2),
            vel_sens: FloatParam::new("VelSens", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 }),

            thump_amt_st: FloatParam::new(
                "Thump",
                12.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 36.0,
                },
            )
            .with_unit(" st"),
            thump_decay_s: FloatParam::new(
                "Thump Decay",
                0.06,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 0.25,
                    factor: 0.25,
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_3),

            glide_ms: FloatParam::new(
                "Glide",
                15.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 200.0,
                    factor: 0.25,
                },
            )
            .with_unit(" ms"),

            drive: FloatParam::new(
                "Drive",
                3.5,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            ),
            tone_hz: FloatParam::new(
                "Tone",
                700.0,
                FloatRange::Skewed {
                    min: 80.0,
                    max: 8000.0,
                    factor: 0.35,
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_0),

            voices: IntParam::new(
                "Voices",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_VOICES as i32,
                },
            ),
            mono: BoolParam::new("Mono", true),
        }
    }
}

struct Sub808 {
    params: Arc<Sub808Params>,
    voices: [SubVoice; MAX_VOICES],
    sample_rate: f32,
    // sustain pedal state (optional)
    sustain: bool,
}

impl Default for Sub808 {
    fn default() -> Self {
        Self {
            params: Arc::new(Sub808Params::default()),
            voices: core::array::from_fn(|_| SubVoice::new()),
            sample_rate: 44100.0,
            sustain: false,
        }
    }
}

impl Plugin for Sub808 {
    const NAME: &'static str = "Harmoniq Sub808";
    const VENDOR: &'static str = "Harmoniq";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: Some(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    #[cfg(feature = "editor")]
    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(ui::editor(self.params.clone()))
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f32;
        for v in &mut self.voices {
            v.reset(self.sample_rate);
        }
        true
    }

    fn reset(&mut self) {
        for v in &mut self.voices {
            v.reset(self.sample_rate);
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sr = self.sample_rate;
        let voices_active = self.params.voices.value() as usize;
        let mono = self.params.mono.value();

        // fetch params (avoid repeated atomic loads in inner loop)
        let level_db = self.params.level.smoothed.next();
        let level = util::db_to_gain_fast(level_db);
        let decay_s = self.params.decay_s.value();
        let vel_sens = self.params.vel_sens.value();
        let thump_st = self.params.thump_amt_st.value();
        let thump_decay_s = self.params.thump_decay_s.value();
        let glide_ms = self.params.glide_ms.value();
        let drive = self.params.drive.value();
        let tone_hz = self.params.tone_hz.value();

        // Handle incoming MIDI
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    let vel = (velocity as f32).clamp(0.0, 1.0);
                    if mono {
                        // Use voice 0, retrigger with glide
                        self.voices[0].note_on(
                            note,
                            vel,
                            sr,
                            decay_s,
                            thump_st,
                            thump_decay_s,
                            glide_ms,
                        );
                    } else {
                        // find free or steal oldest
                        let mut idx = None;
                        for (i, v) in self.voices[..voices_active].iter().enumerate() {
                            if !v.is_on() {
                                idx = Some(i);
                                break;
                            }
                        }
                        let use_idx = idx.unwrap_or_else(|| {
                            // steal quietest/oldest
                            let (mut best_i, mut best_age) = (0usize, 0u64);
                            for (i, v) in self.voices[..voices_active].iter().enumerate() {
                                if v.age > best_age {
                                    best_age = v.age;
                                    best_i = i;
                                }
                            }
                            best_i
                        });
                        self.voices[use_idx].note_on(
                            note,
                            vel,
                            sr,
                            decay_s,
                            thump_st,
                            thump_decay_s,
                            glide_ms,
                        );
                    }
                }
                NoteEvent::NoteOff { note, .. } => {
                    if mono {
                        if self.voices[0].current_note == Some(note) {
                            self.voices[0].note_off();
                        }
                    } else {
                        for v in &mut self.voices[..voices_active] {
                            if v.current_note == Some(note) {
                                v.note_off();
                            }
                        }
                    }
                }
                NoteEvent::PolyPressure { .. } => {}
                NoteEvent::Choke {
                    note_id: _, note, ..
                } => {
                    if mono {
                        if self.voices[0].current_note == Some(note) {
                            self.voices[0].kill();
                        }
                    } else {
                        for v in &mut self.voices[..voices_active] {
                            if v.current_note == Some(note) {
                                v.kill();
                            }
                        }
                    }
                }
                NoteEvent::MidiCC { cc, value, .. } => {
                    if cc == 64 {
                        self.sustain = value >= 64;
                    }
                }
                _ => {}
            }
        }

        // DSP: render frames
        let mut out = buffer.as_slice();
        let num_samples = out.samples();

        // simple one-pole LP after drive, per channel
        let g = (1.0 - (-2.0 * std::f32::consts::PI * tone_hz / sr).exp()).clamp(0.0, 1.0);
        let mut lp_l = 0.0f32;
        let mut lp_r = 0.0f32;

        for s in 0..num_samples {
            let mut acc = 0.0f32;
            // sum active voices
            for v in &mut self.voices[..voices_active] {
                acc += v.process_one(sr, decay_s, thump_st, thump_decay_s, glide_ms, vel_sens);
            }
            // drive
            let pre = acc * drive;
            let driven = fast_tanh(pre);
            // LP
            lp_l += g * (driven - lp_l);
            lp_r += g * (driven - lp_r);
            // output gain
            let out_s = (lp_l * level).clamp(-1.0, 1.0);
            // write stereo
            out.write(0, s, out_s);
            if out.channels() > 1 {
                out.write(1, s, out_s);
            }
        }

        ProcessStatus::Normal
    }
}

// Lightweight tanh for drive
#[inline(always)]
fn fast_tanh(x: f32) -> f32 {
    // fastapprox::tanh is branchless and cheap
    fastapprox::fast::tanh(x)
}

impl ClapPlugin for Sub808 {
    const CLAP_ID: &'static str = "com.harmoniq.sub808";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("808-style sub-bass synthesizer");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Synthesizer,
        ClapFeature::Mono,
        ClapFeature::Stereo,
        ClapFeature::Instrument,
    ];
}

nih_export_clap!(Sub808);

// ===================== Utility =====================
mod util {
    #[inline(always)]
    pub fn db_to_gain_fast(db: f32) -> f32 {
        // 20*log10(g) = dB -> g = 10^(dB/20)
        (db * 0.115129254f32).exp()
    }
}
