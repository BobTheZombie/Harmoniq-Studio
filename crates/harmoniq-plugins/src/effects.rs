use std::f32::consts::PI;
use std::sync::Arc;

use harmoniq_engine::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};
use harmoniq_plugin_sdk::{
    ContinuousParameterOptions, NativePlugin, ParameterDefinition, ParameterId, ParameterKind,
    ParameterLayout, ParameterSet, ParameterValue, PluginFactory, PluginParameterError,
};

const TWO_PI: f32 = PI * 2.0;

#[inline]
fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

#[derive(Debug, Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Debug, Clone, Copy)]
struct BiquadState {
    z1: f32,
    z2: f32,
}

impl BiquadState {
    fn new() -> Self {
        Self { z1: 0.0, z2: 0.0 }
    }

    fn process(&mut self, input: f32, coeffs: &BiquadCoeffs) -> f32 {
        let output = coeffs.b0 * input + self.z1;
        self.z1 = coeffs.b1 * input - coeffs.a1 * output + self.z2;
        self.z2 = coeffs.b2 * input - coeffs.a2 * output;
        output
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

const PARAM_EQ_OUTPUT_GAIN: &str = "output_gain";

const EQ_BAND_COUNT: usize = 4;

#[derive(Debug, Clone, Copy)]
enum EqBandKind {
    LowShelf,
    Peak,
    HighShelf,
}

#[derive(Debug, Clone, Copy)]
struct RangeSetting {
    min: f32,
    max: f32,
    default: f32,
    skew: Option<f32>,
}

impl RangeSetting {
    const fn new(min: f32, max: f32, default: f32, skew: Option<f32>) -> Self {
        Self {
            min,
            max,
            default,
            skew,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EqBandConfig {
    label: &'static str,
    kind: EqBandKind,
    enable_id: &'static str,
    freq_id: &'static str,
    gain_id: &'static str,
    q_id: &'static str,
    freq: RangeSetting,
    gain: RangeSetting,
    q: RangeSetting,
    q_label: &'static str,
    default_enabled: bool,
}

const EQ_BAND_CONFIGS: [EqBandConfig; EQ_BAND_COUNT] = [
    EqBandConfig {
        label: "Low Shelf",
        kind: EqBandKind::LowShelf,
        enable_id: "low_enable",
        freq_id: "low_freq",
        gain_id: "low_gain",
        q_id: "low_slope",
        freq: RangeSetting::new(30.0, 400.0, 80.0, Some(0.45)),
        gain: RangeSetting::new(-18.0, 18.0, 0.0, None),
        q: RangeSetting::new(0.3, 1.5, 0.7, None),
        q_label: "Slope",
        default_enabled: true,
    },
    EqBandConfig {
        label: "Low Mid",
        kind: EqBandKind::Peak,
        enable_id: "low_mid_enable",
        freq_id: "low_mid_freq",
        gain_id: "low_mid_gain",
        q_id: "low_mid_q",
        freq: RangeSetting::new(120.0, 2_000.0, 450.0, Some(0.55)),
        gain: RangeSetting::new(-18.0, 18.0, 0.0, None),
        q: RangeSetting::new(0.2, 5.0, 1.2, None),
        q_label: "Q",
        default_enabled: true,
    },
    EqBandConfig {
        label: "High Mid",
        kind: EqBandKind::Peak,
        enable_id: "high_mid_enable",
        freq_id: "high_mid_freq",
        gain_id: "high_mid_gain",
        q_id: "high_mid_q",
        freq: RangeSetting::new(1_000.0, 8_000.0, 2_500.0, Some(0.55)),
        gain: RangeSetting::new(-18.0, 18.0, 0.0, None),
        q: RangeSetting::new(0.2, 5.0, 1.0, None),
        q_label: "Q",
        default_enabled: true,
    },
    EqBandConfig {
        label: "High Shelf",
        kind: EqBandKind::HighShelf,
        enable_id: "high_enable",
        freq_id: "high_freq",
        gain_id: "high_gain",
        q_id: "high_slope",
        freq: RangeSetting::new(2_000.0, 18_000.0, 8_000.0, Some(0.45)),
        gain: RangeSetting::new(-18.0, 18.0, 0.0, None),
        q: RangeSetting::new(0.3, 1.5, 0.7, None),
        q_label: "Slope",
        default_enabled: true,
    },
];

#[derive(Debug, Clone)]
struct EqBandRuntime {
    enable_id: ParameterId,
    freq_id: ParameterId,
    gain_id: ParameterId,
    q_id: ParameterId,
    coeffs: BiquadCoeffs,
    states: Vec<BiquadState>,
    enabled: bool,
}

impl EqBandRuntime {
    fn new(config_index: usize) -> Self {
        let config = &EQ_BAND_CONFIGS[config_index];
        Self {
            enable_id: ParameterId::from(config.enable_id),
            freq_id: ParameterId::from(config.freq_id),
            gain_id: ParameterId::from(config.gain_id),
            q_id: ParameterId::from(config.q_id),
            coeffs: BiquadCoeffs {
                b0: 1.0,
                b1: 0.0,
                b2: 0.0,
                a1: 0.0,
                a2: 0.0,
            },
            states: Vec::new(),
            enabled: config.default_enabled,
        }
    }

    fn resize_states(&mut self, channels: usize) {
        self.states.resize(channels, BiquadState::new());
        for state in &mut self.states {
            state.reset();
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            for state in &mut self.states {
                state.reset();
            }
        }
        self.enabled = enabled;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ParametricEqBandPreset {
    pub enabled: bool,
    pub frequency: f32,
    pub gain: f32,
    pub q: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ParametricEqPreset {
    pub name: &'static str,
    pub output_gain: f32,
    pub bands: [ParametricEqBandPreset; EQ_BAND_COUNT],
}

pub const PARAMETRIC_EQ_FACTORY_PRESETS: &[ParametricEqPreset] = &[
    ParametricEqPreset {
        name: "Mastering Glue",
        output_gain: -0.5,
        bands: [
            ParametricEqBandPreset {
                enabled: true,
                frequency: 70.0,
                gain: 1.5,
                q: 0.8,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 420.0,
                gain: -1.8,
                q: 1.1,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 2_800.0,
                gain: 2.4,
                q: 1.3,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 11_500.0,
                gain: 3.0,
                q: 0.8,
            },
        ],
    },
    ParametricEqPreset {
        name: "Vocal Presence",
        output_gain: 0.0,
        bands: [
            ParametricEqBandPreset {
                enabled: false,
                frequency: 90.0,
                gain: 0.0,
                q: 0.7,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 250.0,
                gain: -2.5,
                q: 1.4,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 4_500.0,
                gain: 3.5,
                q: 0.9,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 9_500.0,
                gain: 2.2,
                q: 0.7,
            },
        ],
    },
    ParametricEqPreset {
        name: "Drum Bus Punch",
        output_gain: 0.0,
        bands: [
            ParametricEqBandPreset {
                enabled: true,
                frequency: 65.0,
                gain: 2.8,
                q: 0.9,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 180.0,
                gain: -2.2,
                q: 1.1,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 3_200.0,
                gain: 2.0,
                q: 1.0,
            },
            ParametricEqBandPreset {
                enabled: true,
                frequency: 12_000.0,
                gain: 2.5,
                q: 0.9,
            },
        ],
    },
];

#[derive(Debug, Clone)]
pub struct ParametricEqPlugin {
    sample_rate: f32,
    bands: Vec<EqBandRuntime>,
    output_gain: f32,
    output_gain_id: ParameterId,
    parameters: ParameterSet,
}

impl Default for ParametricEqPlugin {
    fn default() -> Self {
        let layout = parametric_eq_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            bands: (0..EQ_BAND_COUNT).map(EqBandRuntime::new).collect(),
            output_gain: 1.0,
            output_gain_id: ParameterId::from(PARAM_EQ_OUTPUT_GAIN),
            parameters,
        };
        plugin.refresh_output_gain();
        plugin.update_all_bands();
        plugin
    }
}

impl ParametricEqPlugin {
    fn refresh_output_gain(&mut self) {
        let gain_db = self
            .parameters
            .get(&self.output_gain_id)
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.0);
        self.output_gain = db_to_gain(gain_db);
    }

    fn identity_coeffs() -> BiquadCoeffs {
        BiquadCoeffs {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    fn update_all_bands(&mut self) {
        for index in 0..self.bands.len() {
            self.update_band(index);
        }
    }

    fn update_band(&mut self, index: usize) {
        let config = EQ_BAND_CONFIGS[index];
        let (enabled, freq, gain_db, q) = {
            let band = &self.bands[index];
            let enabled = self
                .parameters
                .get(&band.enable_id)
                .and_then(ParameterValue::as_toggle)
                .unwrap_or(config.default_enabled);
            let freq = self
                .parameters
                .get(&band.freq_id)
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(config.freq.default)
                .clamp(config.freq.min, config.freq.max);
            let gain_db = self
                .parameters
                .get(&band.gain_id)
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(config.gain.default)
                .clamp(config.gain.min, config.gain.max);
            let q = self
                .parameters
                .get(&band.q_id)
                .and_then(ParameterValue::as_continuous)
                .unwrap_or(config.q.default)
                .clamp(config.q.min, config.q.max);
            (enabled, freq, gain_db, q)
        };

        let band = &mut self.bands[index];
        band.set_enabled(enabled);
        if !enabled {
            band.coeffs = Self::identity_coeffs();
            return;
        }

        let sample_rate = self.sample_rate.max(1.0);
        let limited_freq = freq.min(sample_rate * 0.45).max(10.0);
        band.coeffs = match config.kind {
            EqBandKind::LowShelf => compute_low_shelf(limited_freq, gain_db, q, sample_rate),
            EqBandKind::HighShelf => compute_high_shelf(limited_freq, gain_db, q, sample_rate),
            EqBandKind::Peak => compute_peak(limited_freq, gain_db, q, sample_rate),
        };
    }

    pub fn apply_preset(&mut self, preset: &ParametricEqPreset) {
        let _ = self.parameters.set(
            &self.output_gain_id,
            ParameterValue::from(preset.output_gain),
        );
        self.refresh_output_gain();

        for (index, band_preset) in preset.bands.iter().enumerate() {
            if let Some(band) = self.bands.get(index) {
                let _ = self
                    .parameters
                    .set(&band.enable_id, ParameterValue::from(band_preset.enabled));
                let _ = self
                    .parameters
                    .set(&band.freq_id, ParameterValue::from(band_preset.frequency));
                let _ = self
                    .parameters
                    .set(&band.gain_id, ParameterValue::from(band_preset.gain));
                let _ = self
                    .parameters
                    .set(&band.q_id, ParameterValue::from(band_preset.q));
            }
            self.update_band(index);
        }
    }
}

impl AudioProcessor for ParametricEqPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.parametric_eq",
            "Parametric EQ",
            "Harmoniq Labs",
        )
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        let channels = config.layout.channels() as usize;
        for band in &mut self.bands {
            band.resize_states(channels);
        }
        self.update_all_bands();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel_index, channel) in buffer.channels_mut().enumerate() {
            for sample in channel.iter_mut() {
                let mut value = *sample;
                for band in &mut self.bands {
                    if band.enabled {
                        if let Some(state) = band.states.get_mut(channel_index) {
                            value = state.process(value, &band.coeffs);
                        }
                    }
                }
                *sample = value * self.output_gain;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for ParametricEqPlugin {
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
        if id == &self.output_gain_id {
            self.refresh_output_gain();
        } else {
            for index in 0..self.bands.len() {
                let band = &self.bands[index];
                if id == &band.enable_id
                    || id == &band.freq_id
                    || id == &band.gain_id
                    || id == &band.q_id
                {
                    self.update_band(index);
                    break;
                }
            }
        }
        Ok(())
    }
}

fn parametric_eq_layout() -> ParameterLayout {
    let mut parameters = Vec::new();
    let output_gain_options = ContinuousParameterOptions::new(-24.0..=24.0, 0.0);
    parameters.push(
        ParameterDefinition::new(
            PARAM_EQ_OUTPUT_GAIN,
            "Output Gain",
            ParameterKind::Continuous(output_gain_options),
        )
        .with_unit("dB")
        .with_description("Post-EQ output trim"),
    );

    for config in EQ_BAND_CONFIGS.iter() {
        parameters.push(
            ParameterDefinition::new(
                config.enable_id,
                format!("{} Enable", config.label),
                ParameterKind::Toggle {
                    default: config.default_enabled,
                },
            )
            .with_description(format!("Enable or bypass the {} band", config.label)),
        );

        let mut freq_options =
            ContinuousParameterOptions::new(config.freq.min..=config.freq.max, config.freq.default);
        if let Some(skew) = config.freq.skew {
            freq_options = freq_options.with_skew(skew);
        }
        parameters.push(
            ParameterDefinition::new(
                config.freq_id,
                format!("{} Frequency", config.label),
                ParameterKind::Continuous(freq_options),
            )
            .with_unit("Hz")
            .with_description(format!("Center frequency for the {} band", config.label)),
        );

        let gain_options =
            ContinuousParameterOptions::new(config.gain.min..=config.gain.max, config.gain.default);
        parameters.push(
            ParameterDefinition::new(
                config.gain_id,
                format!("{} Gain", config.label),
                ParameterKind::Continuous(gain_options),
            )
            .with_unit("dB")
            .with_description(format!("Gain applied by the {} band", config.label)),
        );

        let mut q_options =
            ContinuousParameterOptions::new(config.q.min..=config.q.max, config.q.default);
        if let Some(skew) = config.q.skew {
            q_options = q_options.with_skew(skew);
        }
        parameters.push(
            ParameterDefinition::new(
                config.q_id,
                format!("{} {}", config.label, config.q_label),
                ParameterKind::Continuous(q_options),
            )
            .with_description(format!("Bandwidth control for the {} band", config.label)),
        );
    }

    ParameterLayout::new(parameters)
}

fn compute_peak(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let sample_rate = sample_rate.max(1.0);
    let omega = TWO_PI * (frequency / sample_rate);
    let cos = omega.cos();
    let sin = omega.sin();
    let q = q.max(0.05);
    let alpha = sin / (2.0 * q);
    let a = 10.0_f32.powf(gain_db / 40.0);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos;
    let a2 = 1.0 - alpha / a;

    let inv_a0 = 1.0 / a0.max(1e-6);
    BiquadCoeffs {
        b0: b0 * inv_a0,
        b1: b1 * inv_a0,
        b2: b2 * inv_a0,
        a1: a1 * inv_a0,
        a2: a2 * inv_a0,
    }
}

fn shelf_alpha(a: f32, slope: f32, sin: f32) -> f32 {
    let slope = slope.max(0.1);
    let s = ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).max(0.0);
    sin / 2.0 * s.sqrt()
}

fn compute_low_shelf(freq: f32, gain_db: f32, slope: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let sample_rate = sample_rate.max(1.0);
    let omega = TWO_PI * (frequency / sample_rate);
    let cos = omega.cos();
    let sin = omega.sin();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let alpha = shelf_alpha(a, slope, sin);
    let sqrt_a = a.sqrt();
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
    let a2 = (a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha;

    let inv_a0 = 1.0 / a0.max(1e-6);
    BiquadCoeffs {
        b0: b0 * inv_a0,
        b1: b1 * inv_a0,
        b2: b2 * inv_a0,
        a1: a1 * inv_a0,
        a2: a2 * inv_a0,
    }
}

fn compute_high_shelf(freq: f32, gain_db: f32, slope: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let sample_rate = sample_rate.max(1.0);
    let omega = TWO_PI * (frequency / sample_rate);
    let cos = omega.cos();
    let sin = omega.sin();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let alpha = shelf_alpha(a, slope, sin);
    let sqrt_a = a.sqrt();
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos);
    let a2 = (a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha;

    let inv_a0 = 1.0 / a0.max(1e-6);
    BiquadCoeffs {
        b0: b0 * inv_a0,
        b1: b1 * inv_a0,
        b2: b2 * inv_a0,
        a1: a1 * inv_a0,
        a2: a2 * inv_a0,
    }
}

pub struct ParametricEqFactory;

impl PluginFactory for ParametricEqFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.parametric_eq",
            "Parametric EQ",
            "Harmoniq Labs",
        )
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(parametric_eq_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(ParametricEqPlugin::default())
    }
}

const PARAM_COMP_THRESHOLD: &str = "threshold";
const PARAM_COMP_RATIO: &str = "ratio";
const PARAM_COMP_ATTACK: &str = "attack";
const PARAM_COMP_RELEASE: &str = "release";
const PARAM_COMP_MAKEUP: &str = "makeup";

fn time_to_coeff(ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / ((ms.max(0.1) / 1_000.0) * sample_rate.max(1.0))).exp()
}

#[derive(Debug, Clone)]
pub struct CompressorPlugin {
    sample_rate: f32,
    threshold: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    makeup_gain: f32,
    envelope: Vec<f32>,
    gain: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for CompressorPlugin {
    fn default() -> Self {
        let layout = compressor_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            threshold: -18.0,
            ratio: 4.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            makeup_gain: 0.0,
            envelope: Vec::new(),
            gain: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl CompressorPlugin {
    fn refresh_from_parameters(&mut self) {
        self.threshold = self
            .parameters
            .get(&ParameterId::from(PARAM_COMP_THRESHOLD))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(-18.0);
        self.ratio = self
            .parameters
            .get(&ParameterId::from(PARAM_COMP_RATIO))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(4.0);
        let attack = self
            .parameters
            .get(&ParameterId::from(PARAM_COMP_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(10.0);
        let release = self
            .parameters
            .get(&ParameterId::from(PARAM_COMP_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(100.0);
        self.makeup_gain = self
            .parameters
            .get(&ParameterId::from(PARAM_COMP_MAKEUP))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.0);
        self.attack_coeff = time_to_coeff(attack, self.sample_rate);
        self.release_coeff = time_to_coeff(release, self.sample_rate);
    }
}

impl AudioProcessor for CompressorPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.compressor", "Compressor", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope = vec![0.0; config.layout.channels() as usize];
        self.gain = vec![1.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, (env, gain)) in buffer
            .channels_mut()
            .zip(self.envelope.iter_mut().zip(self.gain.iter_mut()))
        {
            for sample in channel.iter_mut() {
                let input = *sample;
                let level = input.abs().max(1e-6);
                let coeff = if level > *env {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                *env = coeff * *env + (1.0 - coeff) * level;

                let env_db = 20.0 * env.log10();
                let mut gain_db = 0.0;
                if env_db > self.threshold {
                    let delta = env_db - self.threshold;
                    let compressed = delta / self.ratio;
                    gain_db = (self.threshold + compressed) - env_db;
                }
                gain_db += self.makeup_gain;
                *gain = db_to_gain(gain_db);
                *sample *= *gain;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for CompressorPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_COMP_THRESHOLD
                | PARAM_COMP_RATIO
                | PARAM_COMP_ATTACK
                | PARAM_COMP_RELEASE
                | PARAM_COMP_MAKEUP
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn compressor_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_COMP_THRESHOLD,
            "Threshold",
            ParameterKind::continuous(-60.0..=0.0, -18.0),
        )
        .with_unit("dB")
        .with_description("Level above which compression engages"),
        ParameterDefinition::new(
            PARAM_COMP_RATIO,
            "Ratio",
            ParameterKind::continuous(1.0..=20.0, 4.0),
        )
        .with_description("Amount of gain reduction applied"),
        ParameterDefinition::new(
            PARAM_COMP_ATTACK,
            "Attack",
            ParameterKind::continuous(0.1..=200.0, 10.0),
        )
        .with_unit("ms")
        .with_description("Envelope attack time"),
        ParameterDefinition::new(
            PARAM_COMP_RELEASE,
            "Release",
            ParameterKind::continuous(5.0..=500.0, 100.0),
        )
        .with_unit("ms")
        .with_description("Envelope release time"),
        ParameterDefinition::new(
            PARAM_COMP_MAKEUP,
            "Makeup",
            ParameterKind::continuous(-12.0..=12.0, 0.0),
        )
        .with_unit("dB")
        .with_description("Output gain applied after compression"),
    ])
}

pub struct CompressorFactory;

impl PluginFactory for CompressorFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.compressor", "Compressor", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(compressor_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(CompressorPlugin::default())
    }
}

const PARAM_LIMITER_CEILING: &str = "ceiling";
const PARAM_LIMITER_RELEASE: &str = "release";

#[derive(Debug, Clone)]
pub struct LimiterPlugin {
    sample_rate: f32,
    ceiling: f32,
    release_coeff: f32,
    gain: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for LimiterPlugin {
    fn default() -> Self {
        let layout = limiter_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            ceiling: db_to_gain(-0.3),
            release_coeff: 0.0,
            gain: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl LimiterPlugin {
    fn refresh_from_parameters(&mut self) {
        let ceiling_db = self
            .parameters
            .get(&ParameterId::from(PARAM_LIMITER_CEILING))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(-0.3);
        let release_ms = self
            .parameters
            .get(&ParameterId::from(PARAM_LIMITER_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(80.0);
        self.ceiling = db_to_gain(ceiling_db);
        self.release_coeff = time_to_coeff(release_ms, self.sample_rate);
    }
}

impl AudioProcessor for LimiterPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.limiter", "Limiter", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.gain = vec![1.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, gain) in buffer.channels_mut().zip(self.gain.iter_mut()) {
            for sample in channel.iter_mut() {
                let abs = sample.abs();
                if abs > self.ceiling && abs > 1e-6 {
                    let target = (self.ceiling / abs).min(*gain);
                    *gain = target;
                } else {
                    *gain = *gain + (1.0 - *gain) * (1.0 - self.release_coeff);
                }
                *sample *= *gain;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for LimiterPlugin {
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
        if matches!(id.as_str(), PARAM_LIMITER_CEILING | PARAM_LIMITER_RELEASE) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn limiter_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_LIMITER_CEILING,
            "Ceiling",
            ParameterKind::continuous(-6.0..=0.0, -0.3),
        )
        .with_unit("dB")
        .with_description("Maximum output level before limiting"),
        ParameterDefinition::new(
            PARAM_LIMITER_RELEASE,
            "Release",
            ParameterKind::continuous(5.0..=200.0, 80.0),
        )
        .with_unit("ms")
        .with_description("Time for the limiter to recover"),
    ])
}

pub struct LimiterFactory;

impl PluginFactory for LimiterFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.limiter", "Limiter", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(limiter_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(LimiterPlugin::default())
    }
}

const PARAM_REVERB_ROOM: &str = "room";
const PARAM_REVERB_DAMP: &str = "damp";
const PARAM_REVERB_MIX: &str = "mix";
const PARAM_REVERB_PREDELAY: &str = "predelay";

const COMB_LENGTHS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS_LENGTHS: [usize; 2] = [556, 441];

#[derive(Debug, Clone)]
struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    damp: f32,
    filter_store: f32,
}

impl CombFilter {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            feedback: 0.7,
            damp: 0.3,
            filter_store: 0.0,
        }
    }

    fn set_params(&mut self, feedback: f32, damp: f32) {
        self.feedback = feedback.clamp(0.0, 0.99);
        self.damp = damp.clamp(0.0, 0.99);
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * (1.0 - self.damp) + self.filter_store * self.damp;
        self.buffer[self.index] = input + self.filter_store * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
        self.filter_store = 0.0;
    }
}

#[derive(Debug, Clone)]
struct AllPassFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}

impl AllPassFilter {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            feedback: 0.5,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buf_out = self.buffer[self.index];
        let output = -input + buf_out;
        self.buffer[self.index] = input + buf_out * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }
}

#[derive(Debug, Clone)]
struct ReverbChannel {
    combs: Vec<CombFilter>,
    allpasses: Vec<AllPassFilter>,
    predelay: Vec<f32>,
    predelay_index: usize,
}

impl ReverbChannel {
    fn new(sample_rate: f32, room: f32, damp: f32, predelay_samples: usize) -> Self {
        let scale = (sample_rate / 44_100.0).max(0.25);
        let mut combs = Vec::new();
        for length in COMB_LENGTHS {
            let len = ((length as f32) * scale).round().max(1.0) as usize;
            let mut comb = CombFilter::new(len);
            comb.set_params(0.75 + room * 0.2, damp);
            combs.push(comb);
        }
        let mut allpasses = Vec::new();
        for length in ALLPASS_LENGTHS {
            let len = ((length as f32) * scale).round().max(1.0) as usize;
            allpasses.push(AllPassFilter::new(len));
        }
        let mut predelay = Vec::new();
        if predelay_samples > 0 {
            predelay.resize(predelay_samples, 0.0);
        }
        Self {
            combs,
            allpasses,
            predelay,
            predelay_index: 0,
        }
    }

    fn update(&mut self, room: f32, damp: f32, predelay_samples: usize) {
        let feedback = 0.75 + room * 0.2;
        for comb in &mut self.combs {
            comb.set_params(feedback, damp);
        }
        if predelay_samples == 0 {
            self.predelay.clear();
            self.predelay_index = 0;
        } else {
            if self.predelay.len() != predelay_samples {
                self.predelay = vec![0.0; predelay_samples];
                self.predelay_index = 0;
            }
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        for comb in &mut self.combs {
            comb.reset();
        }
        for allpass in &mut self.allpasses {
            allpass.reset();
        }
        if !self.predelay.is_empty() {
            self.predelay.fill(0.0);
            self.predelay_index = 0;
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let predelayed = if self.predelay.is_empty() {
            input
        } else {
            let out = self.predelay[self.predelay_index];
            self.predelay[self.predelay_index] = input;
            self.predelay_index = (self.predelay_index + 1) % self.predelay.len();
            out
        };

        let mut sum = 0.0;
        for comb in &mut self.combs {
            sum += comb.process(predelayed);
        }
        for allpass in &mut self.allpasses {
            sum = allpass.process(sum);
        }
        sum / (self.combs.len() as f32)
    }
}

#[derive(Debug, Clone)]
pub struct ReverbPlugin {
    sample_rate: f32,
    room: f32,
    damp: f32,
    mix: f32,
    predelay_samples: usize,
    channels: Vec<ReverbChannel>,
    parameters: ParameterSet,
}

impl Default for ReverbPlugin {
    fn default() -> Self {
        let layout = reverb_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            room: 0.5,
            damp: 0.3,
            mix: 0.3,
            predelay_samples: 0,
            channels: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl ReverbPlugin {
    fn refresh_from_parameters(&mut self) {
        self.room = self
            .parameters
            .get(&ParameterId::from(PARAM_REVERB_ROOM))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        self.damp = self
            .parameters
            .get(&ParameterId::from(PARAM_REVERB_DAMP))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3)
            .clamp(0.0, 0.99);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_REVERB_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3)
            .clamp(0.0, 1.0);
        let predelay = self
            .parameters
            .get(&ParameterId::from(PARAM_REVERB_PREDELAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(20.0);
        self.predelay_samples = ((predelay / 1_000.0) * self.sample_rate).round() as usize;
        for channel in &mut self.channels {
            channel.update(self.room, self.damp, self.predelay_samples);
        }
    }
}

impl AudioProcessor for ReverbPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.reverb", "Reverb", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.channels = (0..config.layout.channels() as usize)
            .map(|_| {
                ReverbChannel::new(
                    self.sample_rate,
                    self.room,
                    self.damp,
                    self.predelay_samples,
                )
            })
            .collect();
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, state) in buffer.channels_mut().zip(self.channels.iter_mut()) {
            for sample in channel.iter_mut() {
                let dry = *sample;
                let wet = state.process(dry);
                *sample = dry * (1.0 - self.mix) + wet * self.mix;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for ReverbPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_REVERB_ROOM | PARAM_REVERB_DAMP | PARAM_REVERB_MIX | PARAM_REVERB_PREDELAY
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn reverb_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_REVERB_ROOM,
            "Room Size",
            ParameterKind::continuous(0.0..=1.0, 0.5),
        )
        .with_description("Overall size and decay of the virtual room"),
        ParameterDefinition::new(
            PARAM_REVERB_DAMP,
            "Dampening",
            ParameterKind::continuous(0.0..=1.0, 0.3),
        )
        .with_description("High-frequency absorption"),
        ParameterDefinition::new(
            PARAM_REVERB_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.3),
        )
        .with_description("Wet/dry balance"),
        ParameterDefinition::new(
            PARAM_REVERB_PREDELAY,
            "Pre-delay",
            ParameterKind::continuous(0.0..=200.0, 20.0),
        )
        .with_unit("ms")
        .with_description("Time before reverberation begins"),
    ])
}

pub struct ReverbFactory;

impl PluginFactory for ReverbFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.reverb", "Reverb", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(reverb_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(ReverbPlugin::default())
    }
}

const PARAM_DELAY_TIME: &str = "time";
const PARAM_DELAY_FEEDBACK: &str = "feedback";
const PARAM_DELAY_MIX: &str = "mix";

#[derive(Debug, Clone)]
struct DelayLine {
    buffer: Vec<f32>,
    index: usize,
}

impl DelayLine {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    fn set_length(&mut self, length: usize) {
        if self.buffer.len() != length.max(1) {
            self.buffer = vec![0.0; length.max(1)];
            self.index = 0;
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }

