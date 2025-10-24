use eframe::egui::{self, Rect, Vec2};

pub const NARROW_STRIP_WIDTH_PX: f32 = 76.0;
pub const WIDE_STRIP_WIDTH_PX: f32 = 120.0;
pub const MASTER_STRIP_RATIO: f32 = 1.8;
pub const MASTER_STRIP_WIDTH_PX: f32 = WIDE_STRIP_WIDTH_PX * MASTER_STRIP_RATIO;
pub const STRIP_GAP_PX: f32 = 4.0;
pub const MIN_ZOOM: f32 = 0.8;
pub const MAX_ZOOM: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripDensity {
    Narrow,
    Wide,
}

impl StripDensity {
    pub fn toggle(self) -> Self {
        match self {
            StripDensity::Narrow => StripDensity::Wide,
            StripDensity::Wide => StripDensity::Narrow,
        }
    }

    pub fn base_width_px(self) -> f32 {
        match self {
            StripDensity::Narrow => NARROW_STRIP_WIDTH_PX,
            StripDensity::Wide => WIDE_STRIP_WIDTH_PX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutState {
    pub strip_w_pt: f32,
    pub gap_pt: f32,
    pub master_w_pt: f32,
    pub zoom: f32,
    pub total: usize,
    pub content_w_pt: f32,
}

impl LayoutState {
    pub fn new(
        ctx: &egui::Context,
        narrow_w_px: f32,
        wide_w_px: f32,
        is_narrow: bool,
        zoom: f32,
        total: usize,
        master_w_px: f32,
    ) -> Self {
        let ppp = ctx.pixels_per_point();
        let base_w_px = if is_narrow { narrow_w_px } else { wide_w_px };
        let strip_w_pt = (base_w_px * zoom) / ppp;
        let gap_pt = (STRIP_GAP_PX * zoom) / ppp;
        let master_w_pt = (master_w_px * zoom) / ppp;
        let content_w_pt = total as f32 * (strip_w_pt + gap_pt);
        Self {
            strip_w_pt,
            gap_pt,
            master_w_pt,
            zoom,
            total,
            content_w_pt,
        }
    }

    pub fn strip_pitch_pt(&self) -> f32 {
        self.strip_w_pt + self.gap_pt
    }

    pub fn clamp_scroll(&self, scroll_x_pt: f32, view_w_pt: f32) -> f32 {
        let max_scroll = (self.content_w_pt - view_w_pt).max(0.0);
        scroll_x_pt.clamp(0.0, max_scroll)
    }

    pub fn visible_range(&self, scroll_x_pt: f32, view_w_pt: f32) -> (usize, usize) {
        if self.total == 0 {
            return (0, 0);
        }
        let strip_pitch = self.strip_pitch_pt();
        let first = (scroll_x_pt / strip_pitch).floor().max(0.0) as isize;
        let last = ((scroll_x_pt + view_w_pt) / strip_pitch).ceil() as isize + 1;
        let first = first.clamp(0, self.total as isize) as usize;
        let last = last.clamp(first as isize, self.total as isize) as usize;
        (first, last)
    }

    pub fn world_x(&self, idx: usize) -> f32 {
        idx as f32 * self.strip_pitch_pt()
    }
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn strip_height_pt(ctx: &egui::Context, zoom: f32) -> f32 {
    let ppp = ctx.pixels_per_point();
    (520.0 * zoom.clamp(0.9, 1.3)) / ppp
}

pub fn master_rect(view_rect: Rect, master_width: f32) -> Rect {
    Rect::from_min_max(view_rect.max - Vec2::new(master_width, 0.0), view_rect.max)
}
