use crossbeam_channel::Sender;
use eframe::egui::{self, Align};

use crate::commands::Command;
use crate::state::session::{ChannelId, ChannelKind, PatternId, Session};

pub struct ChannelRackProps<'a> {
    pub session: &'a mut Session,
    pub selected_pattern: PatternId,
    pub command_tx: Sender<Command>,
}

pub fn channel_rack_ui(ui: &mut egui::Ui, props: ChannelRackProps<'_>) {
    ui.heading("Channel Rack");
    ui.add_space(8.0);

    pattern_picker(
        ui,
        &*props.session,
        props.selected_pattern,
        &props.command_tx,
    );
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        ui.label("Add:");
        if ui.button("Instrument").clicked() {
            // When the plugin browser is wired up, hook the selection callback here and call:
            // let _ = command_tx.send(Command::AddChannelInstrument { name, plugin_uid });
            let name = format!("Instrument {}", props.session.channels.len() + 1);
            let _ = props.command_tx.send(Command::AddChannelInstrument {
                name,
                plugin_uid: None,
            });
        }
        if ui.button("Sample").clicked() {
            let name = format!("Sample {}", props.session.channels.len() + 1);
            let _ = props.command_tx.send(Command::AddChannelSample {
                name,
                path: String::new(),
            });
        }
    });

    ui.add_space(12.0);

    let total_steps = props
        .session
        .pattern(props.selected_pattern)
        .map(|pattern| pattern.total_16th_steps())
        .unwrap_or(16);

    for index in 0..props.session.channels.len() {
        let channel_id = props.session.channels[index].id;
        props
            .session
            .ensure_steps(channel_id, props.selected_pattern);

        let channel_kind = props.session.channels[index].kind;
        let channel_name = props.session.channels[index].name.clone();
        let plugin_badge = props.session.channels[index]
            .target_plugin_uid
            .clone()
            .unwrap_or_default();
        let mute_state = props.session.channels[index].mute;
        let solo_state = props.session.channels[index].solo;

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(channel_name.clone());
                if !plugin_badge.is_empty() {
                    ui.weak(format!("plugin: {plugin_badge}"));
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("⋮", |ui| {
                        if ui.button("Edit in Piano Roll").clicked() {
                            let _ = props.command_tx.send(Command::OpenPianoRoll {
                                channel_id,
                                pattern_id: props.selected_pattern,
                            });
                            ui.close_menu();
                        }
                        if ui.button("Convert Steps → MIDI Clip").clicked() {
                            let _ = props.command_tx.send(Command::ConvertStepsToMidi {
                                channel_id,
                                pattern_id: props.selected_pattern,
                            });
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Clone Channel").clicked() {
                            let _ = props.command_tx.send(Command::CloneChannel(channel_id));
                            ui.close_menu();
                        }
                        if ui.button("Delete Channel").clicked() {
                            let _ = props.command_tx.send(Command::RemoveChannel(channel_id));
                            ui.close_menu();
                        }
                    });

                    let mut mute = mute_state;
                    if ui.toggle_value(&mut mute, "M").clicked() {
                        let _ = props
                            .command_tx
                            .send(Command::ToggleChannelMute(channel_id, mute));
                        if let Some(channel) = props.session.channel_mut(channel_id) {
                            channel.mute = mute;
                        }
                    }

                    let mut solo = solo_state;
                    if ui.toggle_value(&mut solo, "S").clicked() {
                        let _ = props
                            .command_tx
                            .send(Command::ToggleChannelSolo(channel_id, solo));
                        if let Some(channel) = props.session.channel_mut(channel_id) {
                            channel.solo = solo;
                        }
                    }
                });
            });

            ui.add_space(6.0);

            if channel_kind != ChannelKind::Effect {
                step_lane(
                    ui,
                    channel_id,
                    props.selected_pattern,
                    total_steps,
                    props.session,
                );
            } else {
                ui.label("Effects do not have step lanes.");
            }
        });

        ui.add_space(6.0);
    }
}

fn pattern_picker(
    ui: &mut egui::Ui,
    session: &Session,
    selected_pattern: PatternId,
    command_tx: &Sender<Command>,
) {
    ui.horizontal(|ui| {
        ui.label("Patterns:");
        for pattern in &session.patterns {
            let button = ui.selectable_label(pattern.id == selected_pattern, &pattern.name);
            if button.clicked() {
                let _ = command_tx.send(Command::SelectPattern(pattern.id));
            }
        }
        if ui.button("+").clicked() {
            let _ = command_tx.send(Command::AddPattern);
        }
    });
}

fn step_lane(
    ui: &mut egui::Ui,
    channel_id: ChannelId,
    pattern_id: PatternId,
    total_steps: usize,
    session: &mut Session,
) {
    if let Some(channel) = session.channel_mut(channel_id) {
        if let Some(steps) = channel.steps.get_mut(&pattern_id) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (index, step) in steps.iter_mut().enumerate().take(total_steps) {
                    let active = *step;
                    let label = if active { "●" } else { "○" };
                    if ui
                        .add_sized([20.0, 20.0], egui::Button::new(label))
                        .clicked()
                    {
                        *step = !*step;
                    }

                    if (index + 1) % channel.steps_per_bar == 0 {
                        ui.add_space(8.0);
                    }
                }
            });
        }
    }
}
