use crate::convert::steps_to_midi;
use crate::state::{Channel, ChannelId, ChannelKind, PatternId, RackState, Step};
use crate::{RackCallbacks, RackProps};
use egui::{self, Align2, Id, RichText, Sense, Stroke};

#[derive(Clone, Copy, Debug)]
struct ChannelDragState {
    channel: ChannelId,
    insert_at: usize,
}

fn channel_drag_id() -> Id {
    Id::new("rack_channel_reorder")
}

fn mixer_assign_drag_id() -> Id {
    Id::new("rack_mixer_assign")
}

pub fn render(ui: &mut egui::Ui, mut props: RackProps) {
    let RackProps { state, callbacks } = &mut props;

    pattern_strip(ui, state);
    ui.separator();

    add_row(ui, callbacks);
    ui.add_space(6.0);

    drop_hint(ui);

    let pattern_id = state.current_pattern;
    let mut pending_remove: Vec<ChannelId> = Vec::new();
    let mut pending_convert: Vec<(ChannelId, PatternId)> = Vec::new();
    let mut pending_move: Option<(ChannelId, usize)> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut index = 0usize;
            let total_channels = state.channels.len();
            while index < total_channels {
                let (_, tail) = state.channels.split_at_mut(index);
                let channel = &mut tail[0];
                channel_row(
                    ui,
                    pattern_id,
                    channel,
                    index,
                    total_channels,
                    callbacks,
                    &mut pending_remove,
                    &mut pending_convert,
                    &mut pending_move,
                );
                ui.separator();
                index += 1;
            }
        });

    if !pending_remove.is_empty() {
        state
            .channels
            .retain(|ch| !pending_remove.iter().any(|id| id == &ch.id));
    }

    for (channel_id, pat) in pending_convert {
        let _ = steps_to_midi(state, pat, channel_id);
    }

    if let Some((channel_id, insert_at)) = pending_move {
        if let Some(from_idx) = state.channels.iter().position(|c| c.id == channel_id) {
            let mut target = insert_at.min(state.channels.len());
            let channel = state.channels.remove(from_idx);
            if target > from_idx {
                target = target.saturating_sub(1);
            }
            if target != from_idx {
                state.channels.insert(target, channel);
                (callbacks.reorder_channels)(state.channels.iter().map(|c| c.id).collect());
            } else {
                state.channels.insert(from_idx, channel);
            }
        }
    }
}

fn pattern_strip(ui: &mut egui::Ui, state: &mut RackState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Pattern:").strong());
        if ui.button("◀").clicked() {
            state.select_previous_pattern();
        }

        let mut selected_pattern = state.current_pattern;
        let selected_label = state
            .patterns
            .iter()
            .find(|p| p.id == selected_pattern)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "No Pattern".to_string());

        egui::ComboBox::from_id_source("rack_pattern_selector")
            .selected_text(selected_label)
            .width(160.0)
            .show_ui(ui, |ui| {
                for pat in &state.patterns {
                    ui.selectable_value(&mut selected_pattern, pat.id, pat.name.clone());
                }
            });

        if selected_pattern != state.current_pattern {
            state.select_pattern(selected_pattern);
        }

        if ui.button("▶").clicked() {
            state.select_next_pattern();
        }

        if ui
            .button("Clone")
            .on_hover_text("Duplicate the current pattern")
            .clicked()
        {
            let _ = state.clone_pattern(state.current_pattern);
        }

        if ui.button("+").on_hover_text("Add Pattern").clicked() {
            state.add_pattern();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("Channel Rack").monospace());
        });
    });
}

fn add_row(ui: &mut egui::Ui, callbacks: &mut RackCallbacks) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Add:").strong());
        if ui.button("Instrument").clicked() {
            (callbacks.open_plugin_browser)(None);
        }
        if ui.button("Sample").clicked() {
            (callbacks.import_sample_file)(None, None);
        }
        if ui.button("Automation").clicked() {
            (callbacks.create_automation_for)("master");
        }
    });
}

