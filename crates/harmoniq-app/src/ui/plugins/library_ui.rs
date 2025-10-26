use std::collections::HashSet;

use eframe::egui::{self, Button, Color32, RichText, ScrollArea};
use harmoniq_plugin_db::{
    scan_plugins, ManifestProber, PluginEntry, PluginFormat, PluginRef, PluginStore, ScanConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CategoryChip {
    Instrument,
    Effect,
    Dynamics,
    Eq,
    Reverb,
    Delay,
    Mod,
    Distortion,
    Utility,
}

impl CategoryChip {
    fn label(self) -> &'static str {
        match self {
            CategoryChip::Instrument => "Instrument",
            CategoryChip::Effect => "Effect",
            CategoryChip::Dynamics => "Dynamics",
            CategoryChip::Eq => "EQ",
            CategoryChip::Reverb => "Reverb",
            CategoryChip::Delay => "Delay",
            CategoryChip::Mod => "Mod",
            CategoryChip::Distortion => "Distortion",
            CategoryChip::Utility => "Utility",
        }
    }

    fn matches(self, entry: &PluginEntry) -> bool {
        match self {
            CategoryChip::Instrument => entry.is_instrument,
            CategoryChip::Effect => !entry.is_instrument,
            CategoryChip::Dynamics => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("dynamics"))
                .unwrap_or(false),
            CategoryChip::Eq => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("eq") || c.eq_ignore_ascii_case("equalizer"))
                .unwrap_or(false),
            CategoryChip::Reverb => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("reverb"))
                .unwrap_or(false),
            CategoryChip::Delay => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("delay"))
                .unwrap_or(false),
            CategoryChip::Mod => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("mod") || c.eq_ignore_ascii_case("modulation"))
                .unwrap_or(false),
            CategoryChip::Distortion => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("distortion") || c.eq_ignore_ascii_case("drive"))
                .unwrap_or(false),
            CategoryChip::Utility => entry
                .category
                .as_ref()
                .map(|c| c.eq_ignore_ascii_case("utility"))
                .unwrap_or(false),
        }
    }
}

const ALL_FORMATS: &[PluginFormat] = &[
    PluginFormat::Clap,
    PluginFormat::Vst3,
    PluginFormat::Ovst3,
    PluginFormat::Harmoniq,
];

const CATEGORY_ORDER: &[CategoryChip] = &[
    CategoryChip::Instrument,
    CategoryChip::Effect,
    CategoryChip::Dynamics,
    CategoryChip::Eq,
    CategoryChip::Reverb,
    CategoryChip::Delay,
    CategoryChip::Mod,
    CategoryChip::Distortion,
    CategoryChip::Utility,
];

#[derive(Debug, Clone)]
pub enum LibraryAction {
    AddInstrument(PluginEntry),
    AddChannelEffect(PluginEntry),
    AddMixerInsert(PluginEntry),
}

#[derive(Debug, Default, Clone)]
struct FilterState {
    search: String,
    selected_formats: HashSet<PluginFormat>,
    selected_categories: HashSet<CategoryChip>,
}

impl FilterState {
    fn new() -> Self {
        Self {
            search: String::new(),
            selected_formats: ALL_FORMATS.iter().copied().collect(),
            selected_categories: [CategoryChip::Instrument, CategoryChip::Effect]
                .into_iter()
                .collect(),
        }
    }

    fn matches(&self, entry: &PluginEntry) -> bool {
        if !self.selected_formats.contains(&entry.reference.format) {
            return false;
        }
        if !self.search.is_empty() {
            let query = self.search.to_ascii_lowercase();
            let haystack = format!(
                "{} {} {}",
                entry.name,
                entry.vendor.as_deref().unwrap_or(""),
                entry.reference.id
            )
            .to_ascii_lowercase();
            if !haystack.contains(&query) {
                return false;
            }
        }
        if self.selected_categories.is_empty() {
            return true;
        }
        self.selected_categories
            .iter()
            .any(|category| category.matches(entry))
    }
}

pub struct PluginLibraryUi {
    entries: Vec<PluginEntry>,
    filter: FilterState,
    selected: Option<PluginRef>,
}

impl PluginLibraryUi {
    pub fn new(store: &PluginStore) -> Self {
        let mut ui = Self {
            entries: store.plugins(),
            filter: FilterState::new(),
            selected: None,
        };
        ui.entries.sort_by(|a, b| a.name.cmp(&b.name));
        ui
    }

