use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Ovst3,
    Harmoniq,
}

impl PluginFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "clap" => Some(Self::Clap),
            "vst3" => Some(Self::Vst3),
            "ovst3" => Some(Self::Ovst3),
            "hqplug" => Some(Self::Harmoniq),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PluginRef {
    pub id: String,
    pub format: PluginFormat,
    pub path: String,
}

impl PluginRef {
    pub fn new(id: impl Into<String>, format: PluginFormat, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            format,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub reference: PluginRef,
    pub name: String,
    pub vendor: Option<String>,
    pub category: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub is_instrument: bool,
    pub has_editor: bool,
    pub num_inputs: u32,
    pub num_outputs: u32,
    pub quarantined: bool,
    pub last_seen: DateTime<Utc>,
}

impl PluginEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference: PluginRef,
        name: impl Into<String>,
        vendor: Option<String>,
        category: Option<String>,
        version: Option<String>,
        description: Option<String>,
        is_instrument: bool,
        has_editor: bool,
        num_inputs: u32,
        num_outputs: u32,
    ) -> Self {
        Self {
            reference,
            name: name.into(),
            vendor,
            category,
            version,
            description,
            is_instrument,
            has_editor,
            num_inputs,
            num_outputs,
            quarantined: false,
            last_seen: Utc::now(),
        }
    }

    pub fn mark_quarantined(&mut self) {
        self.quarantined = true;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub vendor: Option<String>,
    pub category: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub is_instrument: bool,
    pub has_editor: bool,
    pub num_inputs: u32,
    pub num_outputs: u32,
}

impl PluginMetadata {
    pub fn into_entry(self, format: PluginFormat, path: impl Into<String>) -> PluginEntry {
        let reference = PluginRef::new(self.id.clone(), format, path);
        PluginEntry::new(
            reference,
            self.name,
            self.vendor,
            self.category,
            self.version,
            self.description,
            self.is_instrument,
            self.has_editor,
            self.num_inputs,
            self.num_outputs,
        )
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn plugin_ref_roundtrip() {
        let reference = PluginRef::new("test", PluginFormat::Clap, "/tmp/test");
        let json = serde_json::to_string(&reference).unwrap();
        let roundtrip: PluginRef = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, reference);
    }
}
