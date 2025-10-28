use std::path::PathBuf;

use crossbeam_channel::Sender;
use eframe::egui::{self, Align, Color32, Label, RichText};

use crate::commands::Command;
use crate::state::session::{Channel, ChannelId, ChannelKind, PatternId, Session};

#[derive(Default)]
pub struct ChannelRackState {
    pub new_instrument_name: String,
    pub new_instrument_plugin: String,
    pub new_sample_name: String,
    pub new_sample_path: String,
}

pub fn render(
    ui: &mut egui::Ui,
    session: &mut Session,
    state: &mut ChannelRackState,
    tx: &Sender<Command>,
) {
    pattern_strip(ui, session, tx);
    ui.add_space(8.0);
    add_channel_row(ui, state, tx);
    ui.add_space(12.0);

    let current_pattern = session.current_pattern;
    if session.channels.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No channels yet. Add an instrument or sample to begin.").italics(),
            );
        });
        return;
    }

    let channel_snapshots: Vec<Channel> = session.channels.iter().cloned().collect();
    for channel in channel_snapshots {
        egui::Frame::none()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(ui.visuals().widgets.inactive.bg_stroke)
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                channel_header(ui, &channel, current_pattern, tx);
                ui.add_space(6.0);
                step_lane(ui, session, channel.id, current_pattern);
            });
        ui.add_space(8.0);
    }
}

fn pattern_strip(ui: &mut egui::Ui, session: &mut Session, tx: &Sender<Command>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Patterns").strong());
        ui.add_space(8.0);
        for pattern in &session.patterns {
            let selected = pattern.id == session.current_pattern;
            if ui
                .selectable_label(selected, pattern.name.clone())
                .on_hover_text("Select pattern")
                .clicked()
            {
                let _ = tx.send(Command::SelectPattern(pattern.id));
            }
        }
        if ui.button("+").on_hover_text("Add pattern").clicked() {
            let _ = tx.send(Command::AddPattern);
        }
    });
}

fn add_channel_row(ui: &mut egui::Ui, state: &mut ChannelRackState, tx: &Sender<Command>) {
    egui::Frame::none()
        .fill(ui.visuals().panel_fill.gamma_multiply(0.9))
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Add").strong());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Instrument");
                    ui.add_sized(
                        [140.0, 24.0],
                        egui::TextEdit::singleline(&mut state.new_instrument_name)
                            .hint_text("Name"),
                    );
                    ui.add_sized(
                        [160.0, 24.0],
                        egui::TextEdit::singleline(&mut state.new_instrument_plugin)
                            .hint_text("Plugin UID"),
                    );
                    if ui.button("Add").clicked() {
                        let name = state
                            .new_instrument_name
                            .trim()
                            .to_string()
                            .if_empty(|| "Instrument".into());
                        let plugin_uid = state.new_instrument_plugin.trim().to_string();
                        let plugin_uid = if plugin_uid.is_empty() {
                            "internal://testsynth".to_string()
                        } else {
                            plugin_uid
                        };
                        let _ = tx.send(Command::AddChannelInstrument {
                            name,
                            plugin_uid: plugin_uid.clone(),
                        });
                        state.new_instrument_name.clear();
                        state.new_instrument_plugin = plugin_uid;
                        // TODO: real plugin browser integration should call:
                        // tx.send(Command::AddChannelInstrument { name: selected_plugin.name.clone(), plugin_uid: selected_plugin.unique_id.clone() });
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Sample");
                    ui.add_sized(
                        [140.0, 24.0],
                        egui::TextEdit::singleline(&mut state.new_sample_name).hint_text("Name"),
                    );
                    ui.add_sized(
                        [160.0, 24.0],
                        egui::TextEdit::singleline(&mut state.new_sample_path).hint_text("Path"),
                    );
                    if ui.button("Add").clicked() {
                        let name = state
                            .new_sample_name
                            .trim()
                            .to_string()
                            .if_empty(|| "Sample".into());
                        let path = state.new_sample_path.trim().to_string();
                        let path_buf = if path.is_empty() {
                            PathBuf::from("sample.wav")
                        } else {
                            PathBuf::from(path)
                        };
                        let _ = tx.send(Command::AddChannelSample {
                            name,
                            path: path_buf,
                        });
                        state.new_sample_name.clear();
                        state.new_sample_path.clear();
                    }
                });
            });
        });
}

