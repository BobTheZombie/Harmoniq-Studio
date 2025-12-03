use crate::convert::steps_to_midi;
use crate::state::{Channel, ChannelId, ChannelKind, PatternId, RackState, Step};
use crate::{RackCallbacks, RackProps};
use egui::{self, Align2, Id, RichText};

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

    let total_channels = state.channels.len();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut index = 0usize;
            while index < total_channels {
                let (_, tail) = state.channels.split_at_mut(index);
                let channel = &mut tail[0];
                channel_row(
                    ui,
                    pattern_id,
                    channel,
                    callbacks,
                    &mut pending_remove,
                    &mut pending_convert,
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
}

fn pattern_strip(ui: &mut egui::Ui, state: &mut RackState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Pattern:").strong());
        for pat in &state.patterns {
            let selected = pat.id == state.current_pattern;
            if ui.selectable_label(selected, pat.name.as_str()).clicked() {
                state.current_pattern = pat.id;
            }
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
    callbacks: &mut RackCallbacks,
    pending_remove: &mut Vec<ChannelId>,
    pending_convert: &mut Vec<(ChannelId, PatternId)>,
) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(ch.mute, "M")
            .on_hover_text("Mute")
            .clicked()
        {
            ch.mute = !ch.mute;
        }
        if ui
            .selectable_label(ch.solo, "S")
            .on_hover_text("Solo")
            .clicked()
        {
            ch.solo = !ch.solo;
        }

        ui.text_edit_singleline(&mut ch.name);

        ui.separator();
        ui.label(egui::RichText::new(kind_badge(ch)).weak());

        ui.separator();
        ui.label(egui::RichText::new(instrument_label(ch)).italics().weak());

        ui.separator();
        ui.add(
            egui::Slider::new(&mut ch.gain_db, -24.0..=12.0)
                .text("Vol (dB)")
                .clamp_to_range(true),
        );
        ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).text("Pan"));

        ui.horizontal(|ui| {
            ui.label("Mixer");
            let mut track_val = ch.mixer_track.unwrap_or(0);
            let response = ui.add(
                egui::DragValue::new(&mut track_val)
                    .clamp_range(0..=199)
                    .speed(0.2),
            );
            if response.changed() {
                ch.mixer_track = if track_val == 0 {
                    None
                } else {
                    Some(track_val)
                };
                (callbacks.set_mixer_track)(ch.id, ch.mixer_track);
            }
            if ui
                .small_button("✕")
                .on_hover_text("Unassign from mixer")
                .clicked()
            {
                ch.mixer_track = None;
                (callbacks.set_mixer_track)(ch.id, None);
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

    ui.horizontal_wrapped(|ui| {
        for (i, st) in steps.iter_mut().enumerate() {
            let label = if st.on { "●" } else { "○" };
            let mut button = egui::Button::new(label).min_size(egui::vec2(18.0, 18.0));
            if (i % 4) == 0 {
                button = button.fill(ui.visuals().extreme_bg_color);
            }
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
