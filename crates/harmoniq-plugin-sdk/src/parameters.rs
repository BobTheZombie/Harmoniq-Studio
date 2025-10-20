use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParameterId(String);

impl ParameterId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ParameterId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for ParameterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub id: ParameterId,
    pub name: String,
    pub kind: ParameterKind,
    pub unit: Option<String>,
    pub description: Option<String>,
}

impl ParameterDefinition {
    pub fn new(id: impl Into<ParameterId>, name: impl Into<String>, kind: ParameterKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            unit: None,
            description: None,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterKind {
    Continuous(ContinuousParameterOptions),
    Toggle {
        default: bool,
    },
    Choice {
        options: Vec<String>,
        default: usize,
    },
}

impl ParameterKind {
    pub fn continuous(range: std::ops::RangeInclusive<f32>, default: f32) -> Self {
        Self::Continuous(ContinuousParameterOptions::new(range, default))
    }

    pub fn default_value(&self) -> ParameterValue {
        match self {
            ParameterKind::Continuous(opts) => ParameterValue::Continuous(opts.default),
            ParameterKind::Toggle { default } => ParameterValue::Toggle(*default),
            ParameterKind::Choice { default, .. } => ParameterValue::Choice(*default),
        }
    }

    pub fn validate(
        &self,
        id: &ParameterId,
        value: &ParameterValue,
    ) -> Result<(), PluginParameterError> {
        match (self, value) {
            (ParameterKind::Continuous(opts), ParameterValue::Continuous(v)) => {
                if *v < opts.min || *v > opts.max {
                    Err(PluginParameterError::OutOfRange {
                        id: id.clone(),
                        min: opts.min,
                        max: opts.max,
                        value: *v,
                    })
                } else {
                    Ok(())
                }
            }
            (ParameterKind::Toggle { .. }, ParameterValue::Toggle(_)) => Ok(()),
            (ParameterKind::Choice { options, .. }, ParameterValue::Choice(idx)) => {
                if *idx >= options.len() {
                    Err(PluginParameterError::InvalidChoice {
                        id: id.clone(),
                        index: *idx,
                        count: options.len(),
                    })
                } else {
                    Ok(())
                }
            }
            _ => Err(PluginParameterError::WrongType {
                id: id.clone(),
                expected: self.type_name(),
                actual: value.type_name(),
            }),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            ParameterKind::Continuous(_) => "continuous",
            ParameterKind::Toggle { .. } => "toggle",
            ParameterKind::Choice { .. } => "choice",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousParameterOptions {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: Option<f32>,
    pub skew: Option<f32>,
}

impl ContinuousParameterOptions {
    pub fn new(range: std::ops::RangeInclusive<f32>, default: f32) -> Self {
        let min = *range.start();
        let max = *range.end();
        assert!(min <= max, "parameter min must be <= max");
        assert!(default >= min && default <= max, "default outside range");
        Self {
            min,
            max,
            default,
            step: None,
            skew: None,
        }
    }

    pub fn with_step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    pub fn with_skew(mut self, skew: f32) -> Self {
        self.skew = Some(skew);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParameterValue {
    Continuous(f32),
    Toggle(bool),
    Choice(usize),
}

impl ParameterValue {
    pub fn as_continuous(&self) -> Option<f32> {
        match self {
            ParameterValue::Continuous(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_toggle(&self) -> Option<bool> {
        match self {
            ParameterValue::Toggle(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_choice(&self) -> Option<usize> {
        match self {
            ParameterValue::Choice(v) => Some(*v),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            ParameterValue::Continuous(_) => "continuous",
            ParameterValue::Toggle(_) => "toggle",
            ParameterValue::Choice(_) => "choice",
        }
    }
}

impl From<f32> for ParameterValue {
    fn from(value: f32) -> Self {
        ParameterValue::Continuous(value)
    }
}

impl From<bool> for ParameterValue {
    fn from(value: bool) -> Self {
        ParameterValue::Toggle(value)
    }
}

impl From<usize> for ParameterValue {
    fn from(value: usize) -> Self {
        ParameterValue::Choice(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterLayout {
    parameters: Vec<ParameterDefinition>,
}

impl ParameterLayout {
    pub fn new(parameters: Vec<ParameterDefinition>) -> Self {
        Self { parameters }
    }

    pub fn parameters(&self) -> &[ParameterDefinition] {
        &self.parameters
    }

    pub fn find(&self, id: &ParameterId) -> Option<&ParameterDefinition> {
        self.parameters
            .iter()
            .find(|definition| &definition.id == id)
    }
}

#[derive(Debug, Clone)]
pub struct ParameterSet {
    layout: ParameterLayout,
    values: HashMap<ParameterId, ParameterValue>,
}

impl ParameterSet {
    pub fn new(layout: ParameterLayout) -> Self {
        let mut values = HashMap::new();
        for parameter in layout.parameters() {
            values.insert(parameter.id.clone(), parameter.kind.default_value());
        }
        Self { layout, values }
    }

    pub fn layout(&self) -> &ParameterLayout {
        &self.layout
    }

    pub fn get(&self, id: &ParameterId) -> Option<&ParameterValue> {
        self.values.get(id)
    }

    pub fn set(
        &mut self,
        id: &ParameterId,
        value: ParameterValue,
    ) -> Result<(), PluginParameterError> {
        let definition = self
            .layout
            .find(id)
            .ok_or_else(|| PluginParameterError::UnknownParameter(id.clone()))?;
        definition.kind.validate(id, &value)?;
        self.values.insert(id.clone(), value);
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ParameterId, &ParameterValue)> {
        self.values.iter().map(|(id, value)| (id, value))
    }
}

#[derive(Debug, Error)]
pub enum PluginParameterError {
    #[error("unknown parameter `{0}`")]
    UnknownParameter(ParameterId),
    #[error("parameter `{id}` expected {expected} value but received {actual}")]
    WrongType {
        id: ParameterId,
        expected: &'static str,
        actual: &'static str,
    },
    #[error("parameter `{id}` received value {value} outside of range {min}..={max}")]
    OutOfRange {
        id: ParameterId,
        min: f32,
        max: f32,
        value: f32,
    },
    #[error("parameter `{id}` received choice index {index} outside of 0..{count}")]
    InvalidChoice {
        id: ParameterId,
        index: usize,
        count: usize,
    },
}