    fn process(&mut self, input: f32, feedback: f32) -> f32 {
        let delayed = self.buffer[self.index];
        self.buffer[self.index] = input + delayed * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        delayed
    }
}

#[derive(Debug, Clone)]
pub struct DelayPlugin {
    sample_rate: f32,
    delay_samples: usize,
    feedback: f32,
    mix: f32,
    lines: Vec<DelayLine>,
    parameters: ParameterSet,
}

impl Default for DelayPlugin {
    fn default() -> Self {
        let layout = delay_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            delay_samples: 1,
            feedback: 0.35,
            mix: 0.35,
            lines: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl DelayPlugin {
    fn refresh_from_parameters(&mut self) {
        let time_ms = self
            .parameters
            .get(&ParameterId::from(PARAM_DELAY_TIME))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(400.0);
        self.delay_samples = ((time_ms / 1_000.0) * self.sample_rate).round().max(1.0) as usize;
        self.feedback = self
            .parameters
            .get(&ParameterId::from(PARAM_DELAY_FEEDBACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.35)
            .clamp(0.0, 0.95);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_DELAY_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.35)
            .clamp(0.0, 1.0);
        for line in &mut self.lines {
            line.set_length(self.delay_samples);
        }
    }
}

impl AudioProcessor for DelayPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.delay", "Delay", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.lines = (0..config.layout.channels() as usize)
            .map(|_| DelayLine::new(self.delay_samples))
            .collect();
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, line) in buffer.channels_mut().zip(self.lines.iter_mut()) {
            for sample in channel.iter_mut() {
                let delayed = line.process(*sample, self.feedback);
                *sample = *sample * (1.0 - self.mix) + delayed * self.mix;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for DelayPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_DELAY_TIME | PARAM_DELAY_FEEDBACK | PARAM_DELAY_MIX
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn delay_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_DELAY_TIME,
            "Time",
            ParameterKind::continuous(1.0..=2_000.0, 400.0),
        )
        .with_unit("ms")
        .with_description("Delay time"),
        ParameterDefinition::new(
            PARAM_DELAY_FEEDBACK,
            "Feedback",
            ParameterKind::continuous(0.0..=0.95, 0.35),
        )
        .with_description("Amount of delayed signal fed back"),
        ParameterDefinition::new(
            PARAM_DELAY_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.35),
        )
        .with_description("Wet/dry balance"),
    ])
}

