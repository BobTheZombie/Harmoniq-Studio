#[cfg(feature = "demo-render")]
use std::error::Error;
#[cfg(feature = "demo-render")]
mod demo {
    use std::collections::VecDeque;
    use std::error::Error;
    use std::fs::File;
    use std::io::BufWriter;
    use std::num::NonZeroU32;
    use std::path::Path;

    use hound::{SampleFormat, WavSpec, WavWriter};
    use nih_plug::audio_setup::{AuxiliaryBuffers, BufferConfig, ProcessMode};
    use nih_plug::context::{InitContext, PluginApi, ProcessContext, Transport};
    use nih_plug::prelude::{AudioIOLayout, Buffer, NoteEvent, Plugin};

    use westcoast_whine_synth::WestCoastWhineSynth;

    const BLOCK_SIZE: usize = 256;
    const SAMPLE_RATE: f32 = 48_000.0;
    const OUTPUT_PATH: &str = "/tmp/westcoast_whine_demo.wav";

    struct OfflineInitContext;

    impl InitContext<WestCoastWhineSynth> for OfflineInitContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Standalone
        }

        fn execute(&self, _task: <WestCoastWhineSynth as Plugin>::BackgroundTask) {}

        fn set_latency_samples(&self, _samples: u32) {}

        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    struct OfflineProcessContext {
        transport: Transport,
        events: VecDeque<NoteEvent<()>>,
    }

    impl OfflineProcessContext {
        fn new(sample_rate: f32) -> Self {
            let mut transport = Transport::new(sample_rate);
            transport.playing = true;
            Self {
                transport,
                events: VecDeque::new(),
            }
        }

        fn set_events(&mut self, events: Vec<NoteEvent<()>>) {
            self.events = events.into();
        }
    }

    impl ProcessContext<WestCoastWhineSynth> for OfflineProcessContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Standalone
        }

        fn execute_background(&self, _task: <WestCoastWhineSynth as Plugin>::BackgroundTask) {}

        fn execute_gui(&self, _task: <WestCoastWhineSynth as Plugin>::BackgroundTask) {}

        fn transport(&self) -> &Transport {
            &self.transport
        }

        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            self.events.pop_front()
        }

        fn send_event(&mut self, _event: NoteEvent<()>) {}

        fn set_latency_samples(&self, _samples: u32) {}

        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let mut plugin = WestCoastWhineSynth::default();
        let audio_layout = AudioIOLayout {
            main_input_channels: None,
            main_output_channels: Some(NonZeroU32::new(2).unwrap()),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: Default::default(),
        };
        let buffer_config = BufferConfig {
            sample_rate: SAMPLE_RATE,
            min_buffer_size: Some(BLOCK_SIZE as u32),
            max_buffer_size: BLOCK_SIZE as u32,
            process_mode: ProcessMode::Offline,
        };
        plugin.initialize(&audio_layout, &buffer_config, &mut OfflineInitContext);

        let melody = [
            (84u8, 0.9f32),
            (86, 0.92),
            (88, 0.95),
            (91, 0.9),
            (93, 0.92),
            (96, 0.94),
            (93, 0.9),
            (91, 0.88),
        ];
        let segment_samples = (SAMPLE_RATE * 0.45) as usize;
        let note_length = (SAMPLE_RATE * 0.52) as usize;

        #[derive(Clone, Copy)]
        enum ScheduledKind {
            NoteOn { note: u8, velocity: f32 },
            NoteOff { note: u8 },
        }

        #[derive(Clone, Copy)]
        struct ScheduledEvent {
            sample: usize,
            kind: ScheduledKind,
        }

        let mut schedule = Vec::new();
        for (index, &(note, velocity)) in melody.iter().enumerate() {
            let start = index * segment_samples;
            let end = start + note_length;
            schedule.push(ScheduledEvent {
                sample: start,
                kind: ScheduledKind::NoteOn { note, velocity },
            });
            schedule.push(ScheduledEvent {
                sample: end,
                kind: ScheduledKind::NoteOff { note },
            });
        }

        let total_samples = schedule
            .iter()
            .map(|event| event.sample)
            .max()
            .unwrap_or(0)
            + (SAMPLE_RATE * 1.0) as usize;

        let mut left_output = vec![0.0f32; total_samples];
        let mut right_output = vec![0.0f32; total_samples];

        let mut process_context = OfflineProcessContext::new(SAMPLE_RATE);
        let mut aux_inputs: [Buffer<'static>; 0] = [];
        let mut aux_outputs: [Buffer<'static>; 0] = [];
        let mut aux_buffers = AuxiliaryBuffers {
            inputs: &mut aux_inputs,
            outputs: &mut aux_outputs,
        };

        let mut buffer = Buffer::default();
        let mut current_sample = 0usize;

        while current_sample < total_samples {
            let block_end = (current_sample + BLOCK_SIZE).min(total_samples);
            let block_len = block_end - current_sample;

            let mut block_left = vec![0.0f32; block_len];
            let mut block_right = vec![0.0f32; block_len];

            unsafe {
                buffer.set_slices(block_len, |slices| {
                    slices.clear();
                    slices.push(&mut block_left);
                    slices.push(&mut block_right);
                });
            }

            let mut block_events = Vec::new();
            for event in schedule.iter().filter(|event| {
                event.sample >= current_sample && event.sample < block_end
            }) {
                let timing = (event.sample - current_sample) as u32;
                let note_event = match event.kind {
                    ScheduledKind::NoteOn { note, velocity } => NoteEvent::NoteOn {
                        timing,
                        voice_id: None,
                        channel: 0,
                        note,
                        velocity,
                    },
                    ScheduledKind::NoteOff { note } => NoteEvent::NoteOff {
                        timing,
                        voice_id: None,
                        channel: 0,
                        note,
                    },
                };
                block_events.push(note_event);
            }
            block_events.sort_by_key(|event| event.timing());
            process_context.set_events(block_events);

            plugin.process(&mut buffer, &mut aux_buffers, &mut process_context);

            let channels = buffer.as_slice();
            for (i, sample) in channels[0].iter().enumerate() {
                left_output[current_sample + i] = *sample;
            }
            for (i, sample) in channels[1].iter().enumerate() {
                right_output[current_sample + i] = *sample;
            }

            current_sample = block_end;
        }

        write_wav(&left_output, &right_output)?;
        println!("Rendered demo to {}", OUTPUT_PATH);
        Ok(())
    }

    fn write_wav(left: &[f32], right: &[f32]) -> Result<(), Box<dyn Error>> {
        let spec = WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let path = Path::new(OUTPUT_PATH);
        let file = File::create(path)?;
        let mut writer = WavWriter::new(BufWriter::new(file), spec)?;

        for (&l, &r) in left.iter().zip(right.iter()) {
            let l_sample = (l * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            let r_sample = (r * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer.write_sample(l_sample)?;
            writer.write_sample(r_sample)?;
        }

        writer.finalize()?;
        Ok(())
    }
}

#[cfg(feature = "demo-render")]
fn main() -> Result<(), Box<dyn Error>> {
    demo::run()
}

#[cfg(not(feature = "demo-render"))]
fn main() {
    eprintln!("Enable the 'demo-render' feature to run this binary.");
}
