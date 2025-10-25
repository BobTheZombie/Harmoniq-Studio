use eframe::egui::{self, Id, LayerId, Order};
use harmoniq_engine::mixer::api::MixerUiApi;

use super::debug;
use super::layout;
use super::strip;

#[derive(Debug, Clone)]
pub struct MixerUiState {
    pub narrow: bool,
    pub zoom: f32,
    pub debug: bool,
    pub scroll_x: f32,
}

impl Default for MixerUiState {
    fn default() -> Self {
        Self {
            narrow: true,
            zoom: 1.0,
            debug: false,
            scroll_x: 0.0,
        }
    }
}

pub fn mixer(ui: &mut egui::Ui, state: &mut MixerUiState, api: &dyn MixerUiApi) {
    let total = api.strips_len();
    if total == 0 {
        return;
    }

    let master_index = total.saturating_sub(1);
    let ctx = ui.ctx().clone();
    let zoom = layout::clamp_zoom(state.zoom);
    state.zoom = zoom;

    let available_height = ui.available_height().max(0.0);
    let available_width = ui.available_width().max(0.0);
    let layout = layout::new(
        &ctx,
        76.0,
        120.0,
        state.narrow,
        zoom,
        master_index,
        140.0,
        available_height,
    );

    let (outer_rect, _) = ui.allocate_exact_size(
        egui::vec2(available_width, available_height),
        egui::Sense::hover(),
    );

    let master_rect = egui::Rect::from_min_max(
        egui::pos2(outer_rect.max.x - layout.master_w_pt, outer_rect.min.y),
        outer_rect.max,
    );
    let scroll_rect = egui::Rect::from_min_max(
        outer_rect.min,
        egui::pos2(master_rect.min.x, outer_rect.max.y),
    );
    let left_w = scroll_rect.width().max(0.0);

    let mut scroll_ui = ui.child_ui_with_id_source(
        scroll_rect,
        egui::Layout::top_down(egui::Align::Min),
        "mixer_scroll_container",
    );

    egui::ScrollArea::horizontal()
        .id_source("mixer_scroll")
        .auto_shrink([false, false])
        .show_viewport(&mut scroll_ui, |ui, viewport| {
            state.scroll_x = viewport
                .min
                .x
                .clamp(0.0, (layout.content_w_pt - left_w).max(0.0));
            ui.set_min_size(egui::vec2(layout.content_w_pt, viewport.height()));
            ui.set_height(viewport.height());

            let (first, last) = layout::visible_range(&layout, state.scroll_x, left_w);

            let layer_bg = LayerId::new(Order::Background, Id::new("mixer_bg"));
            let layer_mid = LayerId::new(Order::Middle, Id::new("mixer_mid"));
            let layer_fg = LayerId::new(Order::Foreground, Id::new("mixer_fg"));
            let painter_bg = ui.ctx().layer_painter(layer_bg);
            let painter_mid = ui.ctx().layer_painter(layer_mid);
            let painter_fg = ui.ctx().layer_painter(layer_fg);

            let viewport_clip = viewport.intersect(scroll_rect);

            for index in first..last.min(master_index) {
                let world_x = layout::world_x(&layout, index);
                let snapped = layout::snap_px(&ctx, world_x - state.scroll_x);
                let rect = egui::Rect::from_min_size(
                    egui::pos2(viewport.min.x + snapped, viewport.min.y),
                    egui::vec2(layout.strip_w_pt, viewport.height()),
                );

                if !rect.intersects(viewport) {
                    continue;
                }

                let clip = rect.intersect(viewport_clip);
                if clip.is_empty() {
                    continue;
                }

                let p_bg = painter_bg.with_clip_rect(clip);
                let p_mid = painter_mid.with_clip_rect(clip);
                let p_fg = painter_fg.with_clip_rect(clip);

                strip::draw(&p_bg, &p_mid, &p_fg, rect, index, state, api);
            }

            if state.debug {
                debug::overlay(ui, &layout, state, first, last.min(master_index), viewport);
            }
        });

    if total > 0 {
        let painter = ui.painter_at(master_rect);
        strip::draw_master(&painter, master_rect, master_index, api);
    }
}
