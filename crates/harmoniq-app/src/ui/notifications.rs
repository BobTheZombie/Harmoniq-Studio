use std::time::{Duration, Instant};

use egui::{self, Color32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub title: String,
    pub detail: Option<String>,
    pub progress: Option<f32>,
    pub kind: NoticeKind,
    pub created: Instant,
    pub duration: Duration,
}

impl Notice {
    pub fn new(title: impl Into<String>, kind: NoticeKind) -> Self {
        Self {
            title: title.into(),
            detail: None,
            progress: None,
            kind,
            created: Instant::now(),
            duration: Duration::from_secs(4),
        }
    }
}

#[derive(Default, Debug)]
pub struct Notifications {
    pub queue: Vec<Notice>,
}

impl Notifications {
    #[allow(dead_code)]
    pub fn push(&mut self, notice: Notice) {
        self.queue.push(notice);
    }

    #[allow(dead_code)]
    pub fn info(&mut self, title: impl Into<String>) {
        self.queue.push(Notice::new(title, NoticeKind::Info));
    }

    #[allow(dead_code)]
    pub fn success(&mut self, title: impl Into<String>) {
        self.queue.push(Notice::new(title, NoticeKind::Success));
    }

    #[allow(dead_code)]
    pub fn warning(&mut self, title: impl Into<String>) {
        self.queue.push(Notice::new(title, NoticeKind::Warning));
    }

    #[allow(dead_code)]
    pub fn error(&mut self, title: impl Into<String>) {
        self.queue.push(Notice::new(title, NoticeKind::Error));
    }

    fn clear_finished(&mut self) {
        let now = Instant::now();
        self.queue
            .retain(|notice| now.duration_since(notice.created) < notice.duration);
    }

    pub fn paint(&mut self, ctx: &egui::Context) {
        self.clear_finished();
        let screen = ctx.input(|i| i.screen_rect());
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toasts"));
        let painter = ctx.layer_painter(layer);
        let mut y = 12.0;

        for notice in self.queue.iter() {
            let (bg, accent) = colors_for(notice.kind);
            let width = 320.0;
            let mut height = 64.0;

            if notice.detail.is_some() {
                height += 16.0;
            }
            if notice.progress.is_some() {
                height += 16.0;
            }

            let rect = egui::Rect::from_min_max(
                egui::pos2(screen.max.x - 12.0 - width, screen.min.y + y),
                egui::pos2(screen.max.x - 12.0, screen.min.y + y + height),
            );

            painter.rect(
                rect,
                12.0,
                Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 200),
                egui::Stroke::new(1.0, accent),
            );
            painter.text(
                rect.min + egui::vec2(12.0, 10.0),
                egui::Align2::LEFT_TOP,
                &notice.title,
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );

            if let Some(detail) = &notice.detail {
                painter.text(
                    rect.min + egui::vec2(12.0, 30.0),
                    egui::Align2::LEFT_TOP,
                    detail,
                    egui::FontId::proportional(12.0),
                    Color32::LIGHT_GRAY,
                );
            }

            if let Some(progress) = notice.progress {
                let bar = egui::Rect::from_min_size(
                    rect.min + egui::vec2(12.0, height - 20.0),
                    egui::vec2(width - 24.0, 8.0),
                );
                let visuals = ctx.style().visuals.clone();
                painter.rect_filled(bar, 4.0, visuals.faint_bg_color);
                let fill = egui::Rect::from_min_size(
                    bar.min,
                    egui::vec2(bar.width() * progress.clamp(0.0, 1.0), bar.height()),
                );
                painter.rect_filled(fill, 4.0, accent);
            }

            y += height + 8.0;
        }
    }
}

fn colors_for(kind: NoticeKind) -> (Color32, Color32) {
    match kind {
        NoticeKind::Info => (
            Color32::from_rgb(32, 56, 112),
            Color32::from_rgb(64, 128, 255),
        ),
        NoticeKind::Success => (
            Color32::from_rgb(24, 64, 32),
            Color32::from_rgb(64, 200, 96),
        ),
        NoticeKind::Warning => (
            Color32::from_rgb(64, 48, 0),
            Color32::from_rgb(240, 200, 64),
        ),
        NoticeKind::Error => (
            Color32::from_rgb(72, 24, 24),
            Color32::from_rgb(240, 96, 96),
        ),
    }
}