pub struct DelayFactory;

impl PluginFactory for DelayFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.delay", "Delay", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(delay_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(DelayPlugin::default())
    }
}

const PARAM_CHORUS_RATE: &str = "rate";
const PARAM_CHORUS_DEPTH: &str = "depth";
const PARAM_CHORUS_MIX: &str = "mix";
const PARAM_CHORUS_FEEDBACK: &str = "feedback";

#[derive(Debug, Clone)]
struct ModulatedDelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl ModulatedDelayLine {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(2)],
            write: 0,
        }
    }

    fn resize(&mut self, length: usize) {
        if self.buffer.len() != length.max(2) {
            self.buffer = vec![0.0; length.max(2)];
            self.write = 0;
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write = 0;
    }

    fn process(&mut self, input: f32, delay_samples: f32, feedback: f32) -> f32 {
        let len = self.buffer.len();
        let mut read_pos = self.write as f32 - delay_samples;
        while read_pos < 0.0 {
            read_pos += len as f32;
        }
        let idx0 = read_pos.floor() as usize % len;
        let idx1 = (idx0 + 1) % len;
        let frac = read_pos - (idx0 as f32);
        let delayed = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;
        self.buffer[self.write] = input + delayed * feedback;
        self.write = (self.write + 1) % len;
        delayed
    }
}

