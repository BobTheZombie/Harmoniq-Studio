use clap_host::ClapPluginDescriptor;

use crate::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};

/// A lightweight wrapper around a CLAP plug-in instance.
///
/// The current implementation focuses on plumbing metadata into the engine so that
/// CLAP devices can participate in the graph. Audio processing is handled by the
/// plug-in bridge until the full real-time host is implemented.
pub struct ClapNode {
    descriptor: PluginDescriptor,
    clap_id: String,
}

impl ClapNode {
    pub fn new(descriptor: ClapPluginDescriptor) -> Self {
        let ClapPluginDescriptor { id, name, vendor } = descriptor;
        let vendor_name = if vendor.is_empty() {
            "Unknown".to_string()
        } else {
            vendor
        };

        let plugin_descriptor = PluginDescriptor::new(&id, &name, &vendor_name)
            .with_description("CLAP plug-in hosted by Harmoniq Studio");

        Self {
            descriptor: plugin_descriptor,
            clap_id: id,
        }
    }

    pub fn clap_id(&self) -> &str {
        &self.clap_id
    }
}

impl AudioProcessor for ClapNode {
    fn descriptor(&self) -> PluginDescriptor {
        self.descriptor.clone()
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, _buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        // Processing is delegated to the CLAP runtime via the plug-in bridge.
        Ok(())
    }

    fn supports_layout(&self, _layout: ChannelLayout) -> bool {
        true
    }
}
