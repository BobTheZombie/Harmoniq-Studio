use egui::{
    self, Align, Color32, Frame, Id, Layout, Margin, PointerButton, Response, RichText, Rounding,
    Vec2,
};

use crate::state::{Channel, ChannelId, ChannelKind, PatternId, RackState, Step};
use crate::{convert, RackCallbacks, RackProps};

#[derive(Clone, Default)]
struct PaintSession {
    channel: ChannelId,
    value: bool,
}

#[derive(Clone, Default)]
struct PaintState {
    session: Option<PaintSession>,
}

fn paint_state_id() -> Id {
    Id::new("harmoniq_rack_paint_state")
}

pub fn rack_ui(ui: &mut egui::Ui, mut props: RackProps<'_>) {
    let RackProps { state, callbacks } = &mut props;

    pattern_header(ui, state);
    ui.add_space(8.0);
    add_channel_row(ui, state, callbacks);
    ui.add_space(12.0);

    let mut pending_remove = Vec::new();
    let mut pending_clone = Vec::new();
    let mut pending_replace = Vec::new();
    let mut pending_convert = Vec::new();

    let pattern_id = state.current_pattern;
    let paint_state_key = paint_state_id();
    let mut paint_state = ui
        .ctx()
        .memory(|mem| mem.data.get_temp::<PaintState>(paint_state_key))
        .unwrap_or_default();

    for index in 0..state.channels.len() {
        let pattern_bars = state
            .patterns
            .iter()
            .find(|pat| pat.id == pattern_id)
            .map(|pat| pat.bars)
            .unwrap_or(1);

        let (_, tail) = state.channels.split_at_mut(index);
        let channel = &mut tail[0];

        let frame = Frame::none()
            .fill(Color32::from_rgb(24, 24, 28))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(12.0, 10.0));

        let response = frame.show(ui, |ui| {
            ui.vertical(|ui| {
                channel_header(
                    ui,
                    channel,
                    callbacks,
                    pattern_id,
                    &mut pending_remove,
                    &mut pending_clone,
                    &mut pending_replace,
                    &mut pending_convert,
                );
                ui.add_space(6.0);
                channel_controls(ui, channel);
                ui.add_space(8.0);
                step_grid(ui, channel, pattern_id, pattern_bars, &mut paint_state);
            });
        });

        let row_response = response.response;
        if row_response.secondary_clicked() {
            callbacks.open_piano_roll.as_mut()(channel.id, state.current_pattern);
        }

        ui.add_space(10.0);
    }

    ui.ctx()
        .memory_mut(|mem| mem.data.insert_temp(paint_state_key, paint_state));

    if !pending_remove.is_empty() {
        state
            .channels
            .retain(|channel| !pending_remove.iter().any(|id| id == &channel.id));
    }

    for channel_id in pending_clone {
        if let Some(original) = state.channels.iter().find(|ch| ch.id == channel_id) {
            let mut clone = original.clone();
            let new_id = state.channels.iter().map(|ch| ch.id).max().unwrap_or(0) + 1;
            clone.id = new_id;
            state.channels.push(clone);
        }
    }

    for channel_id in pending_replace {
        callbacks.open_plugin_browser.as_mut()(channel_id);
    }

    for channel_id in pending_convert {
        let _ = convert::steps_to_midi(state, pattern_id, channel_id);
    }
}

fn pattern_header(ui: &mut egui::Ui, state: &mut RackState) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for pattern in &state.patterns {
                    let selected = pattern.id == state.current_pattern;
                    if ui
                        .add(egui::SelectableLabel::new(selected, pattern.name.clone()))
                        .clicked()
                    {
                        state.current_pattern = pattern.id;
                    }
                }

                if ui.button("+").clicked() {
                    let id = state.add_pattern();
                    state.current_pattern = id;
                }
            });
        });
}

