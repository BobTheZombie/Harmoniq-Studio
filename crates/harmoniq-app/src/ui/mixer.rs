use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "mixer_api")]
use crossbeam_channel::Sender as MixerCommandSender;

use eframe::egui::Ui;
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};
#[cfg(feature = "mixer_api")]
use harmoniq_engine::{GuiMeterReceiver, MixerCommand};
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

#[cfg(feature = "mixer_api")]
#[derive(Clone)]
pub struct MixerEngineBridge {
    sender: MixerCommandSender<MixerCommand>,
    meter_rx: GuiMeterReceiver,
}

#[cfg(feature = "mixer_api")]
impl MixerEngineBridge {
    pub fn new(sender: MixerCommandSender<MixerCommand>, meter_rx: GuiMeterReceiver) -> Self {
        Self { sender, meter_rx }
    }

    pub fn sender(&self) -> MixerCommandSender<MixerCommand> {
        self.sender.clone()
    }

    pub fn poll(&self, state: &mut MixerState) -> bool {
        let mut updated = false;
        self.meter_rx.drain(|event| {
            state.update_meter(
                event.ch,
                event.peak_l,
                event.peak_r,
                event.rms_l,
                event.rms_r,
            );
            updated = true;
        });
        updated
    }
}

pub struct MixerView {
    api: Arc<dyn MixerUiApi>,
    state: MixerState,
    master_cpu: f32,
    master_meter_db: (f32, f32),
    #[cfg(feature = "mixer_api")]
    engine: Option<MixerEngineBridge>,
}

impl MixerView {
    #[cfg(feature = "mixer_api")]
    pub fn new(api: Arc<dyn MixerUiApi>, engine: Option<MixerEngineBridge>) -> Self {
        Self {
            api,
            state: MixerState::default(),
            master_cpu: 0.0,
            master_meter_db: (f32::NEG_INFINITY, f32::NEG_INFINITY),
            engine,
        }
    }

    #[cfg(not(feature = "mixer_api"))]
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

    #[cfg(feature = "mixer_api")]
    pub fn poll_engine(&mut self) -> bool {
        if let Some(engine) = &self.engine {
            engine.poll(&mut self.state)
        } else {
            false
        }
    }

    #[cfg(not(feature = "mixer_api"))]
    pub fn poll_engine(&mut self) -> bool {
        false
    }

