use std::time::{Duration, Instant};

use eframe::egui;

use super::layout::{LayoutState, MASTER_STRIP_WIDTH_PX, STRIP_GAP_PX};
use super::meter::{MeterLevels, MeterState};
use super::{gain_db_to_slider, slider_to_gain_db};

#[test]
fn visible_range_advances_with_scroll() {
    let strip_w_pt = 80.0;
    let gap_pt = 4.0;
    let ls = LayoutState {
        strip_w_pt,
        gap_pt,
        master_w_pt: 120.0,
        zoom: 1.0,
        total: 200,
        content_w_pt: 200.0 * (strip_w_pt + gap_pt),
    };
    let viewport = 400.0;
    let (first, last) = ls.visible_range(0.0, viewport);
    assert_eq!(first, 0);
    assert!(last > first);

    let pitch = ls.strip_pitch_pt();
    let (f2, _) = ls.visible_range(pitch, viewport);
    assert_eq!(f2, first + 1);

    let near_end_scroll = ls.content_w_pt - viewport;
    let (f_end, l_end) = ls.visible_range(near_end_scroll, viewport);
    assert!(l_end <= ls.total);
    assert!(f_end <= f2 + (viewport / pitch).ceil() as usize);
}

#[test]
fn clamp_scroll_stays_in_bounds() {
    let ls = LayoutState {
        strip_w_pt: 70.0,
        gap_pt: 3.0,
        master_w_pt: 100.0,
        zoom: 1.0,
        total: 64,
        content_w_pt: 64.0 * 73.0,
    };
    let view = 600.0;
    assert_eq!(ls.clamp_scroll(-100.0, view), 0.0);
    let max = (ls.content_w_pt - view).max(0.0);
    assert_eq!(ls.clamp_scroll(ls.content_w_pt + 100.0, view), max);
}

#[test]
fn layout_new_respects_zoom_and_dpi() {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(2.0);
    let zoom = 1.2;
    let ls = LayoutState::new(&ctx, 60.0, 100.0, true, zoom, 8, MASTER_STRIP_WIDTH_PX);
    assert!(ls.strip_w_pt > 0.0);
    assert!(ls.gap_pt > 0.0);
    let expected_strip = (60.0 * zoom) / 2.0;
    assert!((ls.strip_w_pt - expected_strip).abs() < f32::EPSILON * 100.0);
    let expected_gap = (STRIP_GAP_PX * zoom) / 2.0;
    assert!((ls.gap_pt - expected_gap).abs() < 1e-3);
}

#[test]
fn meter_peak_hold_decays() {
    let mut meter = MeterState::default();
    meter.update(MeterLevels {
        left_peak: 0.0,
        right_peak: 0.0,
        left_true_peak: 0.0,
        right_true_peak: 0.0,
        clipped: false,
    });
    meter.set_last_update_for_test(Instant::now() - Duration::from_millis(750));
    meter.update(MeterLevels {
        left_peak: -6.0,
        right_peak: -6.0,
        left_true_peak: -6.0,
        right_true_peak: -6.0,
        clipped: false,
    });
    let hold = meter.hold_levels();
    assert!(hold[0] < 0.0);
    assert!(hold[0] > -6.0);
}

#[test]
fn meter_clip_latch_clears() {
    let mut meter = MeterState::default();
    meter.update(MeterLevels {
        left_peak: -3.0,
        right_peak: -3.0,
        left_true_peak: -3.0,
        right_true_peak: -3.0,
        clipped: true,
    });
    assert!(meter.clip_latched());
    meter.clear_clip();
    assert!(!meter.clip_latched());
}

#[test]
fn fader_mapping_round_trip() {
    let slider = gain_db_to_slider(0.0);
    assert!((slider - 1.0).abs() < 1e-6);
    let db = slider_to_gain_db(slider);
    assert!(db.abs() < 1e-4);

    let quiet_slider = gain_db_to_slider(-60.0);
    assert!(quiet_slider < 0.01);
    let db_from_slider = slider_to_gain_db(0.5);
    assert!((db_from_slider + 6.0206).abs() < 0.5);
}
