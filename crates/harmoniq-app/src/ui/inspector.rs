use eframe::egui::{self, Color32, RichText};
use harmoniq_ui::HarmoniqPalette;

use crate::ui::channel_rack::ChannelRackPane;
use crate::ui::event_bus::EventBus;
use crate::ui::focus::InputFocus;
use crate::ui::workspace::WorkspacePane;
use harmoniq_playlist::state::SelectedClip;

#[derive(Default)]
pub struct InspectorPane {
    playlist_selection: Option<PlaylistClipDetails>,
}

#[derive(Debug, Clone)]
struct PlaylistClipDetails {
    track_name: String,
    clip_name: String,
    start_beats: f32,
    duration_beats: f32,
    color: Color32,
}

impl InspectorPane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync_playlist_selection(&mut self, selection: Option<SelectedClip>, ppq: u32) {
        let ppq = ppq.max(1) as f32;
        self.playlist_selection = selection.map(|clip| PlaylistClipDetails {
            track_name: clip.track_name,
            clip_name: clip.clip_name,
            start_beats: clip.start_ticks as f32 / ppq,
            duration_beats: clip.duration_ticks as f32 / ppq,
            color: Color32::from_rgba_unmultiplied(
                (clip.color[0] * 255.0) as u8,
                (clip.color[1] * 255.0) as u8,
                (clip.color[2] * 255.0) as u8,
                (clip.color[3] * 255.0) as u8,
            ),
        });
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        focus: &mut InputFocus,
        event_bus: &EventBus,
        channel_rack: &mut ChannelRackPane,
    ) {
        let ctx = ui.ctx().clone();

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Inspector").color(palette.text_primary));
            });
            ui.add_space(8.0);

            if let Some(selection) = self.playlist_selection.as_ref() {
                ui.label(
                    RichText::new(format!("Track: {}", selection.track_name))
                        .color(palette.text_muted),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Clip").color(palette.text_muted).strong());
                    ui.add_space(6.0);
                    ui.label(RichText::new(&selection.clip_name).color(palette.text_primary));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Start").color(palette.text_muted));
                    ui.label(
                        RichText::new(format!("{:.2} beats", selection.start_beats))
                            .color(palette.text_primary),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Length").color(palette.text_muted));
                    ui.label(
                        RichText::new(format!("{:.2} beats", selection.duration_beats))
                            .color(palette.text_primary),
                    );
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Color").color(palette.text_muted));
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, selection.color);
                });
            } else {
                ui.label(
                    RichText::new("Select a clip in the playlist to view its details.")
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
    }
}
