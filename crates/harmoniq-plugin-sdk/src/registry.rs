use std::sync::Arc;

use harmoniq_engine::{AudioProcessor, PluginDescriptor};

use crate::{ParameterId, ParameterLayout, ParameterSet, ParameterValue, PluginParameterError};

pub trait NativePlugin: AudioProcessor {
    fn parameters(&self) -> &ParameterSet;
    fn parameters_mut(&mut self) -> &mut ParameterSet;

    fn parameter_layout(&self) -> &ParameterLayout {
        self.parameters().layout()
    }

    fn parameter_value(&self, id: &ParameterId) -> Result<ParameterValue, PluginParameterError> {
        self.parameters()
            .get(id)
            .cloned()
            .ok_or_else(|| PluginParameterError::UnknownParameter(id.clone()))
    }

    fn set_parameter(
        &mut self,
        id: &ParameterId,
        value: ParameterValue,
    ) -> Result<(), PluginParameterError> {
        let layout = self.parameters().layout().clone();
        let definition = layout
            .find(id)
            .ok_or_else(|| PluginParameterError::UnknownParameter(id.clone()))?;
        definition.kind.validate(id, &value)?;
        self.on_parameter_changed(id, &value)?;
        self.parameters_mut().set(id, value)?;
        Ok(())
    }

    fn on_parameter_changed(
        &mut self,
        _id: &ParameterId,
        _value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        Ok(())
    }
}

pub trait PluginFactory: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;
    fn parameter_layout(&self) -> Arc<ParameterLayout>;
    fn create(&self) -> Box<dyn NativePlugin>;
}

pub struct PluginModule {
    factories: Vec<Box<dyn PluginFactory>>,
}

impl PluginModule {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
        }
    }

    pub fn register_factory(&mut self, factory: Box<dyn PluginFactory>) -> &mut Self {
        self.factories.push(factory);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn PluginFactory> {
        self.factories.iter().map(|factory| factory.as_ref())
    }

    pub fn into_factories(self) -> Vec<Box<dyn PluginFactory>> {
        self.factories
    }
}

pub struct PluginExport {
    module: PluginModule,
}

impl PluginExport {
    pub fn new(module: PluginModule) -> Self {
        Self { module }
    }

    pub fn module(&self) -> &PluginModule {
        &self.module
    }

    pub fn into_module(self) -> PluginModule {
        self.module
    }
}
