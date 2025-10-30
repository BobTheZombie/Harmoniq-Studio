//! Minimal plugin host façade for Mixer inserts (non-RT).
#![cfg(feature = "clap_host")]

pub mod clap_hosting {
    use std::path::Path;

    use anyhow::{anyhow, Result};
    use clap_host::{
        discover::{ClapLibrary, PluginDiscovery},
        ffi::{clap_host, clap_plugin_factory_t},
        instance::{ActivationError, AudioConfig, ClapInstance},
        ClapPluginDescriptor,
    };
    use clap_sys::CLAP_VERSION_LATEST;
    use core::ffi::c_char;

    const HOST_NAME: &[u8] = b"Harmoniq Studio\0";
    const HOST_VENDOR: &[u8] = b"Harmoniq Labs\0";
    const HOST_URL: &[u8] = b"https://harmoniq.audio\0";
    const HOST_VERSION: &[u8] = b"0.1.0\0";

    static HOST_INFO: clap_host = clap_host {
        clap_version: CLAP_VERSION_LATEST,
        host_data: core::ptr::null_mut(),
        name: HOST_NAME.as_ptr() as *const c_char,
        vendor: HOST_VENDOR.as_ptr() as *const c_char,
        url: HOST_URL.as_ptr() as *const c_char,
        version: HOST_VERSION.as_ptr() as *const c_char,
        get_extension: None,
        request_restart: None,
        request_process: None,
        request_callback: None,
    };

    /// Represents a CLAP plug-in slot managed by the host façade.
    pub struct ClapSlot {
        pub name: String,
        pub descriptor: ClapPluginDescriptor,
        pub library: ClapLibrary,
        pub instance: ClapInstance,
        pub bypass: bool,
    }

    impl ClapSlot {
        /// Loads a CLAP plug-in instance from the given path and plug-in identifier.
        pub fn load<P: AsRef<Path>>(
            path: P,
            clap_id: &str,
            sample_rate: f64,
            block_size: u32,
        ) -> Result<Self> {
            unsafe {
                let library = ClapLibrary::load(path)?;
                let factory = library.factory()?;
                let descriptor = PluginDiscovery::new(factory)
                    .list()
                    .into_iter()
                    .find(|desc| desc.id == clap_id)
                    .ok_or_else(|| anyhow!("CLAP plugin id not found: {clap_id}"))?;

                let mut instance = ClapSlot::create_instance(factory, &descriptor)?;
                ClapSlot::activate_instance(&mut instance, sample_rate, block_size)?;

                Ok(Self {
                    name: descriptor.name.clone(),
                    descriptor,
                    library,
                    instance,
                    bypass: false,
                })
            }
        }

        fn create_instance(
            factory: &clap_plugin_factory_t,
            descriptor: &ClapPluginDescriptor,
        ) -> Result<ClapInstance, ActivationError> {
            unsafe { ClapInstance::create(factory, descriptor, &HOST_INFO as *const clap_host) }
        }

        fn activate_instance(
            instance: &mut ClapInstance,
            sample_rate: f64,
            block_size: u32,
        ) -> Result<(), ActivationError> {
            unsafe {
                instance.activate(AudioConfig {
                    sample_rate,
                    min_frames_count: block_size,
                    max_frames_count: block_size,
                })
            }
        }

        /// Sets the bypass state of the plug-in.
        pub fn set_bypass(&mut self, bypass: bool) {
            self.bypass = bypass;
        }

        /// Requests the plug-in to open its editor (if any).
        pub fn open_editor(&mut self) {
            log::debug!("CLAP plug-in '{}' requested editor open", self.name);
        }
    }
}