fn channel_header(
    ui: &mut egui::Ui,
    channel: &Channel,
    pattern_id: PatternId,
    tx: &Sender<Command>,
) {
    ui.horizontal(|ui| {
        let mut mute_state = channel.mute;
        let mute_button = ui.toggle_value(&mut mute_state, "M");
        if mute_button.clicked() {
            let _ = tx.send(Command::ToggleChannelMute(channel.id, !channel.mute));
        }
        let mut solo_state = channel.solo;
        let solo_button = ui.toggle_value(&mut solo_state, "S");
        if solo_button.clicked() {
            let _ = tx.send(Command::ToggleChannelSolo(channel.id, !channel.solo));
        }

        let label_text = match channel.kind {
            ChannelKind::Instrument => channel.name.clone(),
            ChannelKind::Sample => format!("{} (Sample)", channel.name),
            ChannelKind::Effect => format!("{} (FX)", channel.name),
        };
        let label = Label::new(RichText::new(label_text).strong());
        let response = ui.add(label);
        response.context_menu(|ui| {
            if ui.button("Edit in Piano Roll").clicked() {
                let _ = tx.send(Command::OpenPianoRoll {
                    channel_id: channel.id,
                    pattern_id,
                });
                ui.close_menu();
            }
            if ui.button("Convert Steps → MIDI Clip").clicked() {
                let _ = tx.send(Command::ConvertStepsToMidi {
                    channel_id: channel.id,
                    pattern_id,
                });
                ui.close_menu();
            }
            if ui.button("Clone").clicked() {
                let _ = tx.send(Command::CloneChannel(channel.id));
                ui.close_menu();
            }
            if ui.button("Delete").clicked() {
                let _ = tx.send(Command::RemoveChannel(channel.id));
                ui.close_menu();
            }
        });

        if let Some(plugin) = &channel.target_plugin_uid {
            let badge = format!("Plugin: {plugin}");
            ui.label(RichText::new(badge).color(Color32::from_gray(180)).small());
        }

        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("⋮", |ui| {
                if ui.button("Edit in Piano Roll").clicked() {
                    let _ = tx.send(Command::OpenPianoRoll {
                        channel_id: channel.id,
                        pattern_id,
                    });
                    ui.close_menu();
                }
                if ui.button("Convert Steps → MIDI Clip").clicked() {
                    let _ = tx.send(Command::ConvertStepsToMidi {
                        channel_id: channel.id,
                        pattern_id,
                    });
                    ui.close_menu();
                }
                if ui.button("Clone").clicked() {
                    let _ = tx.send(Command::CloneChannel(channel.id));
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    let _ = tx.send(Command::RemoveChannel(channel.id));
                    ui.close_menu();
                }
            });
        });
    });
}

fn step_lane(
    ui: &mut egui::Ui,
    session: &mut Session,
    channel_id: ChannelId,
    pattern_id: PatternId,
) {
    let steps = session.ensure_steps(pattern_id, channel_id);
    let accent = ui.visuals().selection.bg_fill;
    let inactive = ui.visuals().widgets.inactive.bg_fill;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (idx, step) in steps.iter_mut().enumerate() {
            let is_accent = idx % 4 == 0;
            let fill = if *step { accent } else { inactive };
            let mut frame = egui::Frame::none()
                .fill(fill)
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::same(4.0));
            if is_accent {
                frame = frame.stroke(egui::Stroke::new(1.2, accent));
            }
            frame.show(ui, |ui| {
                let text = if *step { "●" } else { "○" };
                let response = ui
                    .add_sized([18.0, 18.0], egui::Button::new(text).frame(false))
                    .on_hover_text(format!("Step {}", idx + 1));
                if response.clicked() {
                    *step = !*step;
                }
            });
        }
    });
}

trait IfEmpty {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}