fn add_channel_row(ui: &mut egui::Ui, state: &mut RackState, callbacks: &mut RackCallbacks) {
    egui::Frame::none()
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Add Instrument").clicked() {
                    let name = next_channel_name(state, "Instrument");
                    let id = state.add_channel(name, ChannelKind::Instrument, None);
                    callbacks.open_plugin_browser.as_mut()(id);
                }

                if ui.button("Add Sample").clicked() {
                    let name = next_channel_name(state, "Sample");
                    let id = state.add_channel(name, ChannelKind::Sample, None);
                    callbacks.import_sample_file.as_mut()(id);
                }

                if ui.button("Add Effect").clicked() {
                    let name = next_channel_name(state, "Effect");
                    let id = state.add_channel(name, ChannelKind::Effect, None);
                    callbacks.open_plugin_browser.as_mut()(id);
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn channel_header(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    callbacks: &mut RackCallbacks,
    pattern_id: PatternId,
    pending_remove: &mut Vec<ChannelId>,
    pending_clone: &mut Vec<ChannelId>,
    pending_replace: &mut Vec<ChannelId>,
    pending_convert: &mut Vec<ChannelId>,
) {
    ui.horizontal(|ui| {
        if ui.toggle_value(&mut channel.mute, "M").clicked() && channel.mute {
            channel.solo = false;
        }
        ui.toggle_value(&mut channel.solo, "S");

        ui.add(
            egui::TextEdit::singleline(&mut channel.name)
                .desired_width(160.0)
                .hint_text("Channel Name"),
        );

        let badge = channel
            .plugin_uid
            .as_deref()
            .map(|uid| uid.to_string())
            .unwrap_or_else(|| match channel.kind {
                ChannelKind::Instrument => "Select Instrument".to_string(),
                ChannelKind::Sample => "Load Sample".to_string(),
                ChannelKind::Effect => "Select Effect".to_string(),
            });
        if ui
            .button(RichText::new(badge).weak())
            .on_hover_text("Click to pick a plugin or sample")
            .clicked()
        {
            pending_replace.push(channel.id);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("⋮", |ui| {
                if ui.button("Edit in Piano Roll").clicked() {
                    callbacks.open_piano_roll.as_mut()(channel.id, pattern_id);
                    ui.close_menu();
                }
                if ui.button("Convert Steps → MIDI").clicked() {
                    pending_convert.push(channel.id);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Replace Instrument").clicked() {
                    pending_replace.push(channel.id);
                    ui.close_menu();
                }
                if ui.button("Clone Channel").clicked() {
                    pending_clone.push(channel.id);
                    ui.close_menu();
                }
                if ui.button("Delete Channel").clicked() {
                    pending_remove.push(channel.id);
                    ui.close_menu();
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Ghost notes (soon)"));
            });
        });
    });
}

fn channel_controls(ui: &mut egui::Ui, channel: &mut Channel) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut channel.gain_db, -60.0..=12.0).text("Level (dB)"));
        ui.add(
            egui::Slider::new(&mut channel.swing, 0.0..=1.0)
                .text("Swing")
                .suffix("%"),
        );

        let mut use_32 = channel.steps_per_bar == 32;
        if ui.toggle_value(&mut use_32, "32 Steps").changed() {
            channel.steps_per_bar = if use_32 { 32 } else { 16 };
        }
    });
}

fn step_grid(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    pattern_id: PatternId,
    bars: u32,
    paint_state: &mut PaintState,
) {
    let steps_per_bar_value = channel.steps_per_bar;
    let channel_id = channel.id;
    let steps_len = (bars * steps_per_bar_value) as usize;
    let steps = channel
        .steps
        .entry(pattern_id)
        .or_insert_with(|| vec![Step::default(); steps_len]);
    if steps.len() != steps_len {
        steps.resize(steps_len, Step::default());
    }

    let steps_per_bar = steps_per_bar_value as usize;
    let total_steps = steps.len();
    let alt_pressed = ui.input(|i| i.modifiers.alt);

    let spacing = Vec2::splat(4.0);
    let step_size = Vec2::new(22.0, 22.0);

    let pointer_down = ui.input(|i| i.pointer.button_down(PointerButton::Primary));
    if !pointer_down {
        paint_state.session = None;
    }

    egui::Grid::new(Id::new((channel_id, "step_grid")))
        .spacing(spacing)
        .min_col_width(step_size.x)
        .show(ui, |ui| {
            for bar in 0..bars {
                for step_in_bar in 0..steps_per_bar {
                    let step_index = bar as usize * steps_per_bar + step_in_bar;
                    if step_index >= total_steps {
                        break;
                    }
                    let step = &mut steps[step_index];
                    let text = if step.on { "●" } else { "○" };
                    let response = ui.add_sized(step_size, egui::Button::new(text));
                    handle_step_interaction(
                        ui,
                        channel_id,
                        step,
                        step_index,
                        response,
                        alt_pressed,
                        paint_state,
                    );

                    if (step_in_bar + 1) % 4 == 0 && step_in_bar + 1 != steps_per_bar {
                        ui.add_space(6.0);
                    }
                }
                ui.end_row();
            }
        });
}

fn handle_step_interaction(
    ui: &mut egui::Ui,
    channel_id: ChannelId,
    step: &mut Step,
    step_index: usize,
    response: Response,
    alt_pressed: bool,
    paint_state: &mut PaintState,
) {
    if response.clicked() {
        if alt_pressed {
            let new_value = !step.on;
            step.on = new_value;
            paint_state.session = Some(PaintSession {
                channel: channel_id,
                value: new_value,
            });
        } else {
            step.on = !step.on;
        }
    }

    if alt_pressed && response.hovered() {
        if let Some(session) = &paint_state.session {
            if session.channel == channel_id
                && ui.input(|i| i.pointer.button_down(PointerButton::Primary))
            {
                step.on = session.value;
            }
        }
    }

    response.context_menu(|ui| {
        ui.label(format!("Step {}", step_index + 1));
        ui.separator();
        ui.add(egui::Slider::new(&mut step.velocity, 0..=127).text("Velocity"));
        ui.add(egui::Slider::new(&mut step.pan, -64..=63).text("Pan"));
        ui.add(egui::Slider::new(&mut step.shift_ticks, -120..=120).text("Shift"));
        if ui.button("Clear Step").clicked() {
            *step = Step::default();
            ui.close_menu();
        }
    });
}

fn next_channel_name(state: &RackState, prefix: &str) -> String {
    let count = state
        .channels
        .iter()
        .filter(|ch| ch.name.starts_with(prefix))
        .count()
        + 1;
    format!("{} {}", prefix, count)
}
