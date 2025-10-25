use eframe::egui;

#[derive(Clone, Debug)]
pub struct Layout {
    pub strip_w_pt: f32,
    pub gap_pt: f32,
    pub master_w_pt: f32,
    pub height_pt: f32,
    pub zoom: f32,
    pub total: usize,
    pub content_w_pt: f32,
}

pub fn new(
    ctx: &egui::Context,
    narrow_px: f32,
    wide_px: f32,
    narrow: bool,
    zoom: f32,
    total: usize,
    master_px: f32,
    height_pt: f32,
) -> Layout {
    let ppp = ctx.pixels_per_point().max(1.0);
    let base = if narrow { narrow_px } else { wide_px };
    let to_pt = |px: f32| (px * zoom) / ppp;
    let strip_w_pt = to_pt(base).max(48.0 / ppp);
    let gap_pt = to_pt(4.0);
    let master_w_pt = to_pt(master_px);
    let content_w_pt = total as f32 * (strip_w_pt + gap_pt);
    Layout {
        strip_w_pt,
        gap_pt,
        master_w_pt,
        height_pt,
        zoom,
        total,
        content_w_pt,
    }
}

pub fn visible_range(layout: &Layout, scroll_x: f32, view_w: f32) -> (usize, usize) {
    if layout.total == 0 {
        return (0, 0);
    }
    let pitch = layout.strip_w_pt + layout.gap_pt;
    let first = (scroll_x / pitch).floor().max(0.0) as isize;
    let last = ((scroll_x + view_w) / pitch).ceil() as isize + 1;
    (
        first.clamp(0, layout.total as isize) as usize,
        last.clamp(first, layout.total as isize) as usize,
    )
}

pub fn world_x(layout: &Layout, idx: usize) -> f32 {
    idx as f32 * (layout.strip_w_pt + layout.gap_pt)
}

pub fn snap_px(ctx: &egui::Context, p: f32) -> f32 {
    let ppp = ctx.pixels_per_point();
    ((p * ppp).round()) / ppp
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(0.8, 1.5)
}
