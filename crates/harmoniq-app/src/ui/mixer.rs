use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "mixer_api")]
use crossbeam_channel::Sender as MixerCommandSender;
use eframe::egui::Ui;
use harmoniq_engine::mixer::api::{MixerUiApi, UiStripInfo};
#[cfg(feature = "mixer_api")]
use harmoniq_engine::{GuiMeterReceiver, MixerCommand};
use harmoniq_mixer::state::{
    AutomationMode, Channel, ChannelEq, ChannelId, ChannelRackState, ChannelStripModules,
    InsertPosition, InsertSlot, Meter, MixerState, PanLaw, RoutingDelta, SendSlot,
};
use harmoniq_mixer::ui::{db_to_gain, gain_to_db};
use harmoniq_mixer::{MixerCallbacks, MixerProps};
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
                event.clip_l,
                event.clip_r,
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

    pub fn zoom_in(&mut self) {}

    pub fn zoom_out(&mut self) {}

    pub fn cpu_estimate(&self) -> f32 {
        self.master_cpu
    }

    pub fn master_meter(&self) -> (f32, f32) {
        self.master_meter_db
    }

    pub fn ui(&mut self, ui: &mut Ui, palette: &HarmoniqPalette) {
        let snapshots = self.sync_from_api();
        let mut callbacks = self.build_callbacks(&snapshots);

        harmoniq_mixer::render(
            ui,
            MixerProps {
                state: &mut self.state,
                callbacks: &mut callbacks,
                palette,
            },
        );
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
        self.master_cpu = 0.0;
        self.master_meter_db = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let previous_selection = self.state.selected;
        let mut previous_meters: HashMap<ChannelId, (Meter, VecDeque<f32>)> = self
            .state
            .channels
            .iter()
            .map(|channel| {
                (
                    channel.id,
                    (channel.meter.clone(), channel.meter_history.clone()),
                )
            })
            .collect();
        self.state.channels.clear();

        let mut has_master = false;
        for idx in 0..total {
            let info = self.api.strip_info(idx);
            let snapshot = self.populate_channel(idx, &info, previous_meters.remove(&info.id));
            snapshots.insert(info.id, snapshot);
            has_master |= info.is_master;
        }

        if !has_master {
            const MASTER_CHANNEL_ID: ChannelId = 10_000;
            let (meter, history) = previous_meters
                .remove(&MASTER_CHANNEL_ID)
                .unwrap_or_else(|| (Meter::default(), Channel::new_meter_history()));
            let channel = Channel {
                id: MASTER_CHANNEL_ID,
                track_number: 0,
                name: "MASTER".to_string(),
                gain_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
                inserts: Vec::new(),
                sends: Vec::new(),
                meter,
                meter_history: history,
                is_master: true,
                visible: true,
                color: [252, 200, 64],
                input_bus: "MixBus".into(),
                output_bus: "Main Out".into(),
                record_enable: false,
                monitor_enable: false,
                phase_invert: false,
                automation: AutomationMode::default(),
                pre_gain_db: 0.0,
                low_cut_hz: 20.0,
                high_cut_hz: 20_000.0,
                quick_controls: [0.5; 8],
                cue_sends: Vec::new(),
                eq: ChannelEq::default(),
                strip_modules: ChannelStripModules::default(),
                rack_state: ChannelRackState::default(),
                inserts_delay_comp: 0,
                pan_law: PanLaw::default(),
                stereo_separation: 1.0,
            };
            self.state.channels.push(channel);
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
        previous_state: Option<(Meter, VecDeque<f32>)>,
    ) -> StripSnapshot {
        let mut channel = Channel {
            id: info.id,
            track_number: if info.is_master {
                0
            } else {
                (idx as u16).saturating_add(1)
            },
            name: info.name.clone(),
            gain_db: info.fader_db,
            pan: info.pan,
            mute: info.muted,
            solo: info.soloed,
            inserts: Vec::with_capacity(info.insert_count),
            sends: Vec::with_capacity(info.send_count),
            meter: Meter::default(),
            meter_history: previous_state
                .as_ref()
                .map(|(_, history)| history.clone())
                .unwrap_or_else(Channel::new_meter_history),
            is_master: info.is_master,
            visible: true,
            color: [
                (info.color_rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                (info.color_rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                (info.color_rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            ],
            input_bus: format!("Input {}", idx + 1),
            output_bus: info.route_target.clone(),
            record_enable: info.armed,
            monitor_enable: false,
            phase_invert: info.phase_invert,
            automation: AutomationMode::default(),
            pre_gain_db: 0.0,
            low_cut_hz: 20.0,
            high_cut_hz: 20_000.0,
            quick_controls: [0.5; 8],
            cue_sends: Vec::new(),
            eq: ChannelEq::default(),
            strip_modules: ChannelStripModules::default(),
            rack_state: ChannelRackState::default(),
            inserts_delay_comp: info.latency_samples,
            pan_law: PanLaw::default(),
            stereo_separation: 1.0,
        };

        let mut insert_bypass = Vec::with_capacity(info.insert_count);
        for slot in 0..info.insert_count {
            let bypass = self.api.insert_is_bypassed(idx, slot);
            let label = self.api.insert_label(idx, slot);
            channel.inserts.push(InsertSlot {
                name: label,
                bypass,
                plugin_uid: None,
                format: None,
                position: InsertPosition::default(),
                sidechains: Vec::new(),
                delay_comp_samples: 0,
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
                pre_fader: self.api.send_is_pre(idx, slot),
                target: Some(self.api.send_label(idx, slot)),
            });
            send_levels_db.push(level_db);
        }

        let mut meter = previous_state.map(|(meter, _)| meter).unwrap_or_default();
        #[cfg(feature = "mixer_api")]
        let use_engine_meters = self.engine.is_some();
        #[cfg(not(feature = "mixer_api"))]
        let use_engine_meters = false;
        let mut master_peak_db = None;
        if !use_engine_meters {
            let (peak_l_db, peak_r_db, rms_l_db, rms_r_db, clipped) = self.api.level_fetch(idx);
            meter.peak_l = db_to_linear(peak_l_db);
            meter.peak_r = db_to_linear(peak_r_db);
            meter.rms_l = db_to_linear(rms_l_db);
            meter.rms_r = db_to_linear(rms_r_db);
            meter.peak_hold_l = meter.peak_l;
            meter.peak_hold_r = meter.peak_r;
            meter.clip_l = clipped;
            meter.clip_r = clipped;
            meter.last_update = Instant::now();
            master_peak_db = Some((peak_l_db, peak_r_db));
        } else if info.is_master {
            master_peak_db = Some((
                gain_to_db(meter.peak_l).clamp(-120.0, 6.0),
                gain_to_db(meter.peak_r).clamp(-120.0, 6.0),
            ));
        }
        channel.meter = meter;

        if info.is_master {
            self.master_cpu = info.cpu_percent;
            if let Some(peak_db) = master_peak_db {
                self.master_meter_db = peak_db;
            }
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

        let api_reorder = Arc::clone(&self.api);
        let map_reorder = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_reorder = engine_sender.clone();
        callbacks.reorder_insert = Box::new(move |channel_id, from, to| {
            if let Some(snapshot) = map_reorder.get(&channel_id) {
                api_reorder.insert_move(snapshot.index, from, to);
            }
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_reorder {
                let _ = tx.send(MixerCommand::ReorderInsert {
                    ch: channel_id,
                    from,
                    to,
                });
            }
        });

        let api_send = Arc::clone(&self.api);
        let map_send = snapshots.clone();
        #[cfg(feature = "mixer_api")]
        let tx_send = engine_sender.clone();
        callbacks.configure_send = Box::new(move |channel_id, send_id, level, _pre_fader| {
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
                    pre_fader: _pre_fader,
                });
            }
        });

        #[cfg(feature = "mixer_api")]
        let tx_stereo = engine_sender.clone();
        callbacks.set_stereo_separation = Box::new(move |_channel_id, _amount| {
            #[cfg(feature = "mixer_api")]
            if let Some(tx) = &tx_stereo {
                let _ = tx.send(MixerCommand::SetStereoSeparation {
                    ch: _channel_id,
                    amount: _amount,
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
        let tx_remove_insert = engine_sender.clone();
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

        #[cfg(feature = "mixer_api")]
        let tx_routing = engine_sender;
        callbacks.apply_routing = Box::new(move |delta: RoutingDelta| {
            #[cfg(feature = "mixer_api")]
            {
                if let Some(tx) = &tx_routing {
                    let cmd = MixerCommand::ApplyRouting {
                        set: delta.set.clone(),
                        remove: delta.remove.clone(),
                    };
                    let _ = tx.send(cmd);
                }
            }

            #[cfg(not(feature = "mixer_api"))]
            let _ = delta;
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