#[derive(Debug, Clone)]
pub struct ChorusPlugin {
    sample_rate: f32,
    rate: f32,
    depth_ms: f32,
    base_ms: f32,
    feedback: f32,
    mix: f32,
    max_delay_samples: usize,
    lines: Vec<ModulatedDelayLine>,
    phases: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for ChorusPlugin {
    fn default() -> Self {
        let layout = chorus_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            rate: 1.2,
            depth_ms: 8.0,
            base_ms: 15.0,
            feedback: 0.15,
            mix: 0.4,
            max_delay_samples: 1,
            lines: Vec::new(),
            phases: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl ChorusPlugin {
    fn refresh_from_parameters(&mut self) {
        self.rate = self
            .parameters
            .get(&ParameterId::from(PARAM_CHORUS_RATE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.2)
            .clamp(0.1, 5.0);
        self.depth_ms = self
            .parameters
            .get(&ParameterId::from(PARAM_CHORUS_DEPTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(8.0)
            .clamp(1.0, 20.0);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_CHORUS_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.4)
            .clamp(0.0, 1.0);
        self.feedback = self
            .parameters
            .get(&ParameterId::from(PARAM_CHORUS_FEEDBACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.15)
            .clamp(0.0, 0.9);
        self.max_delay_samples = (((self.base_ms + self.depth_ms) / 1_000.0) * self.sample_rate)
            .ceil()
            .max(2.0) as usize;
        for line in &mut self.lines {
            line.resize(self.max_delay_samples + 2);
        }
    }
}

impl AudioProcessor for ChorusPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.chorus", "Chorus", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.max_delay_samples = (((self.base_ms + self.depth_ms) / 1_000.0) * self.sample_rate)
            .ceil()
            .max(2.0) as usize;
        self.lines = (0..config.layout.channels() as usize)
            .map(|_| ModulatedDelayLine::new(self.max_delay_samples + 2))
            .collect();
        self.phases = vec![0.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let depth_samples = (self.depth_ms / 1_000.0) * self.sample_rate;
        let base_samples = (self.base_ms / 1_000.0) * self.sample_rate;
        let phase_inc = self.rate / self.sample_rate;
        for ((channel, line), phase) in buffer
            .channels_mut()
            .zip(self.lines.iter_mut())
            .zip(self.phases.iter_mut())
        {
            for sample in channel.iter_mut() {
                *phase = (*phase + phase_inc) % 1.0;
                let lfo = (*phase * TWO_PI).sin();
                let delay = base_samples + depth_samples * (lfo * 0.5 + 0.5);
                let delayed = line.process(*sample, delay, self.feedback);
                *sample = *sample * (1.0 - self.mix) + delayed * self.mix;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for ChorusPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_CHORUS_RATE | PARAM_CHORUS_DEPTH | PARAM_CHORUS_MIX | PARAM_CHORUS_FEEDBACK
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn chorus_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_CHORUS_RATE,
            "Rate",
            ParameterKind::continuous(0.1..=5.0, 1.2),
        )
        .with_unit("Hz")
        .with_description("Modulation rate"),
        ParameterDefinition::new(
            PARAM_CHORUS_DEPTH,
            "Depth",
            ParameterKind::continuous(1.0..=20.0, 8.0),
        )
        .with_unit("ms")
        .with_description("Modulation depth"),
        ParameterDefinition::new(
            PARAM_CHORUS_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.4),
        )
        .with_description("Wet/dry balance"),
        ParameterDefinition::new(
            PARAM_CHORUS_FEEDBACK,
            "Feedback",
            ParameterKind::continuous(0.0..=0.9, 0.15),
        )
        .with_description("Amount of delayed signal fed back"),
    ])
}

pub struct ChorusFactory;

impl PluginFactory for ChorusFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.chorus", "Chorus", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(chorus_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(ChorusPlugin::default())
    }
}

const PARAM_FLANGER_RATE: &str = "rate";
const PARAM_FLANGER_DEPTH: &str = "depth";
const PARAM_FLANGER_MIX: &str = "mix";
const PARAM_FLANGER_FEEDBACK: &str = "feedback";

#[derive(Debug, Clone)]
pub struct FlangerPlugin {
    sample_rate: f32,
    rate: f32,
    depth_ms: f32,
    base_ms: f32,
    feedback: f32,
    mix: f32,
    max_delay_samples: usize,
    lines: Vec<ModulatedDelayLine>,
    phases: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for FlangerPlugin {
    fn default() -> Self {
        let layout = flanger_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            rate: 0.25,
            depth_ms: 2.5,
            base_ms: 0.8,
            feedback: 0.4,
            mix: 0.5,
            max_delay_samples: 1,
            lines: Vec::new(),
            phases: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl FlangerPlugin {
    fn refresh_from_parameters(&mut self) {
        self.rate = self
            .parameters
            .get(&ParameterId::from(PARAM_FLANGER_RATE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.25)
            .clamp(0.05, 5.0);
        self.depth_ms = self
            .parameters
            .get(&ParameterId::from(PARAM_FLANGER_DEPTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(2.5)
            .clamp(0.1, 10.0);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_FLANGER_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        self.feedback = self
            .parameters
            .get(&ParameterId::from(PARAM_FLANGER_FEEDBACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.4)
            .clamp(0.0, 0.95);
        self.max_delay_samples = (((self.base_ms + self.depth_ms) / 1_000.0) * self.sample_rate)
            .ceil()
            .max(2.0) as usize;
        for line in &mut self.lines {
            line.resize(self.max_delay_samples + 2);
        }
    }
}

impl AudioProcessor for FlangerPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.flanger", "Flanger", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.max_delay_samples = (((self.base_ms + self.depth_ms) / 1_000.0) * self.sample_rate)
            .ceil()
            .max(2.0) as usize;
        self.lines = (0..config.layout.channels() as usize)
            .map(|_| ModulatedDelayLine::new(self.max_delay_samples + 2))
            .collect();
        self.phases = vec![0.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let depth_samples = (self.depth_ms / 1_000.0) * self.sample_rate;
        let base_samples = (self.base_ms / 1_000.0) * self.sample_rate;
        let phase_inc = self.rate / self.sample_rate;
        for ((channel, line), phase) in buffer
            .channels_mut()
            .zip(self.lines.iter_mut())
            .zip(self.phases.iter_mut())
        {
            for sample in channel.iter_mut() {
                *phase = (*phase + phase_inc) % 1.0;
                let lfo = (*phase * TWO_PI).sin();
                let delay = base_samples + depth_samples * lfo;
                let delay = delay.abs();
                let delayed = line.process(*sample, delay, self.feedback);
                *sample = *sample * (1.0 - self.mix) + delayed * self.mix;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for FlangerPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_FLANGER_RATE | PARAM_FLANGER_DEPTH | PARAM_FLANGER_MIX | PARAM_FLANGER_FEEDBACK
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn flanger_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_FLANGER_RATE,
            "Rate",
            ParameterKind::continuous(0.05..=5.0, 0.25),
        )
        .with_unit("Hz")
        .with_description("Modulation rate"),
        ParameterDefinition::new(
            PARAM_FLANGER_DEPTH,
            "Depth",
            ParameterKind::continuous(0.1..=10.0, 2.5),
        )
        .with_unit("ms")
        .with_description("Modulation depth"),
        ParameterDefinition::new(
            PARAM_FLANGER_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.5),
        )
        .with_description("Wet/dry balance"),
        ParameterDefinition::new(
            PARAM_FLANGER_FEEDBACK,
            "Feedback",
            ParameterKind::continuous(0.0..=0.95, 0.4),
        )
        .with_description("Feedback amount"),
    ])
}

pub struct FlangerFactory;

impl PluginFactory for FlangerFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.flanger", "Flanger", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(flanger_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(FlangerPlugin::default())
    }
}

const PARAM_PHASER_RATE: &str = "rate";
const PARAM_PHASER_DEPTH: &str = "depth";
const PARAM_PHASER_CENTER: &str = "center";
const PARAM_PHASER_FEEDBACK: &str = "feedback";
const PARAM_PHASER_MIX: &str = "mix";

const PHASER_STAGES: usize = 4;

#[derive(Debug, Clone)]
struct PhaserStage {
    z1: f32,
}

impl PhaserStage {
    fn new() -> Self {
        Self { z1: 0.0 }
    }

    fn process(&mut self, input: f32, coeff: f32) -> f32 {
        let y = -coeff * input + self.z1;
        self.z1 = input + coeff * y;
        y
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.z1 = 0.0;
    }
}

#[derive(Debug, Clone)]
struct PhaserChannel {
    stages: Vec<PhaserStage>,
    feedback_state: f32,
}

impl PhaserChannel {
    fn new() -> Self {
        Self {
            stages: (0..PHASER_STAGES).map(|_| PhaserStage::new()).collect(),
            feedback_state: 0.0,
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
        self.feedback_state = 0.0;
    }
}

#[derive(Debug, Clone)]
pub struct PhaserPlugin {
    sample_rate: f32,
    rate: f32,
    depth: f32,
    center_freq: f32,
    feedback: f32,
    mix: f32,
    channels: Vec<PhaserChannel>,
    phases: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for PhaserPlugin {
    fn default() -> Self {
        let layout = phaser_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            rate: 0.3,
            depth: 0.7,
            center_freq: 800.0,
            feedback: 0.4,
            mix: 0.5,
            channels: Vec::new(),
            phases: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl PhaserPlugin {
    fn refresh_from_parameters(&mut self) {
        self.rate = self
            .parameters
            .get(&ParameterId::from(PARAM_PHASER_RATE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.3)
            .clamp(0.05, 5.0);
        self.depth = self
            .parameters
            .get(&ParameterId::from(PARAM_PHASER_DEPTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.7)
            .clamp(0.0, 1.0);
        self.center_freq = self
            .parameters
            .get(&ParameterId::from(PARAM_PHASER_CENTER))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(800.0)
            .clamp(50.0, 4_000.0);
        self.feedback = self
            .parameters
            .get(&ParameterId::from(PARAM_PHASER_FEEDBACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.4)
            .clamp(0.0, 0.95);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_PHASER_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
    }
}

impl AudioProcessor for PhaserPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.phaser", "Phaser", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.channels = (0..config.layout.channels() as usize)
            .map(|_| PhaserChannel::new())
            .collect();
        self.phases = vec![0.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let min_freq = (self.center_freq * (1.0 - self.depth)).max(40.0);
        let max_freq = (self.center_freq * (1.0 + self.depth)).min(self.sample_rate * 0.45);
        let phase_inc = self.rate / self.sample_rate;
        for ((channel, state), phase) in buffer
            .channels_mut()
            .zip(self.channels.iter_mut())
            .zip(self.phases.iter_mut())
        {
            for sample in channel.iter_mut() {
                *phase = (*phase + phase_inc) % 1.0;
                let lfo = (*phase * TWO_PI).sin() * 0.5 + 0.5;
                let freq = min_freq + (max_freq - min_freq) * lfo;
                let omega = (PI * freq / self.sample_rate).tan().min(10.0);
                let coeff = (1.0 - omega) / (1.0 + omega);

                let mut stage_input = *sample + state.feedback_state * self.feedback;
                for stage in &mut state.stages {
                    stage_input = stage.process(stage_input, coeff);
                }
                state.feedback_state = stage_input;
                let wet = stage_input;
                *sample = wet * self.mix + *sample * (1.0 - self.mix);
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for PhaserPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_PHASER_RATE
                | PARAM_PHASER_DEPTH
                | PARAM_PHASER_CENTER
                | PARAM_PHASER_FEEDBACK
                | PARAM_PHASER_MIX
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn phaser_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_PHASER_RATE,
            "Rate",
            ParameterKind::continuous(0.05..=5.0, 0.3),
        )
        .with_unit("Hz")
        .with_description("Modulation speed"),
        ParameterDefinition::new(
            PARAM_PHASER_DEPTH,
            "Depth",
            ParameterKind::continuous(0.0..=1.0, 0.7),
        )
        .with_description("Range of the sweep"),
        ParameterDefinition::new(
            PARAM_PHASER_CENTER,
            "Center",
            ParameterKind::continuous(50.0..=4_000.0, 800.0),
        )
        .with_unit("Hz")
        .with_description("Center frequency of the phaser"),
        ParameterDefinition::new(
            PARAM_PHASER_FEEDBACK,
            "Feedback",
            ParameterKind::continuous(0.0..=0.95, 0.4),
        )
        .with_description("Feedback amount"),
        ParameterDefinition::new(
            PARAM_PHASER_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.5),
        )
        .with_description("Wet/dry balance"),
    ])
}

pub struct PhaserFactory;

impl PluginFactory for PhaserFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.phaser", "Phaser", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(phaser_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(PhaserPlugin::default())
    }
}

const PARAM_DIST_DRIVE: &str = "drive";
const PARAM_DIST_TONE: &str = "tone";
const PARAM_DIST_MIX: &str = "mix";

#[derive(Debug, Clone)]
pub struct DistortionPlugin {
    sample_rate: f32,
    drive: f32,
    tone_hz: f32,
    mix: f32,
    lowpass_coeff: f32,
    lowpass_state: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for DistortionPlugin {
    fn default() -> Self {
        let layout = distortion_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            drive: 4.0,
            tone_hz: 6_000.0,
            mix: 0.7,
            lowpass_coeff: 0.0,
            lowpass_state: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl DistortionPlugin {
    fn refresh_from_parameters(&mut self) {
        self.drive = self
            .parameters
            .get(&ParameterId::from(PARAM_DIST_DRIVE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(4.0)
            .max(1.0);
        self.tone_hz = self
            .parameters
            .get(&ParameterId::from(PARAM_DIST_TONE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(6_000.0)
            .clamp(100.0, 20_000.0);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_DIST_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.7)
            .clamp(0.0, 1.0);
        let rc = (2.0 * PI * self.tone_hz / self.sample_rate.max(1.0)).min(10.0);
        let alpha = (-rc).exp();
        self.lowpass_coeff = alpha.clamp(0.0, 0.999);
    }
}

impl AudioProcessor for DistortionPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.distortion", "Distortion", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.lowpass_state = vec![0.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, state) in buffer.channels_mut().zip(self.lowpass_state.iter_mut()) {
            for sample in channel.iter_mut() {
                let dry = *sample;
                let driven = (dry * self.drive).tanh();
                let filtered = (1.0 - self.lowpass_coeff) * driven + self.lowpass_coeff * *state;
                *state = filtered;
                *sample = dry * (1.0 - self.mix) + filtered * self.mix;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for DistortionPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_DIST_DRIVE | PARAM_DIST_TONE | PARAM_DIST_MIX
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn distortion_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_DIST_DRIVE,
            "Drive",
            ParameterKind::continuous(1.0..=20.0, 4.0),
        )
        .with_description("Input gain before saturation"),
        ParameterDefinition::new(
            PARAM_DIST_TONE,
            "Tone",
            ParameterKind::continuous(100.0..=20_000.0, 6_000.0),
        )
        .with_unit("Hz")
        .with_description("Post-saturation low-pass filter"),
        ParameterDefinition::new(
            PARAM_DIST_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 0.7),
        )
        .with_description("Wet/dry balance"),
    ])
}

pub struct DistortionFactory;

impl PluginFactory for DistortionFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.distortion", "Distortion", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(distortion_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(DistortionPlugin::default())
    }
}

const PARAM_FILTER_CUTOFF: &str = "cutoff";
const PARAM_FILTER_RESONANCE: &str = "resonance";
const PARAM_FILTER_DEPTH: &str = "depth";
const PARAM_FILTER_RATE: &str = "rate";
const PARAM_FILTER_MODE: &str = "mode";
const PARAM_FILTER_MIX: &str = "mix";

#[derive(Debug, Clone, Copy)]
enum FilterMode {
    LowPass,
    BandPass,
    HighPass,
}

impl FilterMode {
    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::BandPass,
            2 => Self::HighPass,
            _ => Self::LowPass,
        }
    }
}

#[derive(Debug, Clone)]
struct FilterState {
    low: f32,
    band: f32,
}

impl FilterState {
    fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }

    fn process(&mut self, input: f32, g: f32, resonance: f32) -> (f32, f32, f32) {
        let v0 = input;
        let v1 = self.band + g * v0;
        let v2 = self.low + g * v1;
        self.band = v1 - resonance * v2;
        self.low = v2;
        let high = v0 - resonance * self.band - self.low;
        (self.low, self.band, high)
    }
}

#[derive(Debug, Clone)]
pub struct AutoFilterPlugin {
    sample_rate: f32,
    cutoff: f32,
    resonance: f32,
    depth_oct: f32,
    rate: f32,
    mix: f32,
    mode: FilterMode,
    states: Vec<FilterState>,
    phases: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for AutoFilterPlugin {
    fn default() -> Self {
        let layout = autofilter_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            cutoff: 1_000.0,
            resonance: 0.5,
            depth_oct: 1.0,
            rate: 0.5,
            mix: 1.0,
            mode: FilterMode::LowPass,
            states: Vec::new(),
            phases: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl AutoFilterPlugin {
    fn refresh_from_parameters(&mut self) {
        self.cutoff = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_CUTOFF))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1_000.0)
            .clamp(20.0, 20_000.0);
        self.resonance = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_RESONANCE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5)
            .clamp(0.1, 4.0);
        self.depth_oct = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_DEPTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0)
            .clamp(0.0, 4.0);
        self.rate = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_RATE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5)
            .clamp(0.05, 10.0);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let mode_index = self
            .parameters
            .get(&ParameterId::from(PARAM_FILTER_MODE))
            .and_then(ParameterValue::as_choice)
            .unwrap_or(0);
        self.mode = FilterMode::from_index(mode_index);
    }
}

impl AudioProcessor for AutoFilterPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.autofilter",
            "Auto Filter",
            "Harmoniq Labs",
        )
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.states = (0..config.layout.channels() as usize)
            .map(|_| FilterState::new())
            .collect();
        self.phases = vec![0.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let phase_inc = self.rate / self.sample_rate;
        for ((channel, state), phase) in buffer
            .channels_mut()
            .zip(self.states.iter_mut())
            .zip(self.phases.iter_mut())
        {
            for sample in channel.iter_mut() {
                *phase = (*phase + phase_inc) % 1.0;
                let lfo = (*phase * TWO_PI).sin();
                let octave_offset = self.depth_oct * lfo;
                let cutoff =
                    (self.cutoff * 2f32.powf(octave_offset)).clamp(20.0, self.sample_rate * 0.45);
                let g = (PI * cutoff / self.sample_rate).tan();
                let (low, band, high) = state.process(*sample, g, self.resonance);
                let wet = match self.mode {
                    FilterMode::LowPass => low,
                    FilterMode::BandPass => band,
                    FilterMode::HighPass => high,
                };
                *sample = wet * self.mix + *sample * (1.0 - self.mix);
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for AutoFilterPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_FILTER_CUTOFF
                | PARAM_FILTER_RESONANCE
                | PARAM_FILTER_DEPTH
                | PARAM_FILTER_RATE
                | PARAM_FILTER_MODE
                | PARAM_FILTER_MIX
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn autofilter_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_FILTER_CUTOFF,
            "Cutoff",
            ParameterKind::continuous(20.0..=20_000.0, 1_000.0),
        )
        .with_unit("Hz")
        .with_description("Base cutoff frequency"),
        ParameterDefinition::new(
            PARAM_FILTER_RESONANCE,
            "Resonance",
            ParameterKind::continuous(0.1..=4.0, 0.5),
        )
        .with_description("Filter resonance"),
        ParameterDefinition::new(
            PARAM_FILTER_DEPTH,
            "Depth",
            ParameterKind::continuous(0.0..=4.0, 1.0),
        )
        .with_description("Modulation depth in octaves"),
        ParameterDefinition::new(
            PARAM_FILTER_RATE,
            "Rate",
            ParameterKind::continuous(0.05..=10.0, 0.5),
        )
        .with_unit("Hz")
        .with_description("LFO rate"),
        ParameterDefinition::new(
            PARAM_FILTER_MODE,
            "Mode",
            ParameterKind::Choice {
                options: vec!["Low-pass".into(), "Band-pass".into(), "High-pass".into()],
                default: 0,
            },
        )
        .with_description("Filter response"),
        ParameterDefinition::new(
            PARAM_FILTER_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 1.0),
        )
        .with_description("Wet/dry balance"),
    ])
}

