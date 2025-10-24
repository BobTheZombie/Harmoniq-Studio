mod context;
mod inserts;
mod layout;
mod meter;
mod sends;
mod strip;
mod theme;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, pos2, Align, Key, Rect, Ui, Vec2};
use harmoniq_engine::mixer::api::MixerUiApi;
use harmoniq_ui::HarmoniqPalette;

pub use layout::{clamp_zoom, strip_dimensions, StripDensity};
pub use meter::{MeterLevels, MeterState};
pub use theme::MixerTheme;

use crate::ui::mixer::layout::{compute_visible_range, master_rect, MASTER_STRIP_WIDTH};
use crate::ui::mixer::strip::{render_strip, StripRenderArgs};

pub struct MixerView {
    api: Arc<dyn MixerUiApi>,
    density: StripDensity,
    zoom: f32,
    selection: HashSet<u32>,
    last_clicked: Option<u32>,
    meters: Vec<MeterState>,
    theme: MixerTheme,
    pending_focus: Option<usize>,
    rename: Option<RenameState>,
    group_highlight: bool,
}

#[derive(Debug, Clone)]
struct RenameState {
    id: u32,
    name: String,
}

impl MixerView {
    pub fn new(api: Arc<dyn MixerUiApi>) -> Self {
        Self {
            api,
            density: StripDensity::Narrow,
            zoom: 1.0,
            selection: HashSet::new(),
            last_clicked: None,
            meters: Vec::new(),
            theme: MixerTheme::dark(),
            pending_focus: None,
            rename: None,
            group_highlight: true,
        }
    }

    pub fn toggle_density(&mut self) {
        self.density = self.density.toggle();
    }

    pub fn zoom_in(&mut self) {
        self.zoom = clamp_zoom(self.zoom + 0.05);
    }

    pub fn zoom_out(&mut self) {
        self.zoom = clamp_zoom(self.zoom - 0.05);
    }

    pub fn ui(&mut self, ui: &mut Ui, palette: &HarmoniqPalette) {
        self.theme = theme_from_palette(palette);
        self.handle_shortcuts(ui);
        ui.ctx().request_repaint_after(Duration::from_millis(16));

        let total = self.api.strips_len();
        if total <= 1 {
            return;
        }
        ensure_meter_count(&mut self.meters, total);

        let master_index = total - 1;
        let strip_count = master_index;

        let strip_size = strip_dimensions(self.density, self.zoom);
        let total_width = strip_size.x * strip_count as f32;

        egui::ScrollArea::horizontal()
            .id_source("mixer_scroll")
            .show_viewport(ui, |ui, viewport| {
                ui.set_min_size(Vec2::new(
                    total_width + MASTER_STRIP_WIDTH * self.zoom,
                    strip_size.y,
                ));
                if let Some(target) = self.pending_focus.take() {
                    let focus_rect = Rect::from_min_size(
                        pos2(target as f32 * strip_size.x, viewport.min.y),
                        strip_size,
                    );
                    ui.scroll_to_rect(focus_rect, Some(Align::Center));
                }

                let visible = compute_visible_range(
                    strip_count,
                    strip_size.x,
                    viewport.width(),
                    viewport.min.x,
                );

                let content_clip = Rect::from_min_max(
                    viewport.min,
                    pos2(
                        viewport.max.x - MASTER_STRIP_WIDTH * self.zoom,
                        viewport.max.y,
                    ),
                );
                ui.scope(|clip_ui| {
                    clip_ui.set_clip_rect(content_clip);
                    for index in visible.first..visible.last {
                        let info = self.api.strip_info(index);
                        let info_for_render = info.clone();
                        let meter_levels = self.api.level_fetch(index);
                        self.meters[index].update(levels_from_tuple(meter_levels));

                        let x = index as f32 * strip_size.x + visible.offset;
                        let strip_rect = Rect::from_min_size(
                            pos2(viewport.min.x + x, viewport.min.y),
                            strip_size,
                        );
                        if !strip_rect.intersects(content_clip) {
                            continue;
                        }
                        let insert_labels = (0..info.insert_count)
                            .map(|slot| self.api.insert_label(index, slot))
                            .collect::<Vec<_>>();
                        let send_labels = (0..info.send_count)
                            .map(|slot| self.api.send_label(index, slot))
                            .collect::<Vec<_>>();

                        let api = Arc::clone(&self.api);
                        let theme = self.theme.clone();
                        let density = self.density;
                        let is_selected = self.selection.contains(&info.id);

                        let response = {
                            let meter_state = &mut self.meters[index];
                            let insert_labels = insert_labels;
                            let send_labels = send_labels;
                            clip_ui
                                .allocate_ui_at_rect(strip_rect, move |ui| {
                                    render_strip(StripRenderArgs {
                                        ui,
                                        api: api.as_ref(),
                                        info: &info_for_render,
                                        index,
                                        density,
                                        theme: &theme,
                                        width: strip_size.x,
                                        height: strip_size.y,
                                        zoom: self.zoom,
                                        is_selected,
                                        meter: meter_state,
                                        insert_labels,
                                        send_labels,
                                        group_highlight: self.group_highlight,
                                    })
                                })
                                .inner
                        };

                        if response.clicked {
                            self.handle_selection(clip_ui, info.id);
                        }
                        if response.double_clicked {
                            self.rename = Some(RenameState {
                                id: info.id,
                                name: info.name.clone(),
                            });
                        }
                    }
                });

                self.draw_master_strip(ui, viewport, strip_size, master_index);
            });

        self.show_rename_dialog(ui.ctx());
    }