fn drop_hint(ui: &mut egui::Ui) {
    ui.scope(|ui| {
        let rect = ui.available_rect_before_wrap();
        let is_hover = ui.rect_contains_pointer(rect);
        if is_hover && ui.input(|i| !i.raw.dropped_files.is_empty()) {
            let stroke = ui.visuals().selection.stroke.color;
            ui.painter().rect_stroke(rect, 6.0, (1.0, stroke));
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Release to import",
                egui::TextStyle::Heading.resolve(ui.style()),
                stroke,
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn channel_row(
    ui: &mut egui::Ui,
    pat: PatternId,
    ch: &mut Channel,
    index: usize,
    total_channels: usize,
    callbacks: &mut RackCallbacks,
    pending_remove: &mut Vec<ChannelId>,
    pending_convert: &mut Vec<(ChannelId, PatternId)>,
    pending_move: &mut Option<(ChannelId, usize)>,
) {
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;

        let is_dragging = ui
            .ctx()
            .data(|data| data.get_temp::<ChannelDragState>(channel_drag_id()))
            .is_some();
        let handle = ui
            .add(egui::Label::new("≡").sense(Sense::drag()))
            .on_hover_text("Drag to reorder");
        if handle.drag_started() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    channel_drag_id(),
                    ChannelDragState {
                        channel: ch.id,
                        insert_at: index,
                    },
                );
            });
        }
        if handle.dragged() || is_dragging {
            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
        }

        let mute_label = RichText::new("M").color(if ch.mute {
            ui.visuals().warn_fg_color
        } else {
            ui.visuals().text_color()
        });
        let mute_fill = if ch.mute {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };
        if ui
            .add(
                egui::Button::new(mute_label)
                    .min_size(egui::vec2(22.0, 22.0))
                    .fill(mute_fill),
            )
            .on_hover_text("Mute")
            .clicked()
        {
            ch.mute = !ch.mute;
        }

        let solo_label = RichText::new("S").color(if ch.solo {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        });
        let solo_fill = if ch.solo {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().widgets.inactive.bg_fill
        };
        if ui
            .add(
                egui::Button::new(solo_label)
                    .min_size(egui::vec2(22.0, 22.0))
                    .fill(solo_fill),
            )
            .on_hover_text("Solo")
            .clicked()
        {
            ch.solo = !ch.solo;
        }

        ui.color_edit_button_srgba(&mut ch.color);
        ui.text_edit_singleline(&mut ch.name);

        ui.separator();
        let badge_resp = ui.label(egui::RichText::new(kind_badge(ch)).weak());
        badge_resp.context_menu(|ui| {
            if ui.button("Open Piano Roll").clicked() {
                (callbacks.open_piano_roll)(ch.id, pat);
                ui.close_menu();
            }
        });

        ui.separator();
        let instrument_resp = ui.label(egui::RichText::new(instrument_label(ch)).italics().weak());
        instrument_resp.context_menu(|ui| {
            if ui.button("Open Piano Roll").clicked() {
                (callbacks.open_piano_roll)(ch.id, pat);
                ui.close_menu();
            }
        });

        ui.separator();
        ui.add(
            egui::Slider::new(&mut ch.gain_db, -24.0..=12.0)
                .text("Vol (dB)")
                .clamp_to_range(true),
        );
        ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).text("Pan"));

        ui.horizontal(|ui| {
            let mut track_val = ch.mixer_track;
            let badge_fill = if ch.mixer_track == 0 {
                ui.visuals().widgets.inactive.bg_fill
            } else {
                ui.visuals().selection.bg_fill
            };
            let track_badge = ui
                .add(
                    egui::Button::new(
                        RichText::new(format!("CH {}", ch.mixer_track))
                            .monospace()
                            .color(ui.visuals().strong_text_color()),
                    )
                    .min_size(egui::vec2(56.0, 20.0))
                    .fill(badge_fill),
                )
                .on_hover_text("Drag onto a mixer strip to assign");

            if track_badge.drag_started() {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(mixer_assign_drag_id(), ch.id));
            }
            if track_badge.drag_stopped() {
                ui.ctx()
                    .data_mut(|data| data.remove::<ChannelId>(mixer_assign_drag_id()));
            }

            egui::ComboBox::from_label("Mixer Track")
                .selected_text(track_val.to_string())
                .show_ui(ui, |ui| {
                    for track in 0u16..=64 {
                        ui.selectable_value(&mut track_val, track, track.to_string());
                    }
                });
            if track_val != ch.mixer_track {
                ch.mixer_track = track_val;
                (callbacks.set_channel_mixer_track)(ch.id, track_val);
            }
        });

        let mut use_32 = ch.steps_per_bar == 32;
        if ui.toggle_value(&mut use_32, "32").clicked() {
            ch.steps_per_bar = if use_32 { 32 } else { 16 };
        }

        ui.menu_button("⋮", |ui| {
            if matches!(ch.kind, ChannelKind::Instrument | ChannelKind::Sample) {
                if ui.button("Load Plugin…").clicked() {
                    (callbacks.open_plugin_browser)(Some(ch.id));
                    ui.close_menu();
                }
                if ui.button("Load Sample…").clicked() {
                    (callbacks.import_sample_file)(Some(ch.id), None);
                    ui.close_menu();
                }
                ui.separator();
            }
            if matches!(ch.kind, ChannelKind::Instrument | ChannelKind::Sample) {
                if ui.button("Edit in Piano Roll").clicked() {
                    (callbacks.open_piano_roll)(ch.id, pat);
                    ui.close_menu();
                }
                if ui.button("Convert Steps → MIDI Clip").clicked() {
                    pending_convert.push((ch.id, pat));
                    ui.close_menu();
                }
            }
            if matches!(ch.kind, ChannelKind::Instrument)
                && ui.button("Replace Instrument…").clicked()
            {
                (callbacks.open_plugin_browser)(Some(ch.id));
                ui.close_menu();
            }
            if matches!(ch.kind, ChannelKind::Sample) && ui.button("Replace Sample…").clicked() {
                (callbacks.import_sample_file)(Some(ch.id), None);
                ui.close_menu();
            }
            if ui.button("Delete Channel").clicked() {
                pending_remove.push(ch.id);
                ui.close_menu();
            }
        });
    });

    if let Some(mut state) = ui
        .ctx()
        .data_mut(|data| data.get_temp::<ChannelDragState>(channel_drag_id()))
    {
        let row_rect = row.response.rect;
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            if row_rect.contains(pointer) {
                state.insert_at = if pointer.y < row_rect.center().y {
                    index
                } else {
                    (index + 1).min(total_channels)
                };
                ui.ctx()
                    .data_mut(|data| data.insert_temp(channel_drag_id(), state));

                let y_line = if pointer.y < row_rect.center().y {
                    row_rect.top()
                } else {
                    row_rect.bottom()
                };
                ui.painter().line_segment(
                    [
                        egui::pos2(row_rect.left(), y_line),
                        egui::pos2(row_rect.right(), y_line),
                    ],
                    Stroke::new(2.0, ui.visuals().selection.stroke.color),
                );
            }
        }

        if ui.ctx().input(|i| i.pointer.any_released()) {
            *pending_move = Some((state.channel, state.insert_at));
            ui.ctx()
                .data_mut(|data| data.remove::<ChannelDragState>(channel_drag_id()));
        }
    }

    ui.add_space(4.0);
    step_grid(ui, pat, ch);
}

