use std::f32::consts::TAU;

use harmoniq_engine::{
    AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, MidiEvent, MidiProcessor,
    PluginDescriptor,
};
use rand::Rng;

/// Lightweight sine oscillator suitable for pad sounds and metering.
#[derive(Debug, Clone)]
pub struct SineSynth {
    sample_rate: f32,
    phase: f32,
    frequency: f32,
    velocity: f32,
    active: bool,
}

impl Default for SineSynth {
    fn default() -> Self {
        Self {
            sample_rate: 44_100.0,
            phase: 0.0,
            frequency: 440.0,
            velocity: 0.0,
            active: false,
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
        (self.phase).sin() * self.velocity
    }
}

impl AudioProcessor for SineSynth {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.sine", "Sine Synth", "Harmoniq Labs")
            .with_description("Basic anti-aliased sine oscillator for testing")
    }

    fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
        self.sample_rate = config.sample_rate;
        self.velocity = if self.active { 0.8 } else { 0.0 };
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

/// White noise generator for creative FX and testing.
#[derive(Debug, Clone)]
pub struct NoisePlugin;

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
            *sample = rng.gen_range(-0.5..0.5);
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}
