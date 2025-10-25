use std::ffi::CString;

use anyhow::Result;
use clap_sys::{clap_host, clap_plugin_descriptor_t, clap_process, clap_process_status};

/// Description of a plug-in that can be exported via CLAP.
#[derive(Clone)]
pub struct PluginDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub url: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub features: &'static [&'static str],
}

impl PluginDescriptor {
    pub fn to_raw(&'static self) -> &'static clap_plugin_descriptor_t {
        let features = leak_feature_list(self.features);
        Box::leak(Box::new(clap_plugin_descriptor_t {
            clap_version: clap_sys::CLAP_VERSION_LATEST,
            id: leak_c_string(self.id),
            name: leak_c_string(self.name),
            vendor: leak_c_string(self.vendor),
            url: leak_c_string(self.url),
            manual_url: ::core::ptr::null(),
            support_url: ::core::ptr::null(),
            version: leak_c_string(self.version),
            description: leak_c_string(self.description),
            features,
        }))
    }
}

fn leak_c_string(input: &'static str) -> *const i8 {
    CString::new(input)
        .expect("descriptor strings must not contain null bytes")
        .into_raw() as *const i8
}

fn leak_feature_list(features: &'static [&'static str]) -> *const *const i8 {
    let mut c_strings: Vec<*const i8> = features
        .iter()
        .map(|feature| leak_c_string(feature))
        .collect();
    c_strings.push(::core::ptr::null());
    Box::leak(c_strings.into_boxed_slice()).as_ptr()
}

pub struct ActivationContext {
    pub sample_rate: f64,
    pub min_frames_count: u32,
    pub max_frames_count: u32,
}

pub trait AudioProcessor {
    fn process(&mut self, process: &mut clap_process) -> clap_process_status;
}

#[allow(dead_code)]
pub trait Params {}

#[allow(dead_code)]
pub trait State {
    fn save(&mut self, _stream: &mut dyn std::io::Write) -> Result<()> {
        Ok(())
    }

    fn load(&mut self, _stream: &mut dyn std::io::Read) -> Result<()> {
        Ok(())
    }
}

#[allow(dead_code)]
pub trait Gui {
    fn show(&mut self, _host: *const clap_host) -> Result<()> {
        Ok(())
    }
}

#[allow(dead_code)]
pub trait Latency {
    fn latency_samples(&self) -> u32 {
        0
    }
}

#[allow(dead_code)]
pub trait Tail {
    fn tail_samples(&self) -> u32 {
        0
    }
}

#[allow(dead_code)]
pub trait NotePorts {}

pub trait Plugin: AudioProcessor + Send + 'static {
    fn descriptor(&self) -> &'static PluginDescriptor;
    fn init(&mut self) -> Result<()> {
        Ok(())
    }
    fn activate(&mut self, _context: &ActivationContext) -> Result<()> {
        Ok(())
    }
    fn deactivate(&mut self) {}
    fn reset(&mut self) {}
    fn on_main_thread(&mut self) {}
}

pub trait PluginFactory {
    type Plugin: Plugin;

    fn descriptors() -> &'static [PluginDescriptor];
    fn new_plugin(descriptor_id: &str, host: *const clap_host) -> Result<Self::Plugin>;
}
