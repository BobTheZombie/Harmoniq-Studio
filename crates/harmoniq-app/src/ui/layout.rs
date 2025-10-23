use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_dock::DockState;
use serde::{Deserialize, Serialize};

use crate::ui::workspace::{build_default_workspace, WorkspacePane};

#[derive(Debug, Serialize, Deserialize)]
struct StoredLayout {
    #[serde(default)]
    dock: Option<DockState<WorkspacePane>>,
    #[serde(default = "StoredLayout::default_browser_width")]
    browser_width: f32,
    #[serde(default = "StoredLayout::default_browser_visible")]
    browser_visible: bool,
}

impl StoredLayout {
    fn default_browser_width() -> f32 {
        260.0
    }

    fn default_browser_visible() -> bool {
        true
    }
}

impl Default for StoredLayout {
    fn default() -> Self {
        Self {
            dock: Some(build_default_workspace()),
            browser_width: Self::default_browser_width(),
            browser_visible: Self::default_browser_visible(),
        }
    }
}

pub struct LayoutState {
    path: PathBuf,
    stored: StoredLayout,
    dirty: bool,
    last_save: Instant,
}

impl LayoutState {
    pub fn load(path: PathBuf) -> Self {
        let stored = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self {
            path,
            stored,
            dirty: false,
            last_save: Instant::now(),
        }
    }

    pub fn dock(&self) -> Option<DockState<WorkspacePane>> {
        self.stored.dock.clone()
    }

    pub fn store_dock(&mut self, dock: &DockState<WorkspacePane>) {
        let serialized_current = serde_json::to_string(dock).ok();
        let serialized_stored = self
            .stored
            .dock
            .as_ref()
            .and_then(|stored| serde_json::to_string(stored).ok());

        if serialized_current != serialized_stored {
            self.stored.dock = Some(dock.clone());
            self.dirty = true;
        }
    }

    pub fn browser_visible(&self) -> bool {
        self.stored.browser_visible
    }

    pub fn set_browser_visible(&mut self, visible: bool) {
        if self.stored.browser_visible != visible {
            self.stored.browser_visible = visible;
            self.dirty = true;
        }
    }

    pub fn browser_width(&self) -> f32 {
        self.stored.browser_width
    }

    pub fn set_browser_width(&mut self, width: f32) {
        let width = width.clamp(180.0, 480.0);
        if (self.stored.browser_width - width).abs() > f32::EPSILON {
            self.stored.browser_width = width;
            self.dirty = true;
        }
    }

    pub fn maybe_save(&mut self) {
        if self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            self.flush();
        }
    }

    pub fn flush(&mut self) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(serialised) = serde_json::to_string_pretty(&self.stored) {
            if fs::write(&self.path, serialised).is_ok() {
                self.dirty = false;
                self.last_save = Instant::now();
            }
        }
    }
}
