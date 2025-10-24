use std::time::{Duration, Instant};

use super::layout::compute_visible_range;
use super::meter::{MeterLevels, MeterState};
use super::{gain_db_to_slider, slider_to_gain_db};

#[test]
fn virtualization_computes_visible_range() {
    let strip_width = 76.0;
    let viewport = 400.0;
    let range = compute_visible_range(128, strip_width, viewport, 0.0);
    assert_eq!(range.first, 0);
    assert!(range.last > range.first);

    let scrolled = compute_visible_range(128, strip_width, viewport, 360.0);
    assert_eq!(scrolled.first, 4);
    assert!(scrolled.last > scrolled.first);
    assert!(scrolled.offset.abs() <= strip_width);
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