pub struct AutoFilterFactory;

impl PluginFactory for AutoFilterFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.autofilter",
            "Auto Filter",
            "Harmoniq Labs",
        )
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(autofilter_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(AutoFilterPlugin::default())
    }
}

const PARAM_STEREO_WIDTH: &str = "width";
const PARAM_STEREO_DELAY: &str = "delay";
const PARAM_STEREO_MIX: &str = "mix";

#[derive(Debug, Clone)]
pub struct StereoEnhancerPlugin {
    width: f32,
    delay_samples: usize,
    mix: f32,
    delay_buffer: Vec<f32>,
    delay_index: usize,
    parameters: ParameterSet,
    sample_rate: f32,
}

impl Default for StereoEnhancerPlugin {
    fn default() -> Self {
        let layout = stereo_enhancer_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            width: 1.2,
            delay_samples: 0,
            mix: 1.0,
            delay_buffer: Vec::new(),
            delay_index: 0,
            parameters,
            sample_rate: 48_000.0,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl StereoEnhancerPlugin {
    fn refresh_from_parameters(&mut self) {
        self.width = self
            .parameters
            .get(&ParameterId::from(PARAM_STEREO_WIDTH))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.2)
            .clamp(0.0, 2.5);
        let delay_ms = self
            .parameters
            .get(&ParameterId::from(PARAM_STEREO_DELAY))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(5.0)
            .clamp(0.0, 30.0);
        self.mix = self
            .parameters
            .get(&ParameterId::from(PARAM_STEREO_MIX))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        self.delay_samples = ((delay_ms / 1_000.0) * self.sample_rate).round() as usize;
        if self.delay_samples > 0 {
            self.delay_buffer = vec![0.0; self.delay_samples.max(1)];
            self.delay_index = 0;
        } else {
            self.delay_buffer.clear();
            self.delay_index = 0;
        }
    }
}

impl AudioProcessor for StereoEnhancerPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.stereo_enhancer",
            "Stereo Enhancer",
            "Harmoniq Labs",
        )
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let mut channels = buffer.channels_mut();
        let Some(left) = channels.next() else {
            return Ok(());
        };
        let Some(right) = channels.next() else {
            return Ok(());
        };
        for (left_sample, right_sample) in left.iter_mut().zip(right.iter_mut()) {
            let dry_left = *left_sample;
            let dry_right = *right_sample;
            let delayed_right = if self.delay_samples > 0 {
                let val = self.delay_buffer[self.delay_index];
                self.delay_buffer[self.delay_index] = dry_right;
                self.delay_index = (self.delay_index + 1) % self.delay_buffer.len();
                val
            } else {
                dry_right
            };
            let mid = (dry_left + delayed_right) * 0.5;
            let mut side = (dry_left - delayed_right) * 0.5 * self.width;
            side = side.clamp(-2.0, 2.0);
            let wet_left = mid + side;
            let wet_right = mid - side;
            *left_sample = dry_left * (1.0 - self.mix) + wet_left * self.mix;
            *right_sample = dry_right * (1.0 - self.mix) + wet_right * self.mix;
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Stereo)
    }
}

