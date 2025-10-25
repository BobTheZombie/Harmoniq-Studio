use std::f32::consts::PI;

use clap_plugin_authoring::{clap_export, AudioProcessor, Plugin, PluginDescriptor, PluginFactory};
use clap_sys::{clap_host, clap_process, clap_process_status, CLAP_PROCESS_CONTINUE};

struct TestSynth {
    phase: f32,
    increment: f32,
}

impl AudioProcessor for TestSynth {
    fn process(&mut self, process: &mut clap_process) -> clap_process_status {
        unsafe {
            let outputs = std::slice::from_raw_parts_mut(
                process.audio_outputs,
                process.audio_outputs_count as usize,
            );
            for buffer in outputs {
                if buffer.data32.is_null() {
                    continue;
                }
                for channel in 0..buffer.channel_count as isize {
                    let channel_ptr = *buffer.data32.offset(channel);
                    for frame in 0..process.frames_count as isize {
                        let sample = (self.phase + frame as f32 * self.increment).sin();
                        *channel_ptr.offset(frame) = sample;
                    }
                }
            }
            self.phase += process.frames_count as f32 * self.increment;
            while self.phase > 2.0 * PI {
                self.phase -= 2.0 * PI;
            }
        }
        CLAP_PROCESS_CONTINUE.0 as clap_process_status
    }
}

impl Plugin for TestSynth {
    fn descriptor(&self) -> &'static PluginDescriptor {
        static DESCRIPTOR: PluginDescriptor = PluginDescriptor {
            id: "studio.harmoniq.testsynth",
            name: "Test Synth",
            vendor: "Harmoniq",
            url: "https://harmoniq.dev",
            version: "0.1.0",
            description: "A reference CLAP synthesizer that outputs a sine wave",
            features: &[],
        };
        &DESCRIPTOR
    }
}

struct TestSynthFactory;

impl PluginFactory for TestSynthFactory {
    type Plugin = TestSynth;

    fn descriptors() -> &'static [PluginDescriptor] {
        static DESCRIPTORS: [PluginDescriptor; 1] = [PluginDescriptor {
            id: "studio.harmoniq.testsynth",
            name: "Test Synth",
            vendor: "Harmoniq",
            url: "https://harmoniq.dev",
            version: "0.1.0",
            description: "A reference CLAP synthesizer that outputs a sine wave",
            features: &[],
        }];
        &DESCRIPTORS
    }

    fn new_plugin(_descriptor_id: &str, _host: *const clap_host) -> anyhow::Result<Self::Plugin> {
        Ok(TestSynth {
            phase: 0.0,
            increment: 2.0 * PI * 440.0 / 48_000.0,
        })
    }
}

clap_export!(TestSynthFactory);