    pub fn refresh(&mut self, store: &PluginStore) {
        self.entries = store.plugins();
        self.entries.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn scan_and_refresh(&mut self, store: &PluginStore) {
        let config = ScanConfig::default();
        let report = scan_plugins(&config, &ManifestProber::default());
        let _ = store.merge(report.into_entries());
        self.refresh(store);
    }

    pub fn show<F>(&mut self, ui: &mut egui::Ui, mut on_action: F)
    where
        F: FnMut(LibraryAction),
    {
        ui.heading("Plugin Library");
        ui.add_space(6.0);
        self.draw_filters(ui);
        ui.add_space(4.0);
        let available_entries = self.filtered_entries();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(ui.available_width().min(320.0));
                self.draw_list(ui, &available_entries);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                if let Some(entry) = self
                    .selected
                    .as_ref()
                    .and_then(|selected| {
                        available_entries.iter().find(|e| &e.reference == selected)
                    })
                    .or_else(|| available_entries.first())
                {
                    self.selected = Some(entry.reference.clone());
                    self.draw_details(ui, entry, &mut on_action);
                } else {
                    ui.label("Select a plugin to view details.");
                }
            });
        });
    }

    fn filtered_entries(&self) -> Vec<PluginEntry> {
        let mut filtered: Vec<_> = self
            .entries
            .iter()
            .cloned()
            .filter(|entry| self.filter.matches(entry))
            .collect();
        filtered.sort_by(|a, b| a.name.cmp(&b.name));
        filtered
    }

    fn draw_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.text_edit_singleline(&mut self.filter.search);
            if response.changed() {
                self.selected = None;
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Formats:");
            for format in ALL_FORMATS {
                let enabled = self.filter.selected_formats.contains(format);
                let label = format_label(*format);
                let button = egui::SelectableLabel::new(enabled, label);
                if ui.add(button).clicked() {
                    if enabled {
                        self.filter.selected_formats.remove(format);
                    } else {
                        self.filter.selected_formats.insert(*format);
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Categories:");
            for category in CATEGORY_ORDER {
                let enabled = self.filter.selected_categories.contains(category);
                let button = egui::SelectableLabel::new(enabled, category.label());
                if ui.add(button).clicked() {
                    if enabled {
                        self.filter.selected_categories.remove(category);
                    } else {
                        self.filter.selected_categories.insert(*category);
                    }
                }
            }
        });
    }

    fn draw_list(&mut self, ui: &mut egui::Ui, entries: &[PluginEntry]) {
        ScrollArea::vertical()
            .id_source("plugin-library-scroll")
            .show(ui, |ui| {
                for entry in entries {
                    let is_selected = self
                        .selected
                        .as_ref()
                        .map(|selected| *selected == entry.reference)
                        .unwrap_or(false);
                    let label = RichText::new(&entry.name).strong();
                    let button = Button::new(label).fill(if is_selected {
                        Color32::from_rgb(50, 70, 90)
                    } else {
                        Color32::TRANSPARENT
                    });
                    if ui.add(button).clicked() {
                        self.selected = Some(entry.reference.clone());
                    }
                    ui.label(format!(
                        "{} • {}",
                        entry.vendor.as_deref().unwrap_or("Unknown Vendor"),
                        format_label(entry.reference.format)
                    ));
                    ui.add_space(6.0);
                }
            });
    }

    fn draw_details<F>(&self, ui: &mut egui::Ui, entry: &PluginEntry, on_action: &mut F)
    where
        F: FnMut(LibraryAction),
    {
        ui.heading(&entry.name);
        if let Some(vendor) = &entry.vendor {
            ui.label(format!("Vendor: {}", vendor));
        }
        ui.label(format!("Format: {}", format_label(entry.reference.format)));
        ui.label(format!(
            "I/O: {} in • {} out",
            entry.num_inputs, entry.num_outputs
        ));
        ui.label(format!(
            "Instrument: {}",
            if entry.is_instrument { "Yes" } else { "No" }
        ));
        ui.label(format!(
            "Editor: {}",
            if entry.has_editor { "Yes" } else { "No" }
        ));
        if let Some(category) = &entry.category {
            ui.label(format!("Category: {}", category));
        }
        if let Some(version) = &entry.version {
            ui.label(format!("Version: {}", version));
        }
        if let Some(description) = &entry.description {
            ui.separator();
            ui.label(description);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if entry.is_instrument {
                if ui.button("Add to Channel as Instrument").clicked() {
                    on_action(LibraryAction::AddInstrument(entry.clone()));
                }
            }
            if ui.button("Add to Channel as Effect").clicked() {
                on_action(LibraryAction::AddChannelEffect(entry.clone()));
            }
            if ui.button("Add to Mixer Insert").clicked() {
                on_action(LibraryAction::AddMixerInsert(entry.clone()));
            }
        });
    }
}

fn format_label(format: PluginFormat) -> &'static str {
    match format {
        PluginFormat::Clap => "CLAP",
        PluginFormat::Vst3 => "VST3",
        PluginFormat::Ovst3 => "OpenVST3",
        PluginFormat::Harmoniq => "Harmoniq",
    }
}
