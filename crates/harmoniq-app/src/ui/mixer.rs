use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::Ui;
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};
use harmoniq_mixer::state::{Channel, ChannelId, InsertSlot, Meter, MixerState, SendSlot};
use harmoniq_mixer::ui::{db_to_gain, gain_to_db};
use harmoniq_mixer::{self as simple_mixer, MixerCallbacks, MixerProps};
use harmoniq_ui::HarmoniqPalette;
use tracing::{info, warn};

#[derive(Clone, Debug)]
struct StripSnapshot {
    index: usize,
    mute: bool,
    solo: bool,
    insert_bypass: Vec<bool>,
    send_levels_db: Vec<f32>,
}

pub struct MixerView {
    api: Arc<dyn MixerUiApi>,
    state: MixerState,
    master_cpu: f32,
    master_meter_db: (f32, f32),
}

impl MixerView {
    pub fn new(api: Arc<dyn MixerUiApi>) -> Self {
        Self {
            api,
            state: MixerState::default(),
            master_cpu: 0.0,
            master_meter_db: (f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    pub fn toggle_density(&mut self) {}

    pub fn zoom_in(&mut self) {}

    pub fn zoom_out(&mut self) {}

    pub fn ui(&mut self, ui: &mut Ui, _palette: &HarmoniqPalette) {
        let snapshots = self.sync_from_api();
        let mut callbacks = self.build_callbacks(&snapshots);
        simple_mixer::render(
            ui,
            MixerProps {
                state: &mut self.state,
                callbacks: &mut callbacks,
            },
        );
    }

    pub fn cpu_estimate(&self) -> f32 {
        self.master_cpu
    }

    pub fn master_meter(&self) -> (f32, f32) {
        self.master_meter_db
    }

    fn sync_from_api(&mut self) -> HashMap<ChannelId, StripSnapshot> {
        let total = self.api.strips_len();
        let mut snapshots = HashMap::with_capacity(total);
        let previous_selection = self.state.selected;
        self.state.channels.clear();

        for idx in 0..total {
            let info = self.api.strip_info(idx);
            let snapshot = self.populate_channel(idx, &info);
            snapshots.insert(info.id, snapshot);
        }

        if let Some(selected) = previous_selection {
            if self.state.channels.iter().any(|ch| ch.id == selected) {
                self.state.selected = Some(selected);
            } else {
                self.state.selected = None;
            }
        } else {
            self.state.selected = None;
        }

        snapshots
    }

    fn populate_channel(&mut self, idx: usize, info: &UiStripInfo) -> StripSnapshot {
        let mut channel = Channel {
            id: info.id,
            name: info.name.clone(),
            gain_db: info.fader_db,
            pan: info.pan,
            mute: info.muted,
            solo: info.soloed,
            inserts: Vec::with_capacity(info.insert_count),
            sends: Vec::with_capacity(info.send_count),
            meter: Meter::default(),
            is_master: info.is_master,
        };

        let mut insert_bypass = Vec::with_capacity(info.insert_count);
        for slot in 0..info.insert_count {
            let bypass = self.api.insert_is_bypassed(idx, slot);
            let label = self.api.insert_label(idx, slot);
            channel.inserts.push(InsertSlot {
                name: label,
                bypass,
            });
            insert_bypass.push(bypass);
        }

        let mut send_levels_db = Vec::with_capacity(info.send_count);
        for slot in 0..info.send_count {
            let level_db = self.api.send_level(idx, slot);
            let level = db_to_gain(level_db).clamp(0.0, 2.0);
            channel.sends.push(SendSlot {
                id: slot as u8,
                level,
            });
            send_levels_db.push(level_db);
        }

        let (peak_l_db, peak_r_db, rms_l_db, rms_r_db, _clipped) = self.api.level_fetch(idx);
        let mut meter = Meter::default();
        meter.peak_l = db_to_linear(peak_l_db);
        meter.peak_r = db_to_linear(peak_r_db);
        meter.rms_l = db_to_linear(rms_l_db);
        meter.rms_r = db_to_linear(rms_r_db);
        meter.peak_hold_l = meter.peak_l;
        meter.peak_hold_r = meter.peak_r;
        meter.last_update = Instant::now();
        channel.meter = meter;

        if info.is_master {
            self.master_cpu = info.cpu_percent;
            self.master_meter_db = (peak_l_db, peak_r_db);
        }

        self.state.channels.push(channel);

        StripSnapshot {
            index: idx,
            mute: info.muted,
            solo: info.soloed,
            insert_bypass,
            send_levels_db,
        }
    }

    fn build_callbacks(&self, snapshots: &HashMap<ChannelId, StripSnapshot>) -> MixerCallbacks {
        let mut callbacks = MixerCallbacks::noop();

        let api_gain = Arc::clone(&self.api);
        let map_gain = snapshots.clone();
        callbacks.set_gain_pan = Box::new(move |channel_id, db, pan| {
            if let Some(snapshot) = map_gain.get(&channel_id) {
                api_gain.set_fader_db(snapshot.index, db);
                api_gain.set_pan(snapshot.index, pan);
            }
        });

        let api_mute = Arc::clone(&self.api);
        let map_mute = snapshots.clone();
        callbacks.set_mute = Box::new(move |channel_id, mute| {
            if let Some(snapshot) = map_mute.get(&channel_id) {
                if snapshot.mute != mute {
                    api_mute.toggle_mute(snapshot.index);
                }
            }
        });

        let api_solo = Arc::clone(&self.api);
        let map_solo = snapshots.clone();
        callbacks.set_solo = Box::new(move |channel_id, solo| {
            if let Some(snapshot) = map_solo.get(&channel_id) {
                if snapshot.solo != solo {
                    api_solo.toggle_solo(snapshot.index);
                }
            }
        });

        let api_bypass = Arc::clone(&self.api);
        let map_bypass = snapshots.clone();
        callbacks.set_insert_bypass = Box::new(move |channel_id, slot, bypass| {
            if let Some(snapshot) = map_bypass.get(&channel_id) {
                if snapshot.insert_bypass.get(slot).copied().unwrap_or(false) != bypass {
                    api_bypass.insert_toggle_bypass(snapshot.index, slot);
                }
            }
        });

        let api_send = Arc::clone(&self.api);
        let map_send = snapshots.clone();
        callbacks.configure_send = Box::new(move |channel_id, send_id, level| {
            if let Some(snapshot) = map_send.get(&channel_id) {
                let target_db = gain_to_db(level).clamp(-60.0, 6.0);
                let previous = snapshot
                    .send_levels_db
                    .get(send_id as usize)
                    .copied()
                    .unwrap_or(f32::NEG_INFINITY);
                if (target_db - previous).abs() > 0.1 {
                    api_send.send_set_level(snapshot.index, send_id as usize, target_db);
                }
            }
        });

        callbacks.open_insert_browser = Box::new(|channel_id, slot| {
            info!(?channel_id, slot, "open_insert_browser");
        });

        callbacks.open_insert_ui = Box::new(|channel_id, slot| {
            info!(?channel_id, slot, "open_insert_ui");
        });

        callbacks.remove_insert = Box::new(|channel_id, slot| {
            warn!(?channel_id, slot, "remove_insert_unimplemented");
        });

        callbacks
    }
}

fn db_to_linear(db: f32) -> f32 {
    if db.is_finite() {
        (10.0f32).powf(db * 0.05)
    } else {
        0.0
    }
}
