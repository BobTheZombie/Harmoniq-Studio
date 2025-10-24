use eframe::egui::{Rect, Vec2};

pub const NARROW_STRIP_WIDTH: f32 = 72.0;
pub const WIDE_STRIP_WIDTH: f32 = 120.0;
pub const MASTER_STRIP_WIDTH: f32 = 132.0;
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

    pub fn base_width(self) -> f32 {
        match self {
            StripDensity::Narrow => NARROW_STRIP_WIDTH,
            StripDensity::Wide => WIDE_STRIP_WIDTH,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleRange {
    pub first: usize,
    pub last: usize,
    pub offset: f32,
}

impl VisibleRange {
    pub fn is_visible(&self, index: usize) -> bool {
        index >= self.first && index < self.last
    }
}

pub fn compute_visible_range(
    total_strips: usize,
    strip_width: f32,
    viewport_width: f32,
    scroll_x: f32,
) -> VisibleRange {
    if total_strips == 0 {
        return VisibleRange {
            first: 0,
            last: 0,
            offset: -scroll_x,
        };
    }
    let strip_space = strip_width;
    let viewable = viewport_width.max(1.0);
    let first = (scroll_x / strip_space).floor().max(0.0) as usize;
    let visible_count = (viewable / strip_space).ceil() as usize + 1;
    let last = (first + visible_count).min(total_strips);
    VisibleRange {
        first,
        last,
        offset: -(scroll_x - first as f32 * strip_space),
    }
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn strip_dimensions(density: StripDensity, zoom: f32) -> Vec2 {
    let width = density.base_width() * zoom;
    let height = 420.0 * zoom.max(1.0);
    Vec2::new(width, height)
}

pub fn master_rect(view_rect: Rect, master_width: f32) -> Rect {
    Rect::from_min_max(view_rect.max - Vec2::new(master_width, 0.0), view_rect.max)
}
