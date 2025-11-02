use std::fs;
use std::path::{Path, PathBuf};

use egui::Color32;
use serde::Deserialize;
use thiserror::Error;

use crate::theme::HarmoniqPalette;

/// User-configurable overrides for the mixer palette.
///
/// The mixer UI pulls most of its colors from [`HarmoniqPalette`]. By loading a
/// [`MixerSkin`] from disk and applying it to the palette, the mixer can be
/// recolored without recompiling the application.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MixerSkin {
    pub colors: MixerSkinColors,
}

impl MixerSkin {
    /// Load a skin definition from the given JSON file.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, MixerSkinLoadError> {
        let path = path.as_ref();
        let data = fs::read_to_string(path).map_err(|source| MixerSkinLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let skin = serde_json::from_str(&data)?;
        Ok(skin)
    }

    /// Apply the skin to the provided palette.
    pub fn apply(&self, palette: &mut HarmoniqPalette) {
        self.colors.apply(palette);
    }

    /// Returns true if the skin does not override any values.
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MixerSkinColors {
    pub strip_bg: Option<HexColor>,
    pub strip_selected: Option<HexColor>,
    pub strip_solo: Option<HexColor>,
    pub strip_muted: Option<HexColor>,
    pub strip_border: Option<HexColor>,
    pub strip_header: Option<HexColor>,
    pub strip_header_selected: Option<HexColor>,
    pub slot_bg: Option<HexColor>,
    pub slot_active: Option<HexColor>,
    pub slot_border: Option<HexColor>,
    pub toggle_active: Option<HexColor>,
    pub toggle_inactive: Option<HexColor>,
    pub toggle_text: Option<HexColor>,
    pub text_primary: Option<HexColor>,
    pub text_muted: Option<HexColor>,
    pub accent: Option<HexColor>,
    pub accent_alt: Option<HexColor>,
}

impl MixerSkinColors {
    fn apply(&self, palette: &mut HarmoniqPalette) {
        if let Some(color) = self.strip_bg {
            palette.mixer_strip_bg = color.into();
        }
        if let Some(color) = self.strip_selected {
            palette.mixer_strip_selected = color.into();
        }
        if let Some(color) = self.strip_solo {
            palette.mixer_strip_solo = color.into();
        }
        if let Some(color) = self.strip_muted {
            palette.mixer_strip_muted = color.into();
        }
        if let Some(color) = self.strip_border {
            palette.mixer_strip_border = color.into();
        }
        if let Some(color) = self.strip_header {
            palette.mixer_strip_header = color.into();
        }
        if let Some(color) = self.strip_header_selected {
            palette.mixer_strip_header_selected = color.into();
        }
        if let Some(color) = self.slot_bg {
            palette.mixer_slot_bg = color.into();
        }
        if let Some(color) = self.slot_active {
            palette.mixer_slot_active = color.into();
        }
        if let Some(color) = self.slot_border {
            palette.mixer_slot_border = color.into();
        }
        if let Some(color) = self.toggle_active {
            palette.mixer_toggle_active = color.into();
        }
        if let Some(color) = self.toggle_inactive {
            palette.mixer_toggle_inactive = color.into();
        }
        if let Some(color) = self.toggle_text {
            palette.mixer_toggle_text = color.into();
        }
        if let Some(color) = self.text_primary {
            palette.text_primary = color.into();
        }
        if let Some(color) = self.text_muted {
            palette.text_muted = color.into();
        }
        if let Some(color) = self.accent {
            palette.accent = color.into();
        }
        if let Some(color) = self.accent_alt {
            palette.accent_alt = color.into();
        }
    }

    fn is_empty(&self) -> bool {
        self.strip_bg.is_none()
            && self.strip_selected.is_none()
            && self.strip_solo.is_none()
            && self.strip_muted.is_none()
            && self.strip_border.is_none()
            && self.strip_header.is_none()
            && self.strip_header_selected.is_none()
            && self.slot_bg.is_none()
            && self.slot_active.is_none()
            && self.slot_border.is_none()
            && self.toggle_active.is_none()
            && self.toggle_inactive.is_none()
            && self.toggle_text.is_none()
            && self.text_primary.is_none()
            && self.text_muted.is_none()
            && self.accent.is_none()
            && self.accent_alt.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HexColor(Color32);

impl From<HexColor> for Color32 {
    fn from(value: HexColor) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        parse_hex_color(&input)
            .map(HexColor)
            .map_err(|err| serde::de::Error::custom(err))
    }
}

fn parse_hex_color(input: &str) -> Result<Color32, String> {
    let trimmed = input.trim();
    let without_prefix = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let (rgb, alpha) = match without_prefix.len() {
        6 => (without_prefix, 0xFF),
        8 => {
            let (rgb, alpha) = without_prefix.split_at(6);
            let alpha = u8::from_str_radix(alpha, 16)
                .map_err(|_| format!("invalid alpha component in color '{input}'"))?;
            (rgb, alpha)
        }
        _ => {
            return Err(format!(
                "expected hex color in RRGGBB or RRGGBBAA format, got '{input}'"
            ))
        }
    };

    let r = u8::from_str_radix(&rgb[0..2], 16)
        .map_err(|_| format!("invalid red component in color '{input}'"))?;
    let g = u8::from_str_radix(&rgb[2..4], 16)
        .map_err(|_| format!("invalid green component in color '{input}'"))?;
    let b = u8::from_str_radix(&rgb[4..6], 16)
        .map_err(|_| format!("invalid blue component in color '{input}'"))?;

    Ok(Color32::from_rgba_unmultiplied(r, g, b, alpha))
}

#[derive(Debug, Error)]
pub enum MixerSkinLoadError {
    #[error("failed to read mixer skin from {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse mixer skin JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

impl MixerSkinLoadError {
    /// Returns the underlying IO error if this error was caused by IO.
    pub fn io_error(&self) -> Option<&std::io::Error> {
        match self {
            MixerSkinLoadError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
