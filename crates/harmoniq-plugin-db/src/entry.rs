use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Harmoniq,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginId {
    pub format: PluginFormat,
    pub path: String,
}

impl PluginId {
    pub fn new(format: PluginFormat, path: impl Into<String>) -> Self {
        Self {
            format,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub id: PluginId,
    pub name: String,
    pub vendor: Option<String>,
    pub category: Option<String>,
    pub is_instrument: bool,
    pub supports_editor: bool,
    pub last_seen: DateTime<Utc>,
    pub verified: bool,
}

impl PluginEntry {
    pub fn new(id: PluginId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            vendor: None,
            category: None,
            is_instrument: false,
            supports_editor: false,
            last_seen: Utc::now(),
            verified: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn plugin_id_roundtrip() {
        let id = PluginId::new(PluginFormat::Clap, "/tmp/test");
        let json = serde_json::to_string(&id).unwrap();
        let roundtrip: PluginId = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, id);
    }
}
