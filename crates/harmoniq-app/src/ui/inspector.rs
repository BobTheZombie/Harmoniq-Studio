use eframe::egui::{self, RichText};
use harmoniq_ui::HarmoniqPalette;

use crate::ui::channel_rack::ChannelRackPane;
use crate::ui::event_bus::EventBus;
use crate::ui::focus::InputFocus;
use crate::ui::playlist::ClipSelection;
use crate::ui::workspace::WorkspacePane;

#[derive(Debug, Clone)]
pub enum InspectorCommand {
    RenameClip {
        track_index: usize,
        clip_index: usize,
        name: String,
    },
    UpdateClipRange {
        track_index: usize,
        clip_index: usize,
        start: f32,
        length: f32,
    },
    DeleteClip {
        track_index: usize,
        clip_index: usize,
    },
    DuplicateClip {
        track_index: usize,
        clip_index: usize,
    },
}

#[derive(Default)]
pub struct InspectorPane {
    selection: Option<ClipSelection>,
    keep_in_sync: bool,
}

impl InspectorPane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync_selection(&mut self, selection: Option<ClipSelection>) {
        if self.keep_in_sync {
            self.selection = selection;
        } else if self.selection.is_none() {
            self.selection = selection;
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        focus: &mut InputFocus,
        event_bus: &EventBus,
        channel_rack: &mut ChannelRackPane,
    ) -> Vec<InspectorCommand> {
        let ctx = ui.ctx().clone();
        let mut commands = Vec::new();

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Inspector").color(palette.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.keep_in_sync, "Follow selection");
                });
            });
            ui.add_space(8.0);

            if let Some(selection) = &mut self.selection {
                ui.label(
                    RichText::new(format!("Track: {}", selection.track_name))
                        .color(palette.text_muted),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Clip name").color(palette.text_muted));
                    let mut clip_name = selection.clip_name.clone();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut clip_name)
                                .desired_width(160.0)
                                .clip_text(false),
                        )
                        .lost_focus()
                        && clip_name != selection.clip_name
                    {
                        commands.push(InspectorCommand::RenameClip {
                            track_index: selection.track_index,
                            clip_index: selection.clip_index,
                            name: clip_name.clone(),
                        });
                        selection.clip_name = clip_name;
                    }
                });
                ui.add_space(6.0);
                let mut start = selection.start;
                let mut length = selection.length;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Start (beats)").color(palette.text_muted));
                    if ui
                        .add(
                            egui::DragValue::new(&mut start)
                                .clamp_range(0.0..=512.0)
                                .speed(0.1),
                        )
                        .changed()
                    {
                        commands.push(InspectorCommand::UpdateClipRange {
                            track_index: selection.track_index,
                            clip_index: selection.clip_index,
                            start,
                            length: selection.length,
                        });
                        selection.start = start;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Length (beats)").color(palette.text_muted));
                    if ui
                        .add(
                            egui::DragValue::new(&mut length)
                                .clamp_range(0.125..=512.0)
                                .speed(0.1),
                        )
                        .changed()
                    {
                        commands.push(InspectorCommand::UpdateClipRange {
                            track_index: selection.track_index,
                            clip_index: selection.clip_index,
                            start: selection.start,
                            length,
                        });
                        selection.length = length;
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Duplicate").clicked() {
                        commands.push(InspectorCommand::DuplicateClip {
                            track_index: selection.track_index,
                            clip_index: selection.clip_index,
                        });
                        selection.start += selection.length;
                    }
                    if ui.button("Delete").clicked() {
                        commands.push(InspectorCommand::DeleteClip {
                            track_index: selection.track_index,
                            clip_index: selection.clip_index,
                        });
                        self.selection = None;
                    }
                });
            } else {
                ui.label(
                    RichText::new("Select a clip in the arranger to edit its properties.")
                        .color(palette.text_muted),
                );
            }
        });

        ui.add_space(12.0);
        egui::CollapsingHeader::new("Channel Rack")
            .default_open(false)
            .show(ui, |ui| {
                channel_rack.ui(ui, palette, event_bus);
            });

        let used_rect = ui.min_rect();
        focus.track_pane_interaction(&ctx, used_rect, WorkspacePane::Inspector);
        commands
    }
}
