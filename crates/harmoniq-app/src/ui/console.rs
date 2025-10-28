use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use harmoniq_ui::HarmoniqPalette;

use crate::ui::focus::InputFocus;
use crate::ui::workspace::WorkspacePane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl LogLevel {
    fn label(self) -> &'static str {
        match self {
            LogLevel::Info => "info",
            LogLevel::Warning => "warn",
            LogLevel::Error => "error",
        }
    }

    fn color(self, palette: &HarmoniqPalette) -> egui::Color32 {
        match self {
            LogLevel::Info => palette.text_muted,
            LogLevel::Warning => palette.warning,
            LogLevel::Error => palette.accent,
        }
    }
}

#[derive(Debug, Clone)]
struct ConsoleEntry {
    timestamp: Instant,
    level: LogLevel,
    message: String,
    sticky: bool,
}

impl ConsoleEntry {
    fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Instant::now(),
            level,
            message: message.into(),
            sticky: false,
        }
    }
}

#[derive(Debug)]
pub struct ConsolePane {
    entries: Vec<ConsoleEntry>,
    retain_duration: Duration,
    auto_scroll: bool,
    filter: String,
}

impl Default for ConsolePane {
    fn default() -> Self {
        let mut entries = Vec::new();
        entries.push(ConsoleEntry::new(LogLevel::Info, "Console ready"));
        entries.push(ConsoleEntry::new(
            LogLevel::Info,
            "Loaded demo project and connected engine",
        ));
        entries.push(ConsoleEntry::new(
            LogLevel::Warning,
            "MIDI device not detected",
        ));
        entries.push(ConsoleEntry::new(
            LogLevel::Info,
            "Hot reload enabled for scripts",
        ));
        entries.push(ConsoleEntry::new(
            LogLevel::Warning,
            "Missing impulse response - using fallback",
        ));
        Self {
            entries,
            retain_duration: Duration::from_secs(300),
            auto_scroll: true,
            filter: String::new(),
        }
    }
}

impl ConsolePane {
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.entries.push(ConsoleEntry::new(level, message));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, palette: &HarmoniqPalette, focus: &mut InputFocus) {
        self.prune_old_entries();
        let ctx = ui.ctx().clone();

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Console").color(palette.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut auto_scroll = self.auto_scroll;
                    if ui.checkbox(&mut auto_scroll, "Auto-scroll").changed() {
                        self.auto_scroll = auto_scroll;
                    }
                });
            });
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter console…")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);
            let filter_lower = self.filter.to_lowercase();
            egui::ScrollArea::vertical()
                .id_source("console_scroll")
                .stick_to_bottom(self.auto_scroll)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for entry in &self.entries {
                        if !filter_lower.is_empty()
                            && !entry.message.to_lowercase().contains(filter_lower.as_str())
                        {
                            continue;
                        }
                        let time_delta = entry.timestamp.elapsed().as_secs();
                        let label = format!(
                            "[{:<5}] {:>3}s  {}",
                            entry.level.label(),
                            time_delta,
                            entry.message
                        );
                        ui.label(
                            RichText::new(label)
                                .monospace()
                                .color(entry.level.color(palette)),
                        );
                    }
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.entries.clear();
                }
                if ui.button("Dump engine state").clicked() {
                    self.log(LogLevel::Info, "Engine state dumped to console/log.txt");
                }
            });
        });

        let used_rect = ui.min_rect();
        focus.track_pane_interaction(&ctx, used_rect, WorkspacePane::Console);
    }

    fn prune_old_entries(&mut self) {
        let retain = self.retain_duration;
        self.entries
            .retain(|entry| entry.sticky || entry.timestamp.elapsed() <= retain);
    }
}
