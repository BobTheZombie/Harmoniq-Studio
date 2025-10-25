use std::ffi::CString;

use clap_sys::{
    clap_host, clap_plugin, clap_plugin_factory_t, clap_process, clap_process_status,
    CLAP_PROCESS_ERROR,
};
use thiserror::Error;

use crate::discover::ClapPluginDescriptor;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f64,
    pub min_frames_count: u32,
    pub max_frames_count: u32,
}

#[derive(Debug, Error)]
pub enum ActivationError {
    #[error("plug-in refused to activate")]
    ActivationFailed,
    #[error("plug-in returned null instance")]
    NullInstance,
    #[error("plug-in refused to init")]
    InitFailed,
    #[error("factory missing create_plugin entry point")]
    MissingCreatePlugin,
}

/// Represents a running CLAP plug-in instance.
pub struct ClapInstance {
    plugin: *const clap_plugin,
    host: *const clap_host,
    descriptor: ClapPluginDescriptor,
    activated: bool,
}

unsafe impl Send for ClapInstance {}
unsafe impl Sync for ClapInstance {}

impl ClapInstance {
    pub unsafe fn create(
        factory: &clap_plugin_factory_t,
        descriptor: &ClapPluginDescriptor,
        host: *const clap_host,
    ) -> Result<Self, ActivationError> {
        let id = CString::new(descriptor.id.clone()).unwrap();
        let Some(create_plugin) = factory.create_plugin else {
            return Err(ActivationError::MissingCreatePlugin);
        };
        let plugin = create_plugin(factory, host, id.as_ptr());
        if plugin.is_null() {
            return Err(ActivationError::NullInstance);
        }

        let plugin_ref = &*plugin;
        if let Some(init) = plugin_ref.init {
            if !init(plugin) {
                return Err(ActivationError::InitFailed);
            }
        }

        Ok(Self {
            plugin,
            host,
            descriptor: descriptor.clone(),
            activated: false,
        })
    }

    pub fn descriptor(&self) -> &ClapPluginDescriptor {
        &self.descriptor
    }

    pub unsafe fn activate(&mut self, config: AudioConfig) -> Result<(), ActivationError> {
        if self.activated {
            return Ok(());
        }
        let plugin = &*self.plugin;
        if let Some(activate) = plugin.activate {
            if !activate(
                self.plugin,
                config.sample_rate,
                config.min_frames_count,
                config.max_frames_count,
            ) {
                return Err(ActivationError::ActivationFailed);
            }
        }
        self.activated = true;
        Ok(())
    }

    pub unsafe fn deactivate(&mut self) {
        if !self.activated {
            return;
        }
        let plugin = &*self.plugin;
        if let Some(deactivate) = plugin.deactivate {
            deactivate(self.plugin);
        }
        self.activated = false;
    }

    pub unsafe fn start_processing(&mut self) -> bool {
        let plugin = &*self.plugin;
        if let Some(start_processing) = plugin.start_processing {
            return start_processing(self.plugin);
        }
        false
    }

    pub unsafe fn stop_processing(&mut self) {
        let plugin = &*self.plugin;
        if let Some(stop_processing) = plugin.stop_processing {
            stop_processing(self.plugin);
        }
    }

    pub unsafe fn reset(&mut self) {
        let plugin = &*self.plugin;
        if let Some(reset) = plugin.reset {
            reset(self.plugin);
        }
    }

    pub unsafe fn process(&mut self, process: *const clap_process) -> clap_process_status {
        let plugin = &*self.plugin;
        if let Some(process_fn) = plugin.process {
            return process_fn(self.plugin, process);
        }
        CLAP_PROCESS_ERROR.0 as clap_process_status
    }

    pub fn host(&self) -> *const clap_host {
        self.host
    }
}

impl Drop for ClapInstance {
    fn drop(&mut self) {
        unsafe {
            let plugin = &*self.plugin;
            if self.activated {
                if let Some(deactivate) = plugin.deactivate {
                    deactivate(self.plugin);
                }
            }
            if let Some(destroy) = plugin.destroy {
                destroy(self.plugin);
            }
        }
    }
}