fn step_grid(ui: &mut egui::Ui, pat: PatternId, ch: &mut Channel) {
    let steps_per_bar = ch.steps_per_bar.max(1) as usize;
    let steps = ch
        .steps
        .entry(pat)
        .or_insert_with(|| vec![Step::default(); steps_per_bar]);
    if steps.len() != steps_per_bar {
        steps.resize(steps_per_bar, Step::default());
    }

    let id_base = Id::new(("rack_step_drag", ch.id, pat));
    let mut painting = ui.memory(|m| m.data.get_temp::<bool>(id_base));
    let accent = ch.color;

    ui.horizontal_wrapped(|ui| {
        for (i, st) in steps.iter_mut().enumerate() {
            let label = if st.on { "●" } else { "○" };
            let text_color = if st.on {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            };
            let mut button = egui::Button::new(RichText::new(label).color(text_color))
                .min_size(egui::vec2(18.0, 18.0));
            let beat_fill = ui.visuals().extreme_bg_color;
            let on_fill = accent.gamma_multiply(0.6);
            let off_fill = if (i % 4) == 0 {
                beat_fill
            } else {
                ui.visuals().widgets.inactive.bg_fill
            };

            button = button.fill(if st.on { on_fill } else { off_fill });
            let resp = ui.add(button);
            if resp.clicked() {
                st.on = !st.on;
            }
            if resp.hovered() && ui.input(|i| i.pointer.primary_down()) {
                if painting.is_none() {
                    painting = Some(!st.on);
                }
                if let Some(value) = painting {
                    st.on = value;
                }
            }
            if resp.hovered() && ui.input(|i| i.pointer.secondary_clicked()) {
                ui.memory_mut(|m| m.toggle_popup(resp.id));
            }
            egui::popup_below_widget(ui, resp.id, &resp, |ui: &mut egui::Ui| {
                ui.set_min_width(160.0);
                ui.label("Step params");
                ui.add(egui::Slider::new(&mut st.velocity, 1..=127).text("Velocity"));
                let mut pan = st.pan as i32;
                if ui
                    .add(egui::Slider::new(&mut pan, -64..=63).text("Pan"))
                    .changed()
                {
                    st.pan = pan as i8;
                }
                let mut shift = st.shift_ticks as i32;
                if ui
                    .add(egui::Slider::new(&mut shift, -48..=48).text("Shift"))
                    .changed()
                {
                    st.shift_ticks = shift as i16;
                }
            });
        }
    });

    if ui.input(|i| !i.pointer.primary_down()) {
        if painting.is_some() {
            ui.memory_mut(|m| m.data.remove::<bool>(id_base));
        }
    } else if let Some(value) = painting {
        ui.memory_mut(|m| m.data.insert_temp::<bool>(id_base, value));
    }
}

fn instrument_label(ch: &Channel) -> String {
    match ch.kind {
        ChannelKind::Automation => "Automation".into(),
        ChannelKind::Sample => ch
            .instrument_name
            .clone()
            .unwrap_or_else(|| "No sample loaded".into()),
        ChannelKind::Instrument => ch
            .instrument_name
            .clone()
            .or_else(|| ch.plugin_uid.as_ref().map(|uid| short_uid(uid)))
            .unwrap_or_else(|| "No instrument loaded".into()),
    }
}

fn kind_badge(ch: &Channel) -> String {
    match ch.kind {
        ChannelKind::Instrument => ch
            .plugin_uid
            .as_deref()
            .map(short_uid)
            .unwrap_or_else(|| "Instrument".into()),
        ChannelKind::Sample => "Sample".into(),
        ChannelKind::Automation => "Automation".into(),
    }
}

fn short_uid(uid: &str) -> String {
    if let Some((_, tail)) = uid.rsplit_once("::") {
        tail.to_string()
    } else {
        uid.to_string()
    }
}
