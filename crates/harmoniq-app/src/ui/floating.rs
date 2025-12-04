use std::fs;
use std::io;
use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use egui::{Pos2, Rect, Vec2};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FloatingWindowId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FloatingKind {
    PluginEditor { plugin_uid: String },
    PianoRoll { track_id: u32 },
    MixerInsert { insert_idx: u16 },
    MidiMonitor,
    Performance,
    Inspector { selection: Option<String> },
    TaskManager,
    UiWidget { uid: String },
}

impl FloatingKind {
    pub fn uid_label(&self) -> String {
        match self {
            FloatingKind::UiWidget { uid } => uid.clone(),
            FloatingKind::PluginEditor { plugin_uid } => format!("plugin.{plugin_uid}"),
            FloatingKind::PianoRoll { track_id } => format!("piano_roll.{track_id}"),
            FloatingKind::MixerInsert { insert_idx } => format!("mixer_insert.{insert_idx}"),
            FloatingKind::MidiMonitor => "midi_monitor".into(),
            FloatingKind::Performance => "performance".into(),
            FloatingKind::Inspector { selection } => selection
                .as_ref()
                .map(|selection| format!("inspector.{selection}"))
                .unwrap_or_else(|| "inspector".into()),
            FloatingKind::TaskManager => "task_manager".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingWindow {
    pub id: FloatingWindowId,
    pub kind: FloatingKind,
    pub title: String,
    pub open: bool,
    pub pos: Pos2,
    pub size: Vec2,
    pub z: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub translucent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingWindows {
    pub next_id: u64,
    pub windows: IndexMap<FloatingWindowId, FloatingWindow>,
    pub last_focus: Option<FloatingWindowId>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    dirty_since: Option<Instant>,
}

impl Default for FloatingWindows {
    fn default() -> Self {
        Self {
            next_id: 1,
            windows: IndexMap::new(),
            last_focus: None,
            dirty: false,
            dirty_since: None,
        }
    }
}

impl FloatingWindows {
    pub fn spawn(
        &mut self,
        kind: FloatingKind,
        title: impl Into<String>,
        pos: Pos2,
        size: Vec2,
    ) -> FloatingWindowId {
        let id = FloatingWindowId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let z = self
            .windows
            .values()
            .map(|w| w.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let window = FloatingWindow {
            id,
            kind,
            title: title.into(),
            open: true,
            pos,
            size,
            z,
            pinned: false,
            translucent: false,
        };
        self.windows.insert(id, window);
        self.bring_to_front(id);
        self.mark_dirty();
        id
    }

    pub fn ensure_open(
        &mut self,
        kind: FloatingKind,
        title: impl Into<String>,
        pos: Pos2,
        size: Vec2,
    ) -> FloatingWindowId {
        if let Some(id) = self.find_by_kind(&kind) {
            if let Some(window) = self.windows.get_mut(&id) {
                if !window.open {
                    window.open = true;
                    window.pos = pos;
                    window.size = size;
                    self.mark_dirty();
                }
            }
            self.bring_to_front(id);
            id
        } else {
            self.spawn(kind, title, pos, size)
        }
    }

    pub fn close(&mut self, id: FloatingWindowId) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.open {
                window.open = false;
                self.mark_dirty();
            }
        }
    }

    pub fn toggle(&mut self, id: FloatingWindowId) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.open = !window.open;
            self.mark_dirty();
        }
    }

    pub fn toggle_by_kind(
        &mut self,
        kind: FloatingKind,
        title: impl Into<String>,
        pos: Pos2,
        size: Vec2,
    ) {
        if let Some(id) = self.find_by_kind(&kind) {
            if self.is_open(id) {
                self.close(id);
            } else {
                self.ensure_open(kind, title, pos, size);
            }
        } else {
            self.spawn(kind, title, pos, size);
        }
    }

    pub fn kill(&mut self, id: FloatingWindowId) -> bool {
        let removed = self.windows.shift_remove(&id).is_some();
        if removed {
            if self.last_focus == Some(id) {
                self.last_focus = None;
            }
            self.mark_dirty();
        }
        removed
    }

    pub fn close_all_of_kind(&mut self, kind: &FloatingKind) {
        let mut changed = false;
        for window in self.windows.values_mut() {
            if &window.kind == kind && window.open {
                window.open = false;
                changed = true;
            }
        }
        if changed {
            self.mark_dirty();
        }
    }

    pub fn bring_to_front(&mut self, id: FloatingWindowId) {
        let max_z = self.windows.values().map(|w| w.z).max().unwrap_or(0);

        if let Some(window) = self.windows.get_mut(&id) {
            if window.z <= max_z {
                window.z = max_z.saturating_add(1);
            }
            self.last_focus = Some(id);
            self.mark_dirty();
        }
    }

    pub fn iter_sorted_by_z(&self) -> Vec<FloatingWindowId> {
        let mut windows: Vec<_> = self
            .windows
            .values()
            .map(|window| (window.z, window.id))
            .collect();
        windows.sort_by_key(|(z, _)| *z);
        windows.into_iter().map(|(_, id)| id).collect()
    }

    pub fn update_bounds(&mut self, id: FloatingWindowId, rect: Rect) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.pos != rect.min || window.size != rect.size() {
                window.pos = rect.min;
                window.size = rect.size();
                self.mark_dirty();
            }
        }
    }

    pub fn set_open(&mut self, id: FloatingWindowId, open: bool) {
        if let Some(window) = self.windows.get_mut(&id) {
            if window.open != open {
                window.open = open;
                self.mark_dirty();
            }
        }
    }

    pub fn retain_valid<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&FloatingKind) -> bool,
    {
        let mut changed = false;
        self.windows.retain(|_, window| {
            if predicate(&window.kind) {
                true
            } else {
                changed = true;
                false
            }
        });
        if changed {
            self.mark_dirty();
        }
    }

    pub fn is_open(&self, id: FloatingWindowId) -> bool {
        self.windows
            .get(&id)
            .map(|window| window.open)
            .unwrap_or(false)
    }

    pub fn find_by_kind(&self, kind: &FloatingKind) -> Option<FloatingWindowId> {
        self.windows
            .iter()
            .find_map(|(id, window)| (window.kind == *kind).then_some(*id))
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn dirty_since(&self) -> Option<Instant> {
        self.dirty_since
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.dirty_since = None;
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut snapshot = self.clone();
        snapshot.dirty = false;
        snapshot.dirty_since = None;
        let file = FloatingWindowsFile {
            version: 1,
            data: snapshot,
        };
        let json = serde_json::to_vec_pretty(&file)?;
        fs::write(path, json)?;
        self.clear_dirty();
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        let file: FloatingWindowsFile = match serde_json::from_slice(&data) {
            Ok(file) => file,
            Err(_) => return Ok(()),
        };

        if file.version != 1 {
            return Ok(());
        }

        *self = file.data;
        self.clear_dirty();
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FloatingWindowsFile {
    version: u32,
    data: FloatingWindows,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn spawn_assigns_incrementing_ids() {
        let mut windows = FloatingWindows::default();
        let id1 = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        let id2 = windows.spawn(
            FloatingKind::Performance,
            "Performance",
            Pos2::new(20.0, 20.0),
            Vec2::new(100.0, 100.0),
        );
        assert_ne!(id1, id2);
        assert!(windows.is_open(id1));
        assert!(windows.is_open(id2));
    }

    #[test]
    fn bring_to_front_updates_z_order() {
        let mut windows = FloatingWindows::default();
        let id1 = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        let id2 = windows.spawn(
            FloatingKind::Performance,
            "Performance",
            Pos2::new(20.0, 20.0),
            Vec2::new(100.0, 100.0),
        );
        let z_values = windows.iter_sorted_by_z();
        assert_eq!(z_values[0], id1);
        windows.bring_to_front(id1);
        let sorted = windows.iter_sorted_by_z();
        assert_eq!(sorted.last().copied(), Some(id1));
    }

    #[test]
    fn update_bounds_marks_dirty() {
        let mut windows = FloatingWindows::default();
        let id = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        windows.clear_dirty();
        windows.update_bounds(
            id,
            Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(120.0, 140.0)),
        );
        assert!(windows.dirty());
        let win = windows.windows.get(&id).unwrap();
        assert_eq!(win.pos, Pos2::new(10.0, 20.0));
        assert_eq!(win.size, Vec2::new(120.0, 140.0));
    }

    #[test]
    fn persistence_round_trip() {
        let mut windows = FloatingWindows::default();
        let id = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        windows.bring_to_front(id);
        let file = NamedTempFile::new().unwrap();
        windows.save(file.path()).unwrap();

        let mut loaded = FloatingWindows::default();
        loaded.load(file.path()).unwrap();
        assert_eq!(loaded.windows.len(), 1);
        let loaded_id = loaded.iter_sorted_by_z().into_iter().next().unwrap();
        assert_eq!(loaded_id, id);
        let win = loaded.windows.get(&loaded_id).unwrap();
        assert_eq!(win.title, "MIDI Monitor");
        assert!(win.open);
    }

    #[test]
    fn toggle_existing_window() {
        let mut windows = FloatingWindows::default();
        let id = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        windows.toggle(id);
        assert!(!windows.is_open(id));
        windows.toggle(id);
        assert!(windows.is_open(id));
    }

    #[test]
    fn ensure_open_reopens_closed_window() {
        let mut windows = FloatingWindows::default();
        let id = windows.spawn(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(0.0, 0.0),
            Vec2::new(100.0, 100.0),
        );
        windows.close(id);
        assert!(!windows.is_open(id));
        let reopened = windows.ensure_open(
            FloatingKind::MidiMonitor,
            "MIDI Monitor",
            Pos2::new(5.0, 5.0),
            Vec2::new(50.0, 50.0),
        );
        assert_eq!(reopened, id);
        assert!(windows.is_open(id));
    }
}
