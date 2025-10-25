use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use harmoniq_plugin_db::PluginFormat;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeResult {
    pub name: String,
    pub vendor: Option<String>,
    pub category: Option<String>,
    pub is_instrument: bool,
    pub supports_editor: bool,
}

impl ProbeResult {
    pub fn simulate(path: PathBuf, format: PluginFormat) -> Result<Self, ()> {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Unknown")
            .to_string();
        Ok(Self {
            name,
            vendor: Some(match format {
                PluginFormat::Clap => "Simulated CLAP Vendor".into(),
                PluginFormat::Vst3 => "Simulated VST3 Vendor".into(),
                PluginFormat::Ovst3 => "OpenVST3".into(),
                PluginFormat::Harmoniq => "Harmoniq".into(),
            }),
            category: Some("Utility".into()),
            is_instrument: false,
            supports_editor: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use harmoniq_plugin_db::PluginFormat;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn simulation_returns_metadata() {
        let result = ProbeResult::simulate(PathBuf::from("foo.clap"), PluginFormat::Clap).unwrap();
        assert_eq!(result.name, "foo");
        assert!(result.vendor.is_some());
    }
}
