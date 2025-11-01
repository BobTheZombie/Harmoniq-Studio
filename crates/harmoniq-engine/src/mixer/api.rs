use std::sync::Arc;

use parking_lot::RwLock;

use super::levels::MixerLevels;

#[derive(Debug, Clone)]
pub struct UiStripInfo {
    pub id: u32,
    pub name: String,
    pub color_rgba: [f32; 4],
    pub latency_samples: u32,
    pub cpu_percent: f32,
    pub pdc_active: bool,
    pub insert_count: usize,
    pub send_count: usize,
    pub muted: bool,
    pub soloed: bool,
    pub armed: bool,
    pub phase_invert: bool,
    pub fader_db: f32,
    pub pan: f32,
    pub width: f32,
    pub is_master: bool,
    pub route_target: String,
}

impl Default for UiStripInfo {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::from("Track"),
            color_rgba: [0.25, 0.25, 0.28, 1.0],
            latency_samples: 0,
            cpu_percent: 0.0,
            pdc_active: false,
            insert_count: 0,
            send_count: 0,
            muted: false,
            soloed: false,
            armed: false,
            phase_invert: false,
            fader_db: 0.0,
            pan: 0.0,
            width: 1.0,
            is_master: false,
            route_target: String::new(),
        }
    }
}

pub trait MixerUiApi: Send + Sync {
    fn strips_len(&self) -> usize;
    fn strip_info(&self, idx: usize) -> UiStripInfo;
    fn set_name(&self, idx: usize, name: &str);
    fn set_color(&self, idx: usize, rgba: [f32; 4]);
    fn set_fader_db(&self, idx: usize, db: f32);
    fn set_pan(&self, idx: usize, pan: f32);
    fn set_width(&self, idx: usize, width: f32);
    fn toggle_mute(&self, idx: usize);
    fn toggle_solo(&self, idx: usize);
    fn toggle_arm(&self, idx: usize);
    fn toggle_phase(&self, idx: usize);
    fn insert_label(&self, idx: usize, slot: usize) -> String;
    fn insert_toggle_bypass(&self, idx: usize, slot: usize);
    fn insert_is_bypassed(&self, idx: usize, slot: usize) -> bool;
    fn insert_move(&self, idx: usize, from: usize, to: usize);
    fn send_label(&self, idx: usize, slot: usize) -> String;
    fn send_level(&self, idx: usize, slot: usize) -> f32;
    fn send_set_level(&self, idx: usize, slot: usize, db: f32);
    fn send_toggle_pre(&self, idx: usize, slot: usize);
    fn send_is_pre(&self, idx: usize, slot: usize) -> bool;
    fn route_target_label(&self, idx: usize) -> String;
    fn set_route_target(&self, idx: usize, target: u32);
    fn level_fetch(&self, idx: usize) -> (f32, f32, f32, f32, bool);
}

#[derive(Debug)]
pub struct MixerUiState {
    strips: RwLock<Vec<StripState>>,
    levels: Arc<MixerLevels>,
}

#[derive(Debug, Clone)]
struct StripState {
    info: UiStripInfo,
    inserts: Vec<InsertState>,
    sends: Vec<SendState>,
}

#[derive(Debug, Clone)]
struct InsertState {
    label: String,
    bypassed: bool,
}

#[derive(Debug, Clone)]
struct SendState {
    label: String,
    pre: bool,
    level_db: f32,
}

impl MixerUiState {
    pub fn new(track_count: usize) -> Self {
        let mut strips = Vec::with_capacity(track_count);
        let mut id_counter = 1u32;
        for idx in 0..track_count {
            let is_master = idx == track_count - 1;
            let mut info = UiStripInfo::default();
            info.id = id_counter;
            id_counter += 1;
            info.name = if is_master {
                "Master".to_string()
            } else {
                format!("Track {idx:02}")
            };
            info.color_rgba = if is_master {
                [0.32, 0.32, 0.38, 1.0]
            } else {
                [0.25 + idx as f32 * 0.015, 0.2, 0.28, 1.0]
            };
            info.insert_count = 12;
            info.send_count = 6;
            info.is_master = is_master;
            info.cpu_percent = if is_master { 2.4 } else { 0.4 };
            info.latency_samples = if is_master { 0 } else { 128 };
            info.route_target = if is_master {
                "Output 1-2".to_string()
            } else {
                "BUS 1".to_string()
            };

            let inserts = (0..info.insert_count)
                .map(|slot| InsertState {
                    label: if slot == 0 {
                        "Channel EQ".into()
                    } else {
                        format!("Insert {slot}")
                    },
                    bypassed: slot % 3 == 0,
                })
                .collect();

            let sends = (0..info.send_count)
                .map(|slot| SendState {
                    label: format!("Bus {slot}"),
                    pre: slot % 2 == 0,
                    level_db: -12.0 + slot as f32 * 1.5,
                })
                .collect();

            strips.push(StripState {
                info,
                inserts,
                sends,
            });
        }

        let levels = Arc::new(MixerLevels::new(track_count));
        Self {
            strips: RwLock::new(strips),
            levels,
        }
    }

