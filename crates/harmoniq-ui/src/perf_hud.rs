use egui::{self, Align2, Frame, Margin, Rounding, Stroke, Vec2};

/// Non-blocking performance HUD. Does not intercept input.
/// Show a small chip; on hover or when `expanded` is true, show full panel.
pub struct PerfHudState {
    pub visible: bool,
    pub expanded: bool,
    pub last_activity_ms: u64, // millis since last xrun/update; for auto-hide
}

impl Default for PerfHudState {
    fn default() -> Self {
        Self {
            visible: false,
            expanded: false,
            last_activity_ms: 0,
        }
    }
}

pub struct PerfMetrics {
    pub audio_load: f32, // 0..1
    pub max_block_us: u32,
    pub xruns_total: u64,
    pub rt_tick_hz: f32,
    pub workers: u32,
}

pub fn perf_hud(ctx: &egui::Context, st: &mut PerfHudState, m: &PerfMetrics) {
    if !st.visible {
        return;
    }

    // Auto-hide if idle for > 4s and not hovered / not expanded
    let idle = st.last_activity_ms > 4_000;
    let mut hovered = false;

    // Small chip (top-right)
    egui::Area::new("perf_hud_chip".into())
        .anchor(Align2::RIGHT_TOP, Vec2::new(-8.0, 8.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            let frame = Frame::none()
                .fill(ui.visuals().extreme_bg_color.gamma_multiply(0.92))
                .rounding(Rounding::same(10.0))
                .stroke(Stroke::new(1.0, ui.visuals().faint_bg_color))
                .inner_margin(Margin::symmetric(10.0, 5.0));
            frame.show(ui, |ui| {
                let pct = (m.audio_load * 100.0).clamp(0.0, 999.0);
                let bar = egui::ProgressBar::new(m.audio_load)
                    .desired_width(100.0)
                    .text(format!("{pct:.1}%"));
                ui.horizontal(|ui| {
                    ui.label("Audio");
                    ui.add(bar);
                });
                hovered = ui.rect_contains_pointer(ui.max_rect());
            });
        });

    // Expanded card (center-top), only when hovered or forced
    if st.expanded || hovered {
        egui::Area::new("perf_hud_card".into())
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 48.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                let frame = Frame::none()
                    .fill(ui.visuals().extreme_bg_color.gamma_multiply(0.95))
                    .rounding(Rounding::same(12.0))
                    .stroke(Stroke::new(1.0, ui.visuals().faint_bg_color))
                    .inner_margin(Margin::symmetric(14.0, 10.0));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Audio Load").strong());
                        let pct = (m.audio_load * 100.0).clamp(0.0, 999.0);
                        ui.label(format!("{pct:.1}%"));
                        ui.add(egui::ProgressBar::new(m.audio_load).desired_width(240.0));
                        ui.add_space(8.0);
                        // Close button (doesn't intercept global input)
                        if ui.add(egui::Button::new("✕")).clicked() {
                            // NB: area is non-interactable, but the button still toggles our local state
                            // because clicks are within our paint pass; we treat it as a visual control
                            // and just hide on hover close intent.
                            // If your egui version ignores clicks in non-interactable Area, toggle via hover instead:
                            st.visible = false;
                        }
                    });
                    ui.separator();
                    ui.label(format!("Max block: {} μs", m.max_block_us));
                    ui.label(format!("XRuns: {}", m.xruns_total));
                    ui.label(format!("RT tick: {:.1} Hz", m.rt_tick_hz));
                    ui.label(format!("Workers: {}", m.workers));
                });
            });
    }

    if idle && !hovered && !st.expanded {
        st.visible = false;
    }
}

/// Helper to update activity / xrun events from the app.
pub fn perf_hud_on_activity(st: &mut PerfHudState) {
    st.last_activity_ms = 0;
}

/// Call each frame with `dt_ms`.
pub fn perf_hud_tick(st: &mut PerfHudState, dt_ms: u64) {
    st.last_activity_ms = st.last_activity_ms.saturating_add(dt_ms);
}
