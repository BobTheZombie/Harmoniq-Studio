use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ui::config::config_dir;

const CONFIG_FILE: &str = "qwerty.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VelocityCurveSetting {
    Linear,
    Soft,
    Hard,
    Fixed,
}

impl Default for VelocityCurveSetting {
    fn default() -> Self {
        Self::Linear
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardLayout {
    TopRow,
    DualManual,
}

impl Default for KeyboardLayout {
    fn default() -> Self {
        Self::TopRow
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QwertyConfigFile {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_octave")]
    pub octave: i8,
    #[serde(default)]
    pub velocity_curve: VelocityCurveSetting,
    #[serde(default = "default_fixed_velocity")]
    pub fixed_velocity: u8,
    #[serde(default = "default_channel")]
    pub channel: u8,
    #[serde(default)]
    pub layout: KeyboardLayout,
    #[serde(default = "default_sustain_key")]
    pub sustain_key: SustainKey,
}

fn default_enabled() -> bool {
    true
}

fn default_octave() -> i8 {
    4
}

fn default_fixed_velocity() -> u8 {
    100
}

fn default_channel() -> u8 {
    1
}

fn default_sustain_key() -> SustainKey {
    SustainKey::Space
}

impl Default for QwertyConfigFile {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            octave: default_octave(),
            velocity_curve: VelocityCurveSetting::default(),
            fixed_velocity: default_fixed_velocity(),
            channel: default_channel(),
            layout: KeyboardLayout::default(),
            sustain_key: default_sustain_key(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SustainKey {
    Space,
    CapsLock,
}

impl Default for SustainKey {
    fn default() -> Self {
        SustainKey::Space
    }
}

#[derive(Debug, Clone)]
pub struct QwertyConfig {
    pub file: QwertyConfigFile,
    path: PathBuf,
}

impl QwertyConfig {
    pub fn load() -> Self {
        let path = config_dir()
            .map(|dir| dir.join(CONFIG_FILE))
            .unwrap_or_else(|_| PathBuf::from(CONFIG_FILE));
        if let Ok(data) = fs::read_to_string(&path) {
            match serde_json::from_str::<QwertyConfigFile>(&data) {
                Ok(file) => Self { file, path },
                Err(_) => Self {
                    file: QwertyConfigFile::default(),
                    path,
                },
            }
        } else {
            Self {
                file: QwertyConfigFile::default(),
                path,
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = self
            .path
            .parent()
            .map(|parent| parent.to_path_buf())
            .or_else(|| config_dir().ok())
            .context("missing config directory")?;
        if !dir.exists() {
            fs::create_dir_all(&dir).context("create qwerty config directory")?;
        }
        let json = serde_json::to_string_pretty(&self.file)?;
        let path = if self.path.file_name().is_some() {
            self.path.clone()
        } else {
            dir.join(CONFIG_FILE)
        };
        let mut file = fs::File::create(&path).context("create qwerty config file")?;
        file.write_all(json.as_bytes())
            .context("write qwerty config")?;
        Ok(())
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}