impl NativePlugin for StereoEnhancerPlugin {
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
        if matches!(
            id.as_str(),
            PARAM_STEREO_WIDTH | PARAM_STEREO_DELAY | PARAM_STEREO_MIX
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn stereo_enhancer_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_STEREO_WIDTH,
            "Width",
            ParameterKind::continuous(0.0..=2.5, 1.2),
        )
        .with_description("Stereo width multiplier"),
        ParameterDefinition::new(
            PARAM_STEREO_DELAY,
            "Haas Delay",
            ParameterKind::continuous(0.0..=30.0, 5.0),
        )
        .with_unit("ms")
        .with_description("Delay applied to the right channel"),
        ParameterDefinition::new(
            PARAM_STEREO_MIX,
            "Mix",
            ParameterKind::continuous(0.0..=1.0, 1.0),
        )
        .with_description("Wet/dry balance"),
    ])
}

pub struct StereoEnhancerFactory;

impl PluginFactory for StereoEnhancerFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new(
            "harmoniq.effects.stereo_enhancer",
            "Stereo Enhancer",
            "Harmoniq Labs",
        )
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(stereo_enhancer_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(StereoEnhancerPlugin::default())
    }
}

const PARAM_GATE_THRESHOLD: &str = "threshold";
const PARAM_GATE_RATIO: &str = "ratio";
const PARAM_GATE_ATTACK: &str = "attack";
const PARAM_GATE_RELEASE: &str = "release";