    fn sync_from_api(&mut self) -> HashMap<ChannelId, StripSnapshot> {
        let total = self.api.strips_len();
        let mut snapshots = HashMap::with_capacity(total);
        let previous_selection = self.state.selected;
        let mut previous_meters: HashMap<ChannelId, Meter> = self
            .state
            .channels
            .iter()
            .map(|channel| (channel.id, channel.meter.clone()))
            .collect();
        self.state.channels.clear();

        for idx in 0..total {
            let info = self.api.strip_info(idx);
            let snapshot = self.populate_channel(idx, &info, previous_meters.remove(&info.id));
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

    fn populate_channel(
        &mut self,
        idx: usize,
        info: &UiStripInfo,
        previous_meter: Option<Meter>,
    ) -> StripSnapshot {
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

        let mut meter = previous_meter.unwrap_or_default();
        #[cfg(feature = "mixer_api")]
        let use_engine_meters = self.engine.is_some();
        #[cfg(not(feature = "mixer_api"))]
        let use_engine_meters = false;
        let mut master_peak_db = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        if !use_engine_meters {
            let (peak_l_db, peak_r_db, rms_l_db, rms_r_db, _clipped) = self.api.level_fetch(idx);
            meter.peak_l = db_to_linear(peak_l_db);
            meter.peak_r = db_to_linear(peak_r_db);
            meter.rms_l = db_to_linear(rms_l_db);
            meter.rms_r = db_to_linear(rms_r_db);
            meter.peak_hold_l = meter.peak_l;
            meter.peak_hold_r = meter.peak_r;
            meter.last_update = Instant::now();
            master_peak_db = (peak_l_db, peak_r_db);
        } else if info.is_master {
            master_peak_db = (
                gain_to_db(meter.peak_l).clamp(-120.0, 6.0),
                gain_to_db(meter.peak_r).clamp(-120.0, 6.0),
            );
        }
        channel.meter = meter;

        if info.is_master {
            self.master_cpu = info.cpu_percent;
            self.master_meter_db = master_peak_db;
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

        #[cfg(feature = "mixer_api")]
        let engine_sender = self.engine.as_ref().map(|bridge| bridge.sender());

        let api_gain = Arc::clone(&self.api);
        let map_gain = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_gain = engine_sender.clone();
        callbacks.set_gain_pan = Box::new(move |channel_id, db, pan| {
            if let Some(snapshot) = map_gain.get(&channel_id) {
                api_gain.set_fader_db(snapshot.index, db);
                api_gain.set_pan(snapshot.index, pan);
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_gain {
                let _ = tx.send(MixerCommand::SetGainPan {
                    ch: channel_id,
                    gain_db: db,
                    pan,
                });
            }
        });

        let api_mute = Arc::clone(&self.api);
        let map_mute = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_mute = engine_sender.clone();
        callbacks.set_mute = Box::new(move |channel_id, mute| {
            if let Some(snapshot) = map_mute.get(&channel_id) {
                if snapshot.mute != mute {
                    api_mute.toggle_mute(snapshot.index);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_mute {
                let _ = tx.send(MixerCommand::SetMute {
                    ch: channel_id,
                    mute,
                });
            }
        });

        let api_solo = Arc::clone(&self.api);
        let map_solo = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_solo = engine_sender.clone();
        callbacks.set_solo = Box::new(move |channel_id, solo| {
            if let Some(snapshot) = map_solo.get(&channel_id) {
                if snapshot.solo != solo {
                    api_solo.toggle_solo(snapshot.index);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_solo {
                let _ = tx.send(MixerCommand::SetSolo {
                    ch: channel_id,
                    solo,
                });
            }
        });

        let api_bypass = Arc::clone(&self.api);
        let map_bypass = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_bypass = engine_sender.clone();
        callbacks.set_insert_bypass = Box::new(move |channel_id, slot, bypass| {
            if let Some(snapshot) = map_bypass.get(&channel_id) {
                if snapshot.insert_bypass.get(slot).copied().unwrap_or(false) != bypass {
                    api_bypass.insert_toggle_bypass(snapshot.index, slot);
                }
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_bypass {
                let _ = tx.send(MixerCommand::SetInsertBypass {
                    ch: channel_id,
                    slot,
                    bypass,
                });
            }
        });

        let api_send = Arc::clone(&self.api);
        let map_send = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_send = engine_sender.clone();
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
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_send {
                let _ = tx.send(MixerCommand::ConfigureSend {
                    ch: channel_id,
                    id: send_id,
                    level,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_insert_browser = engine_sender.clone();
        callbacks.open_insert_browser = Box::new(move |channel_id, slot| {
            info!(?channel_id, slot, "open_insert_browser");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_insert_browser {
                let _ = tx.send(MixerCommand::OpenInsertBrowser {
                    ch: channel_id,
                    slot,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_insert_ui = engine_sender.clone();
        callbacks.open_insert_ui = Box::new(move |channel_id, slot| {
            info!(?channel_id, slot, "open_insert_ui");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_insert_ui {
                let _ = tx.send(MixerCommand::OpenInsertUi {
                    ch: channel_id,
                    slot,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_remove_insert = engine_sender;
        callbacks.remove_insert = Box::new(move |channel_id, slot| {
            warn!(?channel_id, slot, "remove_insert_unimplemented");
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_remove_insert {
                let _ = tx.send(MixerCommand::RemoveInsert {
                    ch: channel_id,
                    slot,
                });
            }
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
