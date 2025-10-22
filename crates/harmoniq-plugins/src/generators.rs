use std::f32::consts::TAU;
use std::sync::Arc;

use harmoniq_engine::{
    AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, MidiEvent, MidiProcessor,
    PluginDescriptor,
};
use harmoniq_plugin_sdk::{
    NativePlugin, ParameterDefinition, ParameterId, ParameterKind, ParameterLayout, ParameterSet,
    ParameterValue, PluginFactory, PluginParameterError,
};
use rand::Rng;

const LEVEL_PARAM: &str = "level";
const NOISE_LEVEL_PARAM: &str = "amplitude";

/// Lightweight sine oscillator suitable for pad sounds and metering.
#[derive(Debug, Clone)]
pub struct SineSynth {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    active: bool,
    level: f32,
    parameters: ParameterSet,
}

impl Default for SineSynth {
    fn default() -> Self {
        let parameters = ParameterSet::new(sine_layout());
        let level = parameters
            .get(&ParameterId::from(LEVEL_PARAM))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.8);
        Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 440.0,
            velocity: 0.0,
            active: false,
            level,
            parameters,
        }
    }
}

impl SineSynth {
    pub fn with_frequency(frequency: f32) -> Self {
        Self {
            frequency,
            ..Default::default()
        }
    }

    fn render_sample(&mut self) -> f32 {
        let increment = TAU * self.frequency / self.sample_rate;
        self.phase = (self.phase + increment).rem_euclid(TAU);
        (self.phase).sin() * self.velocity * self.level
    }
}

impl AudioProcessor for SineSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sine", "Sine Synth", "Harmoniq Labs")
            .with_description("Basic anti-aliased sine oscillator for testing")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for channel in buffer.channels_mut() {
            for sample in channel.iter_mut() {
                *sample = self.render_sample();
            }
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl MidiProcessor for SineSynth {
    fn process_midi(&mut self, events: &[MidiEvent]) -> anyhow::Result<()> {
        for event in events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.frequency = 440.0 * 2.0_f32.powf((*note as f32 - 69.0) / 12.0);
                    self.velocity = *velocity as f32 / 127.0;
                    self.active = true;
                }
                MidiEvent::NoteOff { .. } => {
                    self.velocity = 0.0;
                    self.active = false;
                }
                MidiEvent::ControlChange { control, value, .. } if *control == 74 => {
                    // Filter cutoff mapping placeholder.
                    self.frequency = 55.0 + (*value as f32 / 127.0) * 2000.0;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl NativePlugin for SineSynth {
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
        if id.as_str() == LEVEL_PARAM {
            if let Some(level) = value.as_continuous() {
                self.level = level;
            }
        }
        Ok(())
    }
}

fn sine_layout() -> ParameterLayout {
    ParameterLayout::new(vec![ParameterDefinition::new(
        LEVEL_PARAM,
        "Level",
        ParameterKind::continuous(0.0..=1.0, 0.8),
    )
    .with_description("Output level applied to generated tone")])
}

pub struct SineSynthFactory;

impl PluginFactory for SineSynthFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sine", "Sine Synth", "Harmoniq Labs")
            .with_description("Basic anti-aliased sine oscillator for testing")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(sine_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(SineSynth::default())
    }
}

/// White noise generator for creative FX and testing.
#[derive(Debug, Clone)]
pub struct NoisePlugin {
    amplitude: f32,
    parameters: ParameterSet,
}

impl Default for NoisePlugin {
    fn default() -> Self {
        let parameters = ParameterSet::new(noise_layout());
        let amplitude = parameters
            .get(&ParameterId::from(NOISE_LEVEL_PARAM))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(0.5);
        Self {
            amplitude,
            parameters,
        }
    }
}

impl AudioProcessor for NoisePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.noise", "Noise", "Harmoniq Labs")
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        let mut rng = rand::thread_rng();
        for sample in buffer.iter_mut() {
            let noise: f32 = rng.gen_range(-0.5..0.5);
            *sample = noise * self.amplitude;
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for NoisePlugin {
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
        if id.as_str() == NOISE_LEVEL_PARAM {
            if let Some(amplitude) = value.as_continuous() {
                self.amplitude = amplitude;
            }
        }
        Ok(())
    }
}

fn noise_layout() -> ParameterLayout {
    ParameterLayout::new(vec![ParameterDefinition::new(
        NOISE_LEVEL_PARAM,
        "Amplitude",
        ParameterKind::continuous(0.0..=1.0, 0.5),
    )
    .with_description("Overall amplitude of the noise source")])
}

pub struct NoisePluginFactory;

impl PluginFactory for NoisePluginFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.noise", "Noise", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(noise_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(NoisePlugin::default())
    }
}
