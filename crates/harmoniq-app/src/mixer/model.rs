use std::ops::RangeInclusive;

use egui::Color32;
use serde::{Deserialize, Serialize};

use super::rt_api::SEND_COUNT;

pub type ChannelId = u32;
pub type PluginId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MixerIcon {
    None,
    Wave,
    Guitar,
    Piano,
    Mic,
    Bus,
}

impl Default for MixerIcon {
    fn default() -> Self {
        MixerIcon::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertSlot {
    pub plugin_id: Option<PluginId>,
    pub bypass: bool,
    pub post_fader: bool,
}

impl Default for InsertSlot {
    fn default() -> Self {
        InsertSlot {
            plugin_id: None,
            bypass: false,
            post_fader: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelView {
    pub id: ChannelId,
    pub name: String,
    pub color: Color32,
    pub icon: MixerIcon,
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub phase_invert: bool,
    pub mono: bool,
    pub stereo_link: bool,
    pub record_arm: bool,
    pub inserts: [InsertSlot; 10],
    pub latency_samples: u32,
    pub meter_peak_l: f32,
    pub meter_peak_r: f32,
    pub meter_rms_l: f32,
    pub meter_rms_r: f32,
}

impl ChannelView {
    pub fn new(id: ChannelId, name: impl Into<String>, color: Color32) -> Self {
        ChannelView {
            id,
            name: name.into(),
            color,
            icon: MixerIcon::None,
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            phase_invert: false,
            mono: false,
            stereo_link: false,
            record_arm: false,
            inserts: Default::default(),
            latency_samples: 0,
            meter_peak_l: f32::NEG_INFINITY,
            meter_peak_r: f32::NEG_INFINITY,
            meter_rms_l: f32::NEG_INFINITY,
            meter_rms_r: f32::NEG_INFINITY,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendView {
    pub channel: ChannelView,
    pub send_gain_db: f32,
    pub pre_fader: bool,
}

impl SendView {
    pub fn new(id: ChannelId, name: impl Into<String>, color: Color32) -> Self {
        SendView {
            channel: ChannelView::new(id, name, color),
            send_gain_db: 0.0,
            pre_fader: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCell {
    pub enabled: bool,
    pub send_gain_db: f32,
}

impl Default for RouteCell {
    fn default() -> Self {
        RouteCell {
            enabled: false,
            send_gain_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingMatrix {
    pub routes: Vec<Vec<RouteCell>>,
}

impl RoutingMatrix {
    pub fn new(sources: usize, destinations: usize) -> Self {
        RoutingMatrix {
            routes: vec![vec![RouteCell::default(); destinations]; sources],
        }
    }

    pub fn ensure_dimensions(&mut self, sources: usize, destinations: usize) {
        if self.routes.len() != sources {
            self.routes
                .resize(sources, vec![RouteCell::default(); destinations]);
        }
        for row in &mut self.routes {
            if row.len() != destinations {
                row.resize(destinations, RouteCell::default());
            }
        }
    }

    pub fn route_mut(&mut self, src: usize, dst: usize) -> Option<&mut RouteCell> {
        self.routes.get_mut(src).and_then(|row| row.get_mut(dst))
    }

    pub fn route(&self, src: usize, dst: usize) -> Option<&RouteCell> {
        self.routes.get(src).and_then(|row| row.get(dst))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerMetrics {
    pub channel_width: f32,
    pub channel_height: f32,
    pub meter_width: f32,
    pub fader_range: RangeInclusive<f32>,
}

impl Default for MixerMetrics {
    fn default() -> Self {
        MixerMetrics {
            channel_width: 92.0,
            channel_height: 520.0,
            meter_width: 18.0,
            fader_range: -60.0..=12.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerTheme {
    pub background: Color32,
    pub strip_bg: Color32,
    pub strip_selected: Color32,
    pub strip_border: Color32,
    pub meter_peak: Color32,
    pub meter_rms: Color32,
    pub meter_hold: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub metrics: MixerMetrics,
}

impl MixerTheme {
    pub fn dark() -> Self {
        MixerTheme {
            background: Color32::from_rgb(22, 22, 24),
            strip_bg: Color32::from_rgb(32, 34, 38),
            strip_selected: Color32::from_rgb(52, 86, 142),
            strip_border: Color32::from_rgba_unmultiplied(0, 0, 0, 160),
            meter_peak: Color32::from_rgb(31, 208, 117),
            meter_rms: Color32::from_rgb(16, 139, 173),
            meter_hold: Color32::from_rgb(255, 196, 0),
            text_primary: Color32::from_rgb(240, 240, 240),
            text_secondary: Color32::from_rgba_unmultiplied(200, 200, 200, 160),
            accent: Color32::from_rgb(255, 118, 38),
            metrics: MixerMetrics::default(),
        }
    }

    pub fn graphite() -> Self {
        MixerTheme {
            background: Color32::from_rgb(26, 28, 30),
            strip_bg: Color32::from_rgb(38, 40, 44),
            strip_selected: Color32::from_rgb(90, 94, 104),
            strip_border: Color32::from_rgba_unmultiplied(0, 0, 0, 190),
            meter_peak: Color32::from_rgb(96, 214, 168),
            meter_rms: Color32::from_rgb(88, 136, 203),
            meter_hold: Color32::from_rgb(255, 210, 64),
            text_primary: Color32::from_rgb(226, 226, 226),
            text_secondary: Color32::from_rgba_unmultiplied(200, 200, 200, 120),
            accent: Color32::from_rgb(255, 150, 66),
            metrics: MixerMetrics::default(),
        }
    }

    pub fn colorful() -> Self {
        MixerTheme {
            background: Color32::from_rgb(12, 16, 22),
            strip_bg: Color32::from_rgb(26, 33, 42),
            strip_selected: Color32::from_rgb(0, 125, 196),
            strip_border: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            meter_peak: Color32::from_rgb(6, 247, 160),
            meter_rms: Color32::from_rgb(3, 168, 231),
            meter_hold: Color32::from_rgb(255, 196, 0),
            text_primary: Color32::from_rgb(234, 236, 255),
            text_secondary: Color32::from_rgba_unmultiplied(180, 200, 220, 140),
            accent: Color32::from_rgb(255, 108, 92),
            metrics: MixerMetrics::default(),
        }
    }
}

impl Default for MixerTheme {
    fn default() -> Self {
        MixerTheme::dark()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertDragState {
    pub channel: usize,
    pub slot: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerView {
    pub channels: Vec<ChannelView>,
    pub sends: [SendView; SEND_COUNT],
    pub master: ChannelView,
    pub routing: RoutingMatrix,
    pub theme: MixerTheme,
    pub selection: Option<usize>,
    pub drag_insert: Option<InsertDragState>,
    pub selected_insert: Option<(usize, usize)>,
}

impl MixerView {
    pub fn new(num_channels: usize) -> Self {
        let theme = MixerTheme::dark();
        let mut channels = Vec::with_capacity(num_channels);
        for i in 0..num_channels {
            let color = Color32::from_rgb(60, 80 + (i as u8 * 7) % 120, 96 + (i as u8 * 11) % 120);
            channels.push(ChannelView::new(
                i as ChannelId,
                format!("Track {i}"),
                color,
            ));
        }

        let mut sends: Vec<SendView> = (0..SEND_COUNT)
            .map(|i| {
                let color = Color32::from_rgb(52, 92, 132 + (i as u8 * 30));
                SendView::new(
                    (num_channels + i) as ChannelId,
                    format!("Send {}", i + 1),
                    color,
                )
            })
            .collect();

        let master = ChannelView::new(
            (num_channels + SEND_COUNT) as ChannelId,
            "Master",
            Color32::from_rgb(96, 110, 160),
        );

        let mut routing =
            RoutingMatrix::new(num_channels + SEND_COUNT, num_channels + SEND_COUNT + 1);
        for src in 0..num_channels {
            if let Some(cell) = routing.route_mut(src, num_channels + SEND_COUNT) {
                cell.enabled = true;
            }
        }
        for send_idx in 0..SEND_COUNT {
            let src_index = num_channels + send_idx;
            if let Some(cell) = routing.route_mut(src_index, num_channels + SEND_COUNT) {
                cell.enabled = true;
            }
        }

        MixerView {
            channels,
            sends: [
                sends.remove(0),
                sends.remove(0),
                sends.remove(0),
                sends.remove(0),
            ],
            master,
            routing,
            theme,
            selection: None,
            drag_insert: None,
            selected_insert: None,
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: &crate::mixer::rt_api::MixerStateSnapshot) {
        for (idx, channel) in self.channels.iter_mut().enumerate() {
            if let Some((l, r)) = snapshot.meter_peak_lr.get(idx) {
                channel.meter_peak_l = *l;
                channel.meter_peak_r = *r;
            }
            if let Some((l, r)) = snapshot.meter_rms_lr.get(idx) {
                channel.meter_rms_l = *l;
                channel.meter_rms_r = *r;
            }
            if let Some(lat) = snapshot.latencies.get(idx) {
                channel.latency_samples = *lat;
            }
        }

        let send_offset = self.channels.len();
        for (idx, send) in self.sends.iter_mut().enumerate() {
            if let Some((l, r)) = snapshot.meter_peak_lr.get(send_offset + idx) {
                send.channel.meter_peak_l = *l;
                send.channel.meter_peak_r = *r;
            }
            if let Some((l, r)) = snapshot.meter_rms_lr.get(send_offset + idx) {
                send.channel.meter_rms_l = *l;
                send.channel.meter_rms_r = *r;
            }
            if let Some(lat) = snapshot.latencies.get(send_offset + idx) {
                send.channel.latency_samples = *lat;
            }
        }

        let master_index = self.channels.len() + self.sends.len();
        if let Some((l, r)) = snapshot.meter_peak_lr.get(master_index) {
            self.master.meter_peak_l = *l;
            self.master.meter_peak_r = *r;
        }
        if let Some((l, r)) = snapshot.meter_rms_lr.get(master_index) {
            self.master.meter_rms_l = *l;
            self.master.meter_rms_r = *r;
        }
        if let Some(lat) = snapshot.latencies.get(master_index) {
            self.master.latency_samples = *lat;
        }
    }
}
