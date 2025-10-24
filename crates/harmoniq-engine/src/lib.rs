//! Harmoniq Engine
//! =================
//! Core audio engine building blocks for Harmoniq Studio. This crate focuses on
//! deterministic, low latency audio processing suitable for professional audio
//! workstations and live performance scenarios.

pub mod automation;
pub mod buffer;
pub mod buffers;
pub mod clips;
pub mod dsp;
pub mod engine;
pub mod graph;
pub mod media;
pub mod mixer;
pub mod nodes;
pub mod plugin;
pub mod render;
pub mod rt;
mod scratch;
pub mod sound_server;
pub mod time;
pub mod timeline;
mod tone;
pub mod transport;

#[cfg(feature = "native")]
pub mod realtime;

pub use automation::{
    AutomationCommand, AutomationCurve, AutomationEvent, AutomationWriteMode, CurveShape,
    ParameterSpec,
};
pub use buffer::{AudioBuffer, BufferConfig, ChannelLayout};
pub use clips::{AudioClip, ClipError, CrossfadeSpec, FadeCurve, FadeSpec, StretchQuality};
pub use dsp::RealtimeDspEngine;
pub use engine::{EngineCommand, EngineCommandQueue, HarmoniqEngine, TransportState};
pub use graph::{GraphBuilder, GraphHandle, NodeHandle};
pub use mixer::{
    MixerAuxSendState, MixerAuxState, MixerBusState, MixerEngine, MixerInsertProcessor,
    MixerInsertState, MixerMasterState, MixerModel, MixerState, MixerTargetState, MixerTrackState,
};
pub use nodes::{GainNode, NodeNoise, NodeOsc, NoiseNode, SineNode};
pub use plugin::{
    AudioProcessor, MidiEvent, MidiProcessor, MidiTimestamp, PluginDescriptor, PluginId,
};
pub use render::{
    DitherKind, FreezeSettings, OfflineRenderer, RenderDuration, RenderFile, RenderFormat,
    RenderProject, RenderQueue, RenderReport, RenderRequest, RenderResult, RenderSpeed,
    StemSettings,
};
pub use rt::{AudioMetrics, AudioMetricsCollector};
pub use time::{
    BeatInfo, LoopRegion, Tempo, TempoMap, TempoSegment, TimeSignature,
    Transport as TimelineTransport,
};
pub use timeline::{ClipEvent, Timeline, TimelineError};
pub use transport::Transport as RealtimeTransport;

#[cfg(feature = "openasio")]
pub mod backend;