    pub fn levels(&self) -> Arc<MixerLevels> {
        Arc::clone(&self.levels)
    }
}

impl MixerUiApi for MixerUiState {
    fn strips_len(&self) -> usize {
        self.strips.read().len()
    }

    fn strip_info(&self, idx: usize) -> UiStripInfo {
        self.strips
            .read()
            .get(idx)
            .map(|state| {
                let mut info = state.info.clone();
                info.insert_count = state.inserts.len();
                info.send_count = state.sends.len();
                info
            })
            .unwrap_or_default()
    }

    fn set_name(&self, idx: usize, name: &str) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.name = name.to_string();
        }
    }

    fn set_color(&self, idx: usize, rgba: [f32; 4]) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.color_rgba = rgba;
        }
    }

    fn set_fader_db(&self, idx: usize, db: f32) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.fader_db = db.clamp(-90.0, 12.0);
        }
    }

    fn set_pan(&self, idx: usize, pan: f32) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.pan = pan.clamp(-1.0, 1.0);
        }
    }

    fn set_width(&self, idx: usize, width: f32) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.width = width.clamp(0.0, 2.0);
        }
    }

    fn toggle_mute(&self, idx: usize) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.muted = !strip.info.muted;
        }
    }

    fn toggle_solo(&self, idx: usize) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.soloed = !strip.info.soloed;
        }
    }

    fn toggle_arm(&self, idx: usize) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.armed = !strip.info.armed;
        }
    }

    fn toggle_phase(&self, idx: usize) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.phase_invert = !strip.info.phase_invert;
        }
    }

    fn insert_label(&self, idx: usize, slot: usize) -> String {
        self.strips
            .read()
            .get(idx)
            .and_then(|strip| strip.inserts.get(slot))
            .map(|slot| slot.label.clone())
            .unwrap_or_default()
    }

    fn insert_toggle_bypass(&self, idx: usize, slot: usize) {
        if let Some(slot) = self
            .strips
            .write()
            .get_mut(idx)
            .and_then(|strip| strip.inserts.get_mut(slot))
        {
            slot.bypassed = !slot.bypassed;
        }
    }

    fn insert_is_bypassed(&self, idx: usize, slot: usize) -> bool {
        self.strips
            .read()
            .get(idx)
            .and_then(|strip| strip.inserts.get(slot))
            .map(|slot| slot.bypassed)
            .unwrap_or(false)
    }

    fn insert_move(&self, idx: usize, from: usize, to: usize) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            if from < strip.inserts.len() && to < strip.inserts.len() && from != to {
                let item = strip.inserts.remove(from);
                strip.inserts.insert(to, item);
            }
        }
    }

    fn send_label(&self, idx: usize, slot: usize) -> String {
        self.strips
            .read()
            .get(idx)
            .and_then(|strip| strip.sends.get(slot))
            .map(|slot| slot.label.clone())
            .unwrap_or_default()
    }

    fn send_level(&self, idx: usize, slot: usize) -> f32 {
        self.strips
            .read()
            .get(idx)
            .and_then(|strip| strip.sends.get(slot))
            .map(|slot| slot.level_db)
            .unwrap_or(-60.0)
    }

    fn send_set_level(&self, idx: usize, slot: usize, db: f32) {
        if let Some(slot) = self
            .strips
            .write()
            .get_mut(idx)
            .and_then(|strip| strip.sends.get_mut(slot))
        {
            slot.level_db = db.clamp(-60.0, 6.0);
        }
    }

    fn send_toggle_pre(&self, idx: usize, slot: usize) {
        if let Some(slot) = self
            .strips
            .write()
            .get_mut(idx)
            .and_then(|strip| strip.sends.get_mut(slot))
        {
            slot.pre = !slot.pre;
        }
    }

    fn send_is_pre(&self, idx: usize, slot: usize) -> bool {
        self.strips
            .read()
            .get(idx)
            .and_then(|strip| strip.sends.get(slot))
            .map(|slot| slot.pre)
            .unwrap_or(false)
    }

    fn route_target_label(&self, idx: usize) -> String {
        self.strips
            .read()
            .get(idx)
            .map(|strip| strip.info.route_target.clone())
            .unwrap_or_default()
    }

    fn set_route_target(&self, idx: usize, target: u32) {
        if let Some(strip) = self.strips.write().get_mut(idx) {
            strip.info.route_target = format!("BUS {target}");
        }
    }

    fn level_fetch(&self, idx: usize) -> (f32, f32, f32, f32, bool) {
        self.levels.snapshot(idx)
    }
}

impl MixerUiState {
    pub fn demo() -> Arc<Self> {
        Arc::new(Self::new(33))
    }
}
