use eframe::egui::{self, RichText};
use harmoniq_engine::TransportState;
use harmoniq_ui::HarmoniqPalette;

use crate::ui::command_dispatch::CommandSender;
use crate::ui::commands::{Command, ViewCommand};
use crate::ui::event_bus::{AppEvent, EventBus, TransportEvent};
use crate::{AppIcons, TimeSignature, TransportClock};

/// Push the cursor so that the *next* widget starts at the horizontal midline of the transport bar.
/// This keeps placement stable across window resizes and regardless of what was drawn before it.
fn push_to_toolbar_midline(ui: &mut egui::Ui) {
    let cursor_x = ui.cursor().min.x;
    let target_x = ui.max_rect().center().x;
    if target_x > cursor_x {
        ui.add_space(target_x - cursor_x);
    }
}

fn toolbar_toggle_button(
    ui: &mut egui::Ui,
    icon: &egui::TextureHandle,
    palette: &HarmoniqPalette,
    active: bool,
    tooltip: &str,
) -> bool {
    let size = egui::vec2(28.0, 28.0);
    let tint = if active {
        palette.accent
    } else {
        palette.text_muted
    };
    let response = ui
        .add(
            egui::ImageButton::new((icon.id(), size))
                .frame(false)
                .tint(tint),
        )
        .on_hover_text(tooltip);
    if active {
        let highlight = response.rect.expand(4.0);
        ui.painter().rect(
            highlight,
            6.0,
            palette.toolbar_highlight,
            egui::Stroke::new(1.0, palette.accent),
        );
    }
    response.clicked()
}

fn mixer_toggle_button(
    ui: &mut egui::Ui,
    palette: &HarmoniqPalette,
    icons: &AppIcons,
    commands: &CommandSender,
    mixer_visible: bool,
) {
    if toolbar_toggle_button(
        ui,
        &icons.mixer,
        palette,
        mixer_visible,
        "Show/Hide Mixer (F9)",
    ) {
        let _ = commands.try_send(Command::View(ViewCommand::ToggleMixer));
    }
}

fn playlist_toggle_button(
    ui: &mut egui::Ui,
    palette: &HarmoniqPalette,
    icons: &AppIcons,
    commands: &CommandSender,
    playlist_visible: bool,
) {
    if toolbar_toggle_button(
        ui,
        &icons.playlist,
        palette,
        playlist_visible,
        "Show/Hide Playlist (F5)",
    ) {
        let _ = commands.try_send(Command::View(ViewCommand::TogglePlaylist));
    }
}

fn piano_roll_toggle_button(
    ui: &mut egui::Ui,
    palette: &HarmoniqPalette,
    icons: &AppIcons,
    commands: &CommandSender,
    piano_roll_visible: bool,
) {
    if toolbar_toggle_button(
        ui,
        &icons.piano_roll,
        palette,
        piano_roll_visible,
        "Show/Hide Piano Roll (F7)",
    ) {
        let _ = commands.try_send(Command::View(ViewCommand::TogglePianoRoll));
    }
}

#[derive(Default)]
pub struct TransportBar;

impl TransportBar {
    pub fn new(_tempo: f32, _signature: TimeSignature) -> Self {
        Self
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &HarmoniqPalette,
        icons: &AppIcons,
        event_bus: &EventBus,
        commands: &CommandSender,
        snapshot: TransportSnapshot,
    ) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);

            let play_icon = if matches!(
                snapshot.transport,
                TransportState::Playing | TransportState::Recording
            ) {
                &icons.pause
            } else {
                &icons.play
            };
            if ui
                .add(
                    egui::ImageButton::new((play_icon.id(), egui::vec2(30.0, 30.0)))
                        .frame(false)
                        .tint(palette.text_primary),
                )
                .clicked()
            {
                if matches!(
                    snapshot.transport,
                    TransportState::Playing | TransportState::Recording
                ) {
                    event_bus.publish(AppEvent::Transport(TransportEvent::Stop));
                } else {
                    event_bus.publish(AppEvent::Transport(TransportEvent::Play));
                }
            }

            if ui
                .add(
                    egui::ImageButton::new((icons.stop.id(), egui::vec2(30.0, 30.0)))
                        .frame(false)
                        .tint(palette.text_primary),
                )
                .clicked()
            {
                event_bus.publish(AppEvent::Transport(TransportEvent::Stop));
            }

            let mut record_enabled = matches!(snapshot.transport, TransportState::Recording);
            let record_color = if record_enabled {
                palette.accent
            } else {
                palette.text_primary
            };
            if ui
                .toggle_value(
                    &mut record_enabled,
                    RichText::new("Rec").color(record_color),
                )
                .clicked()
            {
                event_bus.publish(AppEvent::Transport(TransportEvent::Record(record_enabled)));
            }

            ui.separator();

            let mut tempo = snapshot.tempo;
            let tempo_resp = ui.add(
                egui::DragValue::new(&mut tempo)
                    .speed(0.25)
                    .clamp_range(20.0..=400.0)
                    .suffix(" BPM"),
            );
            if tempo_resp.changed() {
                event_bus.publish(AppEvent::SetTempo(tempo));
            }

            let mut signature = snapshot.time_signature;
            let mut numerator = signature.numerator as i32;
            let mut denominator = signature.denominator as i32;
            let num_resp = ui.add(
                egui::DragValue::new(&mut numerator)
                    .speed(0.2)
                    .clamp_range(1..=16),
            );
            let den_resp = ui.add(
                egui::DragValue::new(&mut denominator)
                    .speed(0.2)
                    .clamp_range(1..=16),
            );
            if num_resp.changed() || den_resp.changed() {
                signature.set_from_tuple((numerator as u32, denominator as u32));
                event_bus.publish(AppEvent::SetTimeSignature(signature));
            }

            let mut metronome = snapshot.metronome;
            if ui
                .toggle_value(
                    &mut metronome,
                    RichText::new("Metronome").color(palette.text_muted),
                )
                .clicked()
            {
                event_bus.publish(AppEvent::ToggleMetronome);
            }

            let mut pattern_mode = snapshot.pattern_mode;
            let pattern_label = if pattern_mode { "Pattern" } else { "Song" };
            if ui
                .toggle_value(
                    &mut pattern_mode,
                    RichText::new(pattern_label).color(palette.text_muted),
                )
                .clicked()
            {
                event_bus.publish(AppEvent::TogglePatternMode);
            }

            push_to_toolbar_midline(ui);
            mixer_toggle_button(ui, palette, icons, commands, snapshot.mixer_visible);
            ui.add_space(6.0);
            playlist_toggle_button(ui, palette, icons, commands, snapshot.playlist_visible);
            ui.add_space(6.0);
            piano_roll_toggle_button(ui, palette, icons, commands, snapshot.piano_roll_visible);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(snapshot.clock.format())
                        .color(palette.text_primary)
                        .monospace()
                        .size(18.0),
                );
            });
        });
    }
}

pub struct TransportSnapshot {
    pub tempo: f32,
    pub time_signature: TimeSignature,
    pub transport: TransportState,
    pub clock: TransportClock,
    pub metronome: bool,
    pub pattern_mode: bool,
    pub mixer_visible: bool,
    pub playlist_visible: bool,
    pub piano_roll_visible: bool,
}
