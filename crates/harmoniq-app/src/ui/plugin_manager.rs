use std::collections::VecDeque;
use std::sync::Arc;

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use crate::core::{
    plugin_registry::PluginRegistry,
    plugin_scanner::{PluginDescriptor, PluginScanner, ScanHandle, ScanState, ScanStatus},
};
use crate::plugin_host;

/// High-level event emitted by the plugin manager UI for the host application.
#[derive(Debug, Clone)]
pub struct PluginManagerFeedback {
    pub message: String,
    pub kind: FeedbackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    Info,
    Error,
}

pub struct PluginManagerPanel {
    registry: Arc<PluginRegistry>,
    scanner: PluginScanner,
    plugins: Vec<PluginDescriptor>,
    filter: String,
    scan_handle: Option<ScanHandle>,
    progress: Option<ScanStatus>,
    feedback_banner: Option<PluginManagerFeedback>,
    pending_feedback: VecDeque<PluginManagerFeedback>,
}

impl PluginManagerPanel {
    pub fn new(registry: Arc<PluginRegistry>, scanner: PluginScanner) -> Self {
        let mut plugins = scanner.cached_plugins();
        plugins.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        Self {
            registry,
            scanner,
            plugins,
            filter: String::new(),
            scan_handle: None,
            progress: None,
            feedback_banner: None,
            pending_feedback: VecDeque::new(),
        }
    }

    /// Polls background jobs for completion and updates UI state.
    pub fn tick(&mut self) {
        if let Some(handle) = &self.scan_handle {
            match handle.snapshot() {
                ScanState::Scanning(status) => {
                    self.progress = Some(status);
                }
                ScanState::Completed { plugins, errors } => {
                    self.plugins = plugins;
                    // Refresh from the registry so hot-reloads pick up persisted state.
                    self.plugins = self.registry.plugins();
                    self.scan_handle = None;
                    self.progress = None;
                    if errors.is_empty() {
                        self.push_feedback(
                            "Plugin scan completed successfully",
                            FeedbackKind::Info,
                        );
                    } else {
                        self.push_feedback(
                            format!("Plugin scan completed with {} warning(s)", errors.len()),
                            FeedbackKind::Error,
                        );
                        for error in errors {
                            tracing::warn!("plugin scan warning: {error}");
                        }
                    }
                }
                ScanState::Failed(err) => {
                    self.scan_handle = None;
                    self.progress = None;
                    self.push_feedback(format!("Plugin scan failed: {err}"), FeedbackKind::Error);
                }
                ScanState::Idle => {
                    self.progress = None;
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, open: &mut bool) {
        egui::Window::new("Plugin Manager")
            .open(open)
            .resizable(true)
            .default_width(720.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                self.render(ui);
            });
    }

    pub fn take_feedback(&mut self) -> Option<PluginManagerFeedback> {
        self.pending_feedback.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search");
            let response = ui.text_edit_singleline(&mut self.filter);
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ui.memory_mut(|mem| mem.request_focus(response.id));
            }
            ui.add_space(8.0);
            let rescan_enabled = self.scan_handle.is_none();
            if ui
                .add_enabled(rescan_enabled, egui::Button::new("Rescan"))
                .clicked()
            {
                self.request_scan();
            }
        });
        ui.separator();

        if let Some(progress) = &self.progress {
            ui.horizontal(|ui| {
                ui.add(egui::widgets::Spinner::new());
                ui.label(format!(
                    "{} ({} of {} folders)",
                    progress.message,
                    progress.completed.min(progress.total),
                    progress.total
                ));
            });
            ui.separator();
        }

        if let Some(banner) = &self.feedback_banner {
            let text = RichText::new(&banner.message).strong();
            let color = match banner.kind {
                FeedbackKind::Info => ui.visuals().hyperlink_color,
                FeedbackKind::Error => ui.visuals().error_fg_color,
            };
            ui.colored_label(color, text);
            ui.separator();
        }

        let filter = self.filter.to_ascii_lowercase();
        let filtered: Vec<_> = self
            .plugins
            .iter()
            .filter(|plugin| plugin.name.to_ascii_lowercase().contains(&filter))
            .cloned()
            .collect();

        if filtered.is_empty() {
            ui.label(RichText::new("No plugins found").italics());
            return;
        }

        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(180.0).resizable(true))
            .column(Column::initial(70.0))
            .column(Column::initial(140.0).resizable(true))
            .column(Column::initial(80.0))
            .column(Column::remainder().resizable(true))
            .column(Column::initial(70.0))
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Name");
                });
                header.col(|ui| {
                    ui.strong("Type");
                });
                header.col(|ui| {
                    ui.strong("Vendor");
                });
                header.col(|ui| {
                    ui.strong("Version");
                });
                header.col(|ui| {
                    ui.strong("Path");
                });
                header.col(|ui| {
                    ui.strong("Actions");
                });
            })
            .body(|mut body| {
                for plugin in filtered {
                    body.row(26.0, |mut row| {
                        row.col(|ui| {
                            ui.label(&plugin.name);
                        });
                        row.col(|ui| {
                            ui.label(plugin.format.label());
                        });
                        row.col(|ui| {
                            ui.label(plugin.vendor.as_deref().unwrap_or("Unknown"));
                        });
                        row.col(|ui| {
                            ui.label(plugin.version.as_deref().unwrap_or("Unknown"));
                        });
                        row.col(|ui| {
                            ui.monospace(plugin.path.display().to_string());
                        });
                        row.col(|ui| {
                            if ui.button("Load").clicked() {
                                self.load_plugin(&plugin);
                            }
                        });
                    });
                }
            });
    }

    pub fn request_scan(&mut self) {
        self.feedback_banner = None;
        self.progress = None;
        let handle = self.scanner.scan_async();
        self.scan_handle = Some(handle);
    }

    fn load_plugin(&mut self, plugin: &PluginDescriptor) {
        match plugin_host::load_plugin(plugin.path.as_path()) {
            Ok(_) => {
                self.push_feedback(format!("Loaded plugin {}", plugin.name), FeedbackKind::Info);
            }
            Err(err) => {
                self.push_feedback(
                    format!("Failed to load {}: {err}", plugin.name),
                    FeedbackKind::Error,
                );
            }
        }
    }

    fn push_feedback(&mut self, message: impl Into<String>, kind: FeedbackKind) {
        let feedback = PluginManagerFeedback {
            message: message.into(),
            kind,
        };
        self.feedback_banner = Some(feedback.clone());
        self.pending_feedback.push_back(feedback);
    }
}