#[derive(Debug, Clone)]
pub struct NoiseGatePlugin {
    sample_rate: f32,
    threshold: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: Vec<f32>,
    gain: Vec<f32>,
    parameters: ParameterSet,
}

impl Default for NoiseGatePlugin {
    fn default() -> Self {
        let layout = noise_gate_layout();
        let parameters = ParameterSet::new(layout);
        let mut plugin = Self {
            sample_rate: 48_000.0,
            threshold: db_to_gain(-40.0),
            ratio: 4.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            envelope: Vec::new(),
            gain: Vec::new(),
            parameters,
        };
        plugin.refresh_from_parameters();
        plugin
    }
}

impl NoiseGatePlugin {
    fn refresh_from_parameters(&mut self) {
        let threshold_db = self
            .parameters
            .get(&ParameterId::from(PARAM_GATE_THRESHOLD))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(-40.0);
        self.threshold = db_to_gain(threshold_db);
        self.ratio = self
            .parameters
            .get(&ParameterId::from(PARAM_GATE_RATIO))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(4.0)
            .max(1.0);
        let attack = self
            .parameters
            .get(&ParameterId::from(PARAM_GATE_ATTACK))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(5.0);
        let release = self
            .parameters
            .get(&ParameterId::from(PARAM_GATE_RELEASE))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(80.0);
        self.attack_coeff = time_to_coeff(attack, self.sample_rate);
        self.release_coeff = time_to_coeff(release, self.sample_rate);
    }
}

