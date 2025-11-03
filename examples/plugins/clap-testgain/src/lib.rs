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
            description: "A deterministic CLAP plug-in used for integration tests.",
            features: &["audio-effect", "utility"],
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
            description: "A deterministic CLAP plug-in used for integration tests.",
            features: &["audio-effect", "utility"],
        }];
        &DESCRIPTORS
    }

    fn new_plugin(_descriptor_id: &str, _host: *const clap_host) -> anyhow::Result<Self::Plugin> {
        Ok(TestGain { gain: 0.75 })
    }
}

clap_export!(TestGainFactory);

#[cfg(test)]
mod tests {
    use super::clap_entry;
    use clap_sys::clap_plugin_factory_t;
    use std::ffi::CStr;

    #[test]
    fn clap_entry_exposes_plugin_factory() {
        let entry = &clap_entry;

        let init = entry.init.expect("entry init");
        assert!(
            unsafe { init(std::ptr::null()) },
            "CLAP entry init should succeed"
        );

        let get_factory = entry.get_factory.expect("get_factory");
        let factory_ptr = unsafe { get_factory(b"clap.plugin-factory\0".as_ptr() as *const i8) }
            as *const clap_plugin_factory_t;
        assert!(!factory_ptr.is_null(), "factory pointer must not be null");
        let factory = unsafe { &*factory_ptr };

        let count = unsafe { (factory.get_plugin_count.expect("get_plugin_count"))(factory_ptr) };
        assert_eq!(count, 1, "expected exactly one test plug-in");

        let descriptor_ptr = unsafe {
            (factory
                .get_plugin_descriptor
                .expect("get_plugin_descriptor"))(factory_ptr, 0)
        };
        assert!(!descriptor_ptr.is_null(), "descriptor must not be null");
        let descriptor = unsafe { &*descriptor_ptr };
        let id = unsafe { CStr::from_ptr(descriptor.id) }
            .to_str()
            .expect("descriptor id should be valid UTF-8");
        assert_eq!(id, "studio.harmoniq.testgain");

        let plugin_ptr = unsafe {
            (factory.create_plugin.expect("create_plugin"))(
                factory_ptr,
                std::ptr::null(),
                descriptor.id,
            )
        };
        assert!(!plugin_ptr.is_null(), "plugin pointer must not be null");

        let plugin = unsafe { &*plugin_ptr };
        let plugin_descriptor = unsafe { &*plugin.desc };
        let plugin_id = unsafe { CStr::from_ptr(plugin_descriptor.id) }
            .to_str()
            .expect("plugin descriptor id should be valid UTF-8");
        assert_eq!(
            plugin_id, id,
            "plugin descriptor should match factory descriptor"
        );

        let plugin_init = plugin.init.expect("plugin init");
        assert!(
            unsafe { plugin_init(plugin_ptr) },
            "plugin init should succeed"
        );

        if let Some(destroy) = plugin.destroy {
            unsafe { destroy(plugin_ptr) };
        }

        if let Some(deinit) = entry.deinit {
            unsafe { deinit() };
        }
    }
}