    fn draw_master_strip(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        strip_size: Vec2,
        master_index: usize,
    ) {
        let info = self.api.strip_info(master_index);
        let levels = self.api.level_fetch(master_index);
        self.meters[master_index].update(levels_from_tuple(levels));

        let rect = master_rect(viewport, MASTER_STRIP_WIDTH * self.zoom);
        ui.allocate_ui_at_rect(rect, |ui| {
            let insert_labels = (0..info.insert_count)
                .map(|slot| self.api.insert_label(master_index, slot))
                .collect::<Vec<_>>();
            let send_labels = (0..info.send_count)
                .map(|slot| self.api.send_label(master_index, slot))
                .collect::<Vec<_>>();
            let _ = render_strip(StripRenderArgs {
                ui,
                api: self.api.as_ref(),
                info: &info,
                index: master_index,
                density: StripDensity::Wide,
                theme: &self.theme,
                width: rect.width(),
                height: strip_size.y,
                zoom: self.zoom,
                is_selected: false,
                meter: &mut self.meters[master_index],
                insert_labels,
                send_labels,
                group_highlight: self.group_highlight,
            });
        });
    }

    fn handle_shortcuts(&mut self, ui: &Ui) {
        if ui.ctx().input(|i| i.key_pressed(Key::N)) {
            self.density = StripDensity::Narrow;
        }
        if ui.ctx().input(|i| i.key_pressed(Key::W)) {
            self.density = StripDensity::Wide;
        }
        if ui.ctx().input(|i| {
            i.modifiers.command && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals))
        }) {
            self.zoom = clamp_zoom(self.zoom + 0.05);
        }
        if ui
            .ctx()
            .input(|i| i.modifiers.command && i.key_pressed(Key::Minus))
        {
            self.zoom = clamp_zoom(self.zoom - 0.05);
        }
        if ui.ctx().input(|i| i.key_pressed(Key::G)) {
            self.group_highlight = !self.group_highlight;
        }
        if ui
            .ctx()
            .input(|i| i.modifiers.ctrl && i.key_pressed(Key::ArrowRight))
        {
            self.nudge_focus(8);
        }
        if ui
            .ctx()
            .input(|i| i.modifiers.ctrl && i.key_pressed(Key::ArrowLeft))
        {
            self.nudge_focus(-8);
        }
    }

    fn nudge_focus(&mut self, delta: isize) {
        let total = self.api.strips_len();
        if total == 0 {
            return;
        }
        let current = self
            .last_clicked
            .and_then(|id| self.index_for_id(id))
            .unwrap_or(0);
        let new_index = (current as isize + delta).clamp(0, (total as isize) - 2) as usize;
        self.pending_focus = Some(new_index);
    }

    fn index_for_id(&self, id: u32) -> Option<usize> {
        (0..self.api.strips_len()).find(|&idx| self.api.strip_info(idx).id == id)
    }

    fn handle_selection(&mut self, ui: &Ui, id: u32) {
        let shift = ui.ctx().input(|i| i.modifiers.shift);
        if shift {
            if !self.selection.insert(id) {
                self.selection.remove(&id);
            }
        } else {
            self.selection.clear();
            self.selection.insert(id);
            self.last_clicked = Some(id);
        }
    }

    fn show_rename_dialog(&mut self, ctx: &egui::Context) {
        if self.rename.is_none() {
            return;
        }

        let mut close_dialog = false;
        let mut apply_change: Option<(u32, String)> = None;

        if let Some(rename) = self.rename.as_mut() {
            let mut open = true;
            egui::Window::new("Rename Track")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Track name");
                    ui.text_edit_singleline(&mut rename.name);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                        if ui.button("OK").clicked() {
                            apply_change = Some((rename.id, rename.name.clone()));
                            close_dialog = true;
                        }
                    });
                });

            if !open {
                close_dialog = true;
            }
        }

        if let Some((id, name)) = apply_change {
            if let Some(index) = self.index_for_id(id) {
                self.api.set_name(index, &name);
            }
        }

        if close_dialog {
            self.rename = None;
        }
    }

    pub fn cpu_estimate(&self) -> f32 {
        let total = self.api.strips_len();
        if total == 0 {
            return 0.0;
        }
        let master = self.api.strip_info(total - 1);
        master.cpu_percent
    }

    pub fn master_meter(&self) -> (f32, f32) {
        let total = self.api.strips_len();
        if total == 0 {
            return (f32::NEG_INFINITY, f32::NEG_INFINITY);
        }
        let levels = self.api.level_fetch(total - 1);
        (levels.0, levels.1)
    }
}

fn ensure_meter_count(meters: &mut Vec<MeterState>, total: usize) {
    if meters.len() < total {
        meters.resize_with(total, MeterState::default);
    }
}

fn levels_from_tuple(tuple: (f32, f32, f32, f32, bool)) -> MeterLevels {
    MeterLevels {
        left_peak: tuple.0,
        right_peak: tuple.1,
        left_true_peak: tuple.2,
        right_true_peak: tuple.3,
        clipped: tuple.4,
    }
}

fn theme_from_palette(palette: &HarmoniqPalette) -> MixerTheme {
    let mut theme = MixerTheme::dark();
    theme.background = palette.panel;
    theme.strip_bg = palette.panel_alt;
    theme.header_text = palette.text_primary;
    theme.accent = palette.accent;
    theme.selection = palette.accent;
    theme.icon_bg = palette.panel_alt.linear_multiply(1.05);
    theme.knob_bg = palette.panel_alt.linear_multiply(0.9);
    theme.fader_track = palette.panel_alt.linear_multiply(0.85);
    theme
}

pub fn gain_db_to_slider(db: f32) -> f32 {
    let linear = 10.0f32.powf(db / 20.0);
    linear.clamp(0.0, 1.0)
}

pub fn slider_to_gain_db(value: f32) -> f32 {
    if value <= 0.001 {
        -60.0
    } else {
        20.0 * value.log10()
    }
}