impl AudioProcessor for NoiseGatePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.noise_gate", "Noise Gate", "Harmoniq Labs")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.envelope = vec![0.0; config.layout.channels() as usize];
        self.gain = vec![1.0; config.layout.channels() as usize];
        self.refresh_from_parameters();
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for (channel, (env, gain)) in buffer
            .channels_mut()
            .zip(self.envelope.iter_mut().zip(self.gain.iter_mut()))
        {
            for sample in channel.iter_mut() {
                let level = sample.abs().max(1e-6);
                let coeff = if level > *env {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                *env = coeff * *env + (1.0 - coeff) * level;

                let target_gain = if *env < self.threshold {
                    let ratio = (*env / self.threshold).powf(self.ratio - 1.0);
                    ratio.clamp(0.0, 1.0)
                } else {
                    1.0
                };
                *gain = 0.9 * *gain + 0.1 * target_gain;
                *sample *= *gain;
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for NoiseGatePlugin {
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
        if matches!(
            id.as_str(),
            PARAM_GATE_THRESHOLD | PARAM_GATE_RATIO | PARAM_GATE_ATTACK | PARAM_GATE_RELEASE
        ) {
            self.refresh_from_parameters();
        }
        Ok(())
    }
}

fn noise_gate_layout() -> ParameterLayout {
    ParameterLayout::new(vec![
        ParameterDefinition::new(
            PARAM_GATE_THRESHOLD,
            "Threshold",
            ParameterKind::continuous(-80.0..=0.0, -40.0),
        )
        .with_unit("dB")
        .with_description("Level below which the gate engages"),
        ParameterDefinition::new(
            PARAM_GATE_RATIO,
            "Ratio",
            ParameterKind::continuous(1.0..=12.0, 4.0),
        )
        .with_description("Expansion ratio"),
        ParameterDefinition::new(
            PARAM_GATE_ATTACK,
            "Attack",
            ParameterKind::continuous(1.0..=50.0, 5.0),
        )
        .with_unit("ms")
        .with_description("Time for the gate to open"),
        ParameterDefinition::new(
            PARAM_GATE_RELEASE,
            "Release",
            ParameterKind::continuous(10.0..=500.0, 80.0),
        )
        .with_unit("ms")
        .with_description("Time for the gate to close"),
    ])
}

pub struct NoiseGateFactory;

impl PluginFactory for NoiseGateFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.effects.noise_gate", "Noise Gate", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(noise_gate_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(NoiseGatePlugin::default())
    }
}
