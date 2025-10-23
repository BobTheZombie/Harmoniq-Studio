use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::{AppEvent, EventBus};

struct BrowserCategory {
    name: String,
    path: PathBuf,
    entries: Vec<BrowserEntry>,
    expanded: bool,
}

struct BrowserEntry {
    name: String,
    path: PathBuf,
}

pub struct BrowserPane {
    root: PathBuf,
    categories: Vec<BrowserCategory>,
    filter: String,
    visible: bool,
    width: f32,
    last_scan: Instant,
}

impl BrowserPane {
    pub fn new(root: PathBuf, visible: bool, width: f32) -> Self {
        let mut pane = Self {
            root,
            categories: Vec::new(),
            filter: String::new(),
            visible,
            width,
            last_scan: Instant::now() - Duration::from_secs(10),
        };
        pane.refresh_categories();
        pane
    }

    fn refresh_categories(&mut self) {
        if self.last_scan.elapsed() < Duration::from_secs(3) {
            return;
        }
        self.categories.clear();
        let groups = [
            ("Samples", "samples"),
            ("Instruments", "instruments"),
            ("Effects", "effects"),
            ("Presets", "presets"),
            ("Projects", "projects"),
        ];
        for (label, folder) in groups {
            let path = self.root.join(folder);
            let entries = read_directory_entries(&path);
            self.categories.push(BrowserCategory {
                name: label.to_string(),
                path,
                entries,
                expanded: true,
            });
        }
        self.last_scan = Instant::now();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, event_bus: &EventBus) {
        self.refresh_categories();
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search browser…")
                    .frame(false)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_source("browser_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for category in &mut self.categories {
                        let header = egui::CollapsingHeader::new(
                            RichText::new(&category.name)
                                .color(palette.text_primary)
                                .strong(),
                        )
                        .default_open(category.expanded);
                        let response = header.show(ui, |ui| {
                            for entry in &category.entries {
                                if !self.matches_filter(&entry.name) {
                                    continue;
                                }
                                let label = RichText::new(&entry.name).color(palette.text_muted);
                                if ui.selectable_label(false, label).clicked() {
                                    event_bus.publish(AppEvent::OpenFile(entry.path.clone()));
                                }
                            }
                        });
                        category.expanded = response.fully_open();
                    }
                });
        });
    }

    fn matches_filter(&self, name: &str) -> bool {
        let needle = self.filter.trim().to_ascii_lowercase();
        if needle.is_empty() {
            true
        } else {
            name.to_ascii_lowercase().contains(&needle)
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }
}

fn read_directory_entries(path: &Path) -> Vec<BrowserEntry> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                entries.push(BrowserEntry {
                    name: format!("{}/", entry.file_name().to_string_lossy()),
                    path,
                });
            } else {
                entries.push(BrowserEntry {
                    name: entry.file_name().to_string_lossy().into(),
                    path,
                });
            }
        }
    }
    entries.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    entries
}
