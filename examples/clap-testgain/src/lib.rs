use clap_plugin_authoring::{clap_export, AudioProcessor, Plugin, PluginDescriptor, PluginFactory};
use clap_sys::{clap_host, clap_process, clap_process_status, CLAP_PROCESS_CONTINUE};

struct TestGain {
    gain: f32,
}

impl AudioProcessor for TestGain {
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
                        let sample = *channel_ptr.offset(frame) * self.gain;
                        *channel_ptr.offset(frame) = sample;
                    }
                }
            }
        }
        CLAP_PROCESS_CONTINUE.0 as clap_process_status
    }
}

impl Plugin for TestGain {
    fn descriptor(&self) -> &'static PluginDescriptor {
        static DESCRIPTOR: PluginDescriptor = PluginDescriptor {
            id: "studio.harmoniq.testgain",
            name: "Test Gain",
            vendor: "Harmoniq",
            url: "https://harmoniq.dev",
            version: "0.1.0",
            description: "A reference CLAP plug-in that applies a constant gain",
            features: &[],
        };
        &DESCRIPTOR
    }
}

struct TestGainFactory;

impl PluginFactory for TestGainFactory {
    type Plugin = TestGain;

    fn descriptors() -> &'static [PluginDescriptor] {
        static DESCRIPTORS: [PluginDescriptor; 1] = [PluginDescriptor {
            id: "studio.harmoniq.testgain",
            name: "Test Gain",
            vendor: "Harmoniq",
            url: "https://harmoniq.dev",
            version: "0.1.0",
            description: "A reference CLAP plug-in that applies a constant gain",
            features: &[],
        }];
        &DESCRIPTORS
    }

    fn new_plugin(_descriptor_id: &str, _host: *const clap_host) -> anyhow::Result<Self::Plugin> {
        Ok(TestGain { gain: 0.5 })
    }
}

clap_export!(TestGainFactory);