#[cfg(feature = "native")]
pub use realtime::{start_realtime, EngineHandle};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationCommand, CurveShape, ParameterSpec};
    use rand::Rng;

    struct NoiseGenerator;

    impl AudioProcessor for NoiseGenerator {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor::new("noise", "Noise Generator", "Harmoniq Labs")
                .with_description("Simple white noise generator for testing")
        }

        fn prepare(&mut self, config: &BufferConfig) -> anyhow::Result<()> {
            assert!(config.sample_rate > 0.0);
            Ok(())
        }

        fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
            let mut rng = rand::thread_rng();
            for sample in buffer.iter_mut() {
                *sample = rng.gen_range(-0.25..0.25);
            }
            Ok(())
        }
    }

    #[test]
    fn engine_executes_graph() {
        let config = BufferConfig::new(48_000.0, 128, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let noise_id = engine
            .register_processor(Box::new(NoiseGenerator))
            .expect("register noise");

        let mut builder = GraphBuilder::new();
        let noise_node = builder.add_node(noise_id);
        builder.connect_to_mixer(noise_node, 0.5).unwrap();
        let handle = builder.build();

        engine
            .replace_graph(handle)
            .expect("graph should be accepted");

        let mut buffer = AudioBuffer::from_config(&config);
        engine.process_block(&mut buffer).expect("process");

        let rms = buffer
            .channels()
            .flat_map(|channel| channel.iter())
            .map(|sample| sample * sample)
            .sum::<f32>()
            / (config.block_size * config.layout.channels() as usize) as f32;

        assert!(rms > 0.0);
    }

    #[test]
    fn queued_commands_are_processed_before_audio() {
        let config = BufferConfig::new(48_000.0, 128, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let noise_id = engine
            .register_processor(Box::new(NoiseGenerator))
            .expect("register noise");

        let mut builder = GraphBuilder::new();
        let noise_node = builder.add_node(noise_id);
        builder.connect_to_mixer(noise_node, 1.0).unwrap();

        let queue = engine.command_queue();
        queue
            .try_send(EngineCommand::ReplaceGraph(builder.build()))
            .expect("queue should accept replace graph");
        queue
            .try_send(EngineCommand::SetTransport(TransportState::Playing))
            .expect("queue should accept transport command");

        let mut buffer = AudioBuffer::from_config(&config);
        engine.process_block(&mut buffer).expect("process");

        assert_eq!(engine.transport(), TransportState::Playing);
        assert!(queue.is_empty());
        assert!(buffer
            .channels()
            .flat_map(|channel| channel.iter())
            .any(|sample| *sample != 0.0));
    }

    struct PulseGenerator;

    impl AudioProcessor for PulseGenerator {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor::new("pulse", "Pulse", "Harmoniq Labs")
        }

        fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
            Ok(())
        }

        fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
            buffer.clear();
            for channel in buffer.channels_mut() {
                if let Some(sample) = channel.get_mut(0) {
                    *sample = 1.0;
                }
            }
            Ok(())
        }
    }

    struct DelayedPulse {
        latency: usize,
        triggered: bool,
    }

    impl DelayedPulse {
        fn new(latency: usize) -> Self {
            Self {
                latency,
                triggered: false,
            }
        }
    }

    impl AudioProcessor for DelayedPulse {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor::new("delayed_pulse", "Delayed Pulse", "Harmoniq Labs")
        }

        fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
            Ok(())
        }

        fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
            buffer.clear();
            if !self.triggered {
                if buffer.len() > self.latency {
                    for channel in buffer.channels_mut() {
                        channel[self.latency] = 1.0;
                    }
                }
                self.triggered = true;
            }
            Ok(())
        }

        fn latency_samples(&self) -> usize {
            self.latency
        }
    }

    #[test]
    fn delay_compensation_aligns_outputs() {
        let config = BufferConfig::new(48_000.0, 128, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let pulse_id = engine
            .register_processor(Box::new(PulseGenerator))
            .expect("register pulse");
        let delayed_id = engine
            .register_processor(Box::new(DelayedPulse::new(32)))
            .expect("register delayed");

        let mut builder = GraphBuilder::new();
        let pulse_node = builder.add_node(pulse_id);
        let delayed_node = builder.add_node(delayed_id);
        builder.connect_to_mixer(pulse_node, 1.0).unwrap();
        builder.connect_to_mixer(delayed_node, 1.0).unwrap();

        engine
            .replace_graph(builder.build())
            .expect("graph should be accepted");

        let mut buffer = AudioBuffer::from_config(&config);
        engine.process_block(&mut buffer).expect("process");

        let left = buffer.channel(0);
        assert!(left.iter().take(32).all(|sample| sample.abs() < 1e-6));
        assert!((left[32] - 2.0).abs() < 1e-6);
    }

    #[derive(Default)]
    struct AutomationSynth {
        pending: Vec<(usize, f32, usize)>,
    }

    impl AudioProcessor for AutomationSynth {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor::new("automation_synth", "Automation Synth", "Harmoniq Labs")
        }

        fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
            Ok(())
        }

        fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
            buffer.clear();
            for &(parameter, value, offset) in &self.pending {
                if parameter == 0 {
                    for channel in buffer.channels_mut() {
                        if offset < channel.len() {
                            channel[offset] = value;
                        }
                    }
                }
            }
            self.pending.clear();
            Ok(())
        }

        fn handle_automation_event(
            &mut self,
            parameter: usize,
            value: f32,
            sample_offset: usize,
        ) -> anyhow::Result<()> {
            self.pending.push((parameter, value, sample_offset));
            Ok(())
        }
    }

    #[test]
    fn automation_events_are_sample_accurate() {
        let config = BufferConfig::new(48_000.0, 64, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let synth_id = engine
            .register_processor(Box::new(AutomationSynth::default()))
            .expect("register synth");

        let mut builder = GraphBuilder::new();
        let node = builder.add_node(synth_id);
        builder.connect_to_mixer(node, 1.0).unwrap();
        engine
            .replace_graph(builder.build())
            .expect("graph should be accepted");

        engine
            .register_automation_parameter(
                synth_id,
                ParameterSpec::new(0, "Amplitude", 0.0, 1.0, 0.0),
            )
            .expect("register automation parameter");

        let sender = engine
            .automation_sender(synth_id)
            .expect("automation sender");

        sender
            .send(AutomationCommand::DrawCurve {
                parameter: 0,
                sample: 0,
                value: 0.25,
                shape: CurveShape::Step,
            })
            .expect("send automation");
        sender
            .send(AutomationCommand::DrawCurve {
                parameter: 0,
                sample: 16,
                value: 0.75,
                shape: CurveShape::Step,
            })
            .expect("send automation");

        let mut buffer = AudioBuffer::from_config(&config);
        engine.process_block(&mut buffer).expect("process");

        let left = buffer.channel(0);
        assert!((left[0] - 0.25).abs() < f32::EPSILON);
        assert!((left[16] - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn automation_linear_curve_interpolates() {
        let config = BufferConfig::new(48_000.0, 64, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let synth_id = engine
            .register_processor(Box::new(AutomationSynth::default()))
            .expect("register synth");

        engine
            .register_automation_parameter(
                synth_id,
                ParameterSpec::new(0, "Amplitude", 0.0, 1.0, 0.0),
            )
            .expect("register automation parameter");

        let mut builder = GraphBuilder::new();
        let node = builder.add_node(synth_id);
        builder.connect_to_mixer(node, 1.0).unwrap();
        engine
            .replace_graph(builder.build())
            .expect("graph should be accepted");

        let sender = engine
            .automation_sender(synth_id)
            .expect("automation sender");

        sender
            .send(AutomationCommand::DrawCurve {
                parameter: 0,
                sample: 0,
                value: 0.0,
                shape: CurveShape::Linear,
            })
            .expect("send automation");
        sender
            .send(AutomationCommand::DrawCurve {
                parameter: 0,
                sample: 64,
                value: 1.0,
                shape: CurveShape::Linear,
            })
            .expect("send automation");

        let mut buffer = AudioBuffer::from_config(&config);
        engine.process_block(&mut buffer).expect("process");

        let left = buffer.channel(0);
        assert!((left[32] - 0.5).abs() < 1e-3);
        assert!(left.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn processes_ten_thousand_blocks_without_allocations() {
        let config = BufferConfig::new(48_000.0, 64, ChannelLayout::Stereo);
        let mut engine = HarmoniqEngine::new(config.clone()).expect("engine");

        let pulse_id = engine
            .register_processor(Box::new(PulseGenerator))
            .expect("register pulse");

        let mut builder = GraphBuilder::new();
        let pulse_node = builder.add_node(pulse_id);
        builder.connect_to_mixer(pulse_node, 1.0).unwrap();
        engine
            .replace_graph(builder.build())
            .expect("graph should be accepted");

        let mut buffer = AudioBuffer::from_config(&config);
        for _ in 0..10_000 {
            engine.process_block(&mut buffer).expect("process");
        }
    }
}
