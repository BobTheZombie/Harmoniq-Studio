use eframe::egui;

use super::layout;
use super::render::MixerUiState;
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};

struct DummyMixerApi;

impl MixerUiApi for DummyMixerApi {
    fn strips_len(&self) -> usize {
        5
    }

    fn strip_info(&self, idx: usize) -> UiStripInfo {
        let mut info = UiStripInfo::default();
        info.name = format!("Track {idx}");
        info.is_master = idx == self.strips_len() - 1;
        info
    }

    fn set_name(&self, _idx: usize, _name: &str) {}
    fn set_color(&self, _idx: usize, _rgba: [f32; 4]) {}
    fn set_fader_db(&self, _idx: usize, _db: f32) {}
    fn set_pan(&self, _idx: usize, _pan: f32) {}
    fn set_width(&self, _idx: usize, _width: f32) {}
    fn toggle_mute(&self, _idx: usize) {}
    fn toggle_solo(&self, _idx: usize) {}
    fn toggle_arm(&self, _idx: usize) {}
    fn toggle_phase(&self, _idx: usize) {}
    fn insert_label(&self, _idx: usize, slot: usize) -> String {
        format!("Insert {slot}")
    }
    fn insert_toggle_bypass(&self, _idx: usize, _slot: usize) {}
    fn insert_is_bypassed(&self, _idx: usize, _slot: usize) -> bool {
        false
    }
    fn insert_move(&self, _idx: usize, _from: usize, _to: usize) {}
    fn send_label(&self, _idx: usize, slot: usize) -> String {
        format!("Send {slot}")
    }
    fn send_level(&self, _idx: usize, _slot: usize) -> f32 {
        -6.0
    }
    fn send_set_level(&self, _idx: usize, _slot: usize, _db: f32) {}
    fn send_toggle_pre(&self, _idx: usize, _slot: usize) {}
    fn send_is_pre(&self, _idx: usize, _slot: usize) -> bool {
        false
    }
    fn route_target_label(&self, _idx: usize) -> String {
        "Master".into()
    }
    fn set_route_target(&self, _idx: usize, _target: u32) {}
    fn level_fetch(&self, idx: usize) -> (f32, f32, f32, f32, bool) {
        if idx == self.strips_len() - 1 {
            (0.0, 0.0, 0.0, 0.0, false)
        } else {
            (-12.0, -12.0, -12.0, -12.0, false)
        }
    }
}

#[test]
fn range_scroll_right_increases_first() {
    let ctx = egui::Context::default();
    let layout = layout::new(&ctx, 80.0, 120.0, true, 1.0, 50, 140.0, 600.0);
    let width = layout.strip_w_pt * 3.0;
    let (first, _) = layout::visible_range(&layout, 0.0, width);
    let pitch = layout.strip_w_pt + layout.gap_pt;
    let (second, _) = layout::visible_range(&layout, pitch * 2.0, width);
    assert!(second > first);
}

#[test]
fn pixel_snap_is_stable_across_ppi() {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.0);
    let snapped = layout::snap_px(&ctx, 12.345);
    let resnapped = layout::snap_px(&ctx, snapped);
    assert!((snapped - resnapped).abs() < f32::EPSILON);

    ctx.set_pixels_per_point(2.0);
    let hi = layout::snap_px(&ctx, 12.345);
    let hi_again = layout::snap_px(&ctx, hi);
    assert!((hi - hi_again).abs() < f32::EPSILON);
}

#[test]
fn no_nested_scroll_areas() {
    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.time = Some(0.0);
    ctx.begin_frame(raw_input);

    let mut state = MixerUiState::default();
    egui::CentralPanel::default().show(&ctx, |ui| {
        super::theme::with_active_theme(&super::theme::MixerTheme::default(), || {
            super::render::mixer(ui, &mut state, &DummyMixerApi);
        });
    });
    ctx.end_frame();

    let stored = ctx.data_mut(|data| {
        data.get_persisted::<egui::containers::scroll_area::State>(egui::Id::new("mixer_scroll"))
    });
    assert!(stored.is_some());

    let total_entries = ctx.data(|data| data.len());
    assert_eq!(total_entries, 1);
}
