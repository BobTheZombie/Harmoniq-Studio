use std::sync::Arc;

use harmoniq_engine::{AudioBuffer, AudioProcessor, BufferConfig, ChannelLayout, PluginDescriptor};
use harmoniq_plugin_sdk::{
    NativePlugin, ParameterDefinition, ParameterId, ParameterKind, ParameterLayout, ParameterSet,
    ParameterValue, PluginFactory, PluginParameterError,
};

const GAIN_PARAM: &str = "gain";

/// Simple gain plugin for balancing levels in the mixer.
#[derive(Debug, Clone)]
pub struct GainPlugin {
    gain: f32,
    parameters: ParameterSet,
}

impl GainPlugin {
    pub fn new(gain: f32) -> Self {
        let mut plugin = Self::default();
        let _ = plugin.set_gain(gain);
        plugin
    }

    pub fn set_gain(&mut self, gain: f32) -> Result<(), PluginParameterError> {
        self.parameters
            .set(&ParameterId::from(GAIN_PARAM), ParameterValue::from(gain))?;
        self.gain = gain;
        Ok(())
    }
}

impl Default for GainPlugin {
    fn default() -> Self {
        let parameters = ParameterSet::new(gain_layout());
        let gain = parameters
            .get(&ParameterId::from(GAIN_PARAM))
            .and_then(ParameterValue::as_continuous)
            .unwrap_or(1.0);
        Self { gain, parameters }
    }
}

impl AudioProcessor for GainPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.gain", "Gain", "Harmoniq Labs")
    }

    fn prepare(&mut self, _config: &BufferConfig) -> anyhow::Result<()> {
        Ok(())
    }

    fn process(&mut self, buffer: &mut AudioBuffer) -> anyhow::Result<()> {
        for sample in buffer.iter_mut() {
            *sample *= self.gain;
        }
        Ok(())
    }

    fn supports_layout(&self, layout: ChannelLayout) -> bool {
        matches!(layout, ChannelLayout::Mono | ChannelLayout::Stereo)
    }
}

impl NativePlugin for GainPlugin {
    fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    fn on_parameter_changed(
        &mut self,
        id: &ParameterId,
        value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        if id.as_str() == GAIN_PARAM {
            if let Some(gain) = value.as_continuous() {
                self.gain = gain;
            }
        }
        Ok(())
    }
}

fn gain_layout() -> ParameterLayout {
    ParameterLayout::new(vec![ParameterDefinition::new(
        GAIN_PARAM,
        "Gain",
        ParameterKind::continuous(0.0..=2.0, 1.0),
    )
    .with_description("Linear gain applied to the input signal")])
}

pub struct GainPluginFactory;

impl PluginFactory for GainPluginFactory {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor::new("harmoniq.gain", "Gain", "Harmoniq Labs")
    }

    fn parameter_layout(&self) -> Arc<ParameterLayout> {
        Arc::new(gain_layout())
    }

    fn create(&self) -> Box<dyn NativePlugin> {
        Box::new(GainPlugin::default())
    }
}
