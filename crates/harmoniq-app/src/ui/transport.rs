use eframe::egui::{self, RichText};
use harmoniq_engine::TransportState;
use harmoniq_ui::HarmoniqPalette;

use crate::ui::event_bus::{AppEvent, EventBus, TransportEvent};
use crate::{AppIcons, TimeSignature, TransportClock};

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
            if ui
                .toggle_value(
                    &mut record_enabled,
                    RichText::new("Rec").color(if record_enabled {
                        palette.accent
                    } else {
                        palette.text_primary
                    }),
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
            if ui
                .toggle_value(
                    &mut pattern_mode,
                    RichText::new(if pattern_mode { "Pattern" } else { "Song" })
                        .color(palette.text_muted),
                )
                .clicked()
            {
                event_bus.publish(AppEvent::TogglePatternMode);
            }

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
}
