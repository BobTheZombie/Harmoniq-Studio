//! Harmoniq Engine
//! =================
//! Core audio engine building blocks for Harmoniq Studio. This crate focuses on
//! deterministic, low latency audio processing suitable for professional audio
//! workstations and live performance scenarios.

pub mod buffer;
pub mod engine;
pub mod graph;
pub mod plugin;
pub mod sound_server;
pub mod time;
mod tone;

pub use buffer::{AudioBuffer, BufferConfig, ChannelLayout};
pub use engine::{EngineCommand, EngineCommandQueue, HarmoniqEngine, TransportState};
pub use graph::{GraphBuilder, GraphHandle, NodeHandle};
pub use plugin::{AudioProcessor, MidiEvent, MidiProcessor, PluginDescriptor, PluginId};

#[cfg(test)]
mod tests {
    use super::*;
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

        let mut buffer = AudioBuffer::from_config(config.clone());
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

        let mut buffer = AudioBuffer::from_config(config.clone());
        engine.process_block(&mut buffer).expect("process");

        assert_eq!(engine.transport(), TransportState::Playing);
        assert!(queue.is_empty());
        assert!(buffer
            .channels()
            .flat_map(|channel| channel.iter())
            .any(|sample| *sample != 0.0));
    }
}
