use eframe::egui::{self, Align2, Frame, Margin, RichText, Stroke};
use harmoniq_engine::rt::metrics::BlockStat;
use harmoniq_ui::HarmoniqPalette;

pub struct MetricsHud {
    average_load: f32,
    max_block_ns: u64,
    xruns: u32,
    rt_tick_hz: f32,
    worker_count: u32,
}

impl MetricsHud {
    pub fn new() -> Self {
        Self {
            average_load: 0.0,
            max_block_ns: 0,
            xruns: 0,
            rt_tick_hz: 0.0,
            worker_count: 0,
        }
    }

    pub fn pull(&mut self, stats: Vec<BlockStat>, sample_rate: u32, frames: u32) {
        if stats.is_empty() {
            return;
        }
        let mut max_block_ns = self.max_block_ns;
        let mut xruns = self.xruns;
        let mut total_load = 0.0f32;
        let mut count = 0usize;
        let period_ns = if sample_rate > 0 {
            (1_000_000_000u64 * frames as u64) / sample_rate as u64
        } else {
            0
        };
        for stat in stats.iter() {
            max_block_ns = max_block_ns.max(stat.ns);
            xruns = xruns.max(stat.xruns);
            if period_ns > 0 {
                let load = (stat.ns as f64) / (period_ns as f64);
                total_load += load as f32;
                count += 1;
            }
        }
        if count > 0 {
            self.average_load = total_load / count as f32;
        }
        self.max_block_ns = max_block_ns;
        self.xruns = xruns;
        if frames > 0 {
            self.rt_tick_hz = sample_rate as f32 / frames as f32;
        }
    }

    pub fn average_load(&self) -> f32 {
        self.average_load
    }

    pub fn max_block_us(&self) -> u32 {
        (self.max_block_ns / 1_000).min(u32::MAX as u64) as u32
    }

    pub fn xruns(&self) -> u32 {
        self.xruns
    }

    pub fn rt_tick_hz(&self) -> f32 {
        self.rt_tick_hz
    }

    pub fn worker_count(&self) -> u32 {
        self.worker_count
    }

    pub fn show(&self, ctx: &egui::Context, palette: &HarmoniqPalette) {
        egui::Area::new("metrics_hud".into())
            .anchor(Align2::RIGHT_TOP, [-16.0, 16.0])
            .show(ctx, |ui| {
                Frame::none()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.toolbar_outline))
                    .rounding(egui::Rounding::same(12.0))
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        let load_pct = (self.average_load * 100.0).min(999.9);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Audio Load").color(palette.text_muted));
                            ui.label(
                                RichText::new(format!("{load_pct:>5.1}%"))
                                    .color(palette.text_primary)
                                    .monospace(),
                            );
                            ui.separator();
                            ui.label(
                                RichText::new(format!(
                                    "Max block: {} µs",
                                    self.max_block_ns / 1_000
                                ))
                                .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new(format!("XRuns: {}", self.xruns))
                                    .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new(format!("RT tick: {:.1} Hz", self.rt_tick_hz))
                                    .color(palette.text_muted),
                            );
                            ui.label(
                                RichText::new(format!("Workers: {}", self.worker_count))
                                    .color(palette.text_muted),
                            );
                        });
                    });
            });
    }

    pub fn set_worker_count(&mut self, count: u32) {
        self.worker_count = count;
    }
}
