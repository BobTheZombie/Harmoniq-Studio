use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DEFAULT_TRACK_COLORS: &[[f32; 4]] = &[
    [0.24, 0.54, 0.95, 1.0],
    [0.60, 0.36, 0.92, 1.0],
    [0.93, 0.47, 0.38, 1.0],
    [0.32, 0.75, 0.57, 1.0],
    [0.91, 0.76, 0.35, 1.0],
];

/// Identifier for an audio source that can be shown in the playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AudioSourceId(u64);

impl AudioSourceId {
    pub fn from_path(path: &Path) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Identifier for a playlist track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub u32);

impl TrackId {
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Identifier for a clip instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipId(pub u64);

/// Kind of clip stored in the playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipKind {
    Pattern { pattern_id: u32 },
    Audio { source: AudioSourceId },
    Automation,
}

/// Concrete clip instance positioned in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    pub start_ticks: u64,
    pub duration_ticks: u64,
    pub color: [f32; 4],
    pub kind: ClipKind,
}

impl Clip {
    pub fn end_ticks(&self) -> u64 {
        self.start_ticks + self.duration_ticks
    }
}

/// A clip lane inside a track timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackLane {
    pub id: u32,
    pub name: String,
    pub clips: Vec<Clip>,
}

impl TrackLane {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
        self.clips.sort_by_key(|clip| clip.start_ticks);
    }

    pub fn take_clip(&mut self, clip_id: ClipId) -> Option<Clip> {
        if let Some(index) = self.clips.iter().position(|clip| clip.id == clip_id) {
            Some(self.clips.remove(index))
        } else {
            None
        }
    }
}

/// Type of slot rendered inside the track rack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RackSlotKind {
    Instrument,
    Insert,
    Send,
    Midi,
}

impl RackSlotKind {
    pub fn label(self) -> &'static str {
        match self {
            RackSlotKind::Instrument => "Instrument",
            RackSlotKind::Insert => "Insert",
            RackSlotKind::Send => "Send",
            RackSlotKind::Midi => "MIDI",
        }
    }
}

/// A single module shown in the Cubase-style rack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackSlot {
    pub id: u32,
    pub name: String,
    pub kind: RackSlotKind,
    pub active: bool,
}

impl RackSlot {
    pub fn new(id: u32, name: impl Into<String>, kind: RackSlotKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            active: true,
        }
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
    }
}

/// Mixer-like controls available on each track header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackControls {
    pub record_arm: bool,
    pub solo: bool,
    pub mute: bool,
    pub monitor: bool,
}

impl Default for TrackControls {
    fn default() -> Self {
        Self {
            record_arm: false,
            solo: false,
            mute: false,
            monitor: true,
        }
    }
}

impl TrackControls {
    pub fn toggle_record_arm(&mut self) {
        self.record_arm = !self.record_arm;
    }

    pub fn toggle_solo(&mut self) {
        self.solo = !self.solo;
    }

    pub fn toggle_mute(&mut self) {
        self.mute = !self.mute;
    }

    pub fn toggle_monitor(&mut self) {
        self.monitor = !self.monitor;
    }
}

/// Track within the playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub color: [f32; 4],
    pub controls: TrackControls,
    pub rack: Vec<RackSlot>,
    pub lanes: Vec<TrackLane>,
}

impl Track {
    pub fn new(id: TrackId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color: DEFAULT_TRACK_COLORS[id.as_usize() % DEFAULT_TRACK_COLORS.len()],
            controls: TrackControls::default(),
            rack: Vec::new(),
            lanes: Vec::new(),
        }
    }

    pub fn add_lane(&mut self, lane: TrackLane) {
        self.lanes.push(lane);
    }

    pub fn lane_mut(&mut self, lane_id: u32) -> Option<&mut TrackLane> {
        self.lanes.iter_mut().find(|lane| lane.id == lane_id)
    }

    pub fn lane(&self, lane_id: u32) -> Option<&TrackLane> {
        self.lanes.iter().find(|lane| lane.id == lane_id)
    }

    pub fn add_clip(&mut self, clip: Clip) {
        if let Some(lane) = self.lanes.first_mut() {
            lane.add_clip(clip);
        }
    }

    pub fn add_clip_to_lane(&mut self, lane_id: u32, clip: Clip) {
        if let Some(lane) = self.lane_mut(lane_id) {
            lane.add_clip(clip);
        }
    }

    pub fn rack_slot_mut(&mut self, slot_id: u32) -> Option<&mut RackSlot> {
        self.rack.iter_mut().find(|slot| slot.id == slot_id)
    }
}

/// Snap resolution for drawing and editing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Snap {
    N1_1,
    N1_2,
    N1_4,
    N1_8,
    N1_16,
    N1_32,
    N1_64,
}

impl Snap {
    pub fn label(self) -> &'static str {
        match self {
            Snap::N1_1 => "1", // whole note
            Snap::N1_2 => "1/2",
            Snap::N1_4 => "1/4",
            Snap::N1_8 => "1/8",
            Snap::N1_16 => "1/16",
            Snap::N1_32 => "1/32",
            Snap::N1_64 => "1/64",
        }
    }

    pub fn division(self) -> u32 {
        match self {
            Snap::N1_1 => 1,
            Snap::N1_2 => 2,
            Snap::N1_4 => 4,
            Snap::N1_8 => 8,
            Snap::N1_16 => 16,
            Snap::N1_32 => 32,
            Snap::N1_64 => 64,
        }
    }
}

/// Selected clip information returned to the host application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedClip {
    pub track: TrackId,
    pub clip: ClipId,
    pub track_name: String,
    pub clip_name: String,
    pub start_ticks: u64,
    pub duration_ticks: u64,
    pub color: [f32; 4],
}

/// In-memory playlist state shared with the UI renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub ppq: u32,
    pub tracks: Vec<Track>,
    pub selection: Option<(TrackId, ClipId)>,
    pub dropped_files: Vec<PathBuf>,
}

impl Playlist {
    pub fn new_default(ppq: u32) -> Self {
        let mut playlist = Self {
            ppq,
            tracks: Vec::new(),
            selection: None,
            dropped_files: Vec::new(),
        };
        playlist.seed_demo_content();
        playlist
    }

    pub fn ppq(&self) -> u32 {
        self.ppq
    }

    pub fn set_ppq(&mut self, ppq: u32) {
        self.ppq = ppq.max(1);
    }

    pub fn selected_clip(&self) -> Option<SelectedClip> {
        let (track_id, clip_id) = self.selection?;
        let track = self.tracks.iter().find(|track| track.id == track_id)?;
        let (lane_name, clip) = track.lanes.iter().find_map(|lane| {
            lane.clips
                .iter()
                .find(|clip| clip.id == clip_id)
                .map(|clip| (&lane.name, clip))
        })?;
        Some(SelectedClip {
            track: track.id,
            clip: clip.id,
            track_name: track.name.clone(),
            clip_name: format!("{} ({lane_name})", clip.name),
            start_ticks: clip.start_ticks,
            duration_ticks: clip.duration_ticks,
            color: clip.color,
        })
    }

    pub fn set_selection(&mut self, track: TrackId, clip: ClipId) {
        self.selection = Some((track, clip));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn register_drop(&mut self, path: PathBuf) {
        self.dropped_files.push(path);
    }

    pub fn take_dropped_files(&mut self) -> impl Iterator<Item = PathBuf> + '_ {
        self.dropped_files.drain(..)
    }

    fn seed_demo_content(&mut self) {
        if !self.tracks.is_empty() {
            return;
        }
        let ppq = self.ppq as u64;
        for (index, name) in ["Drums", "Bass", "Keys", "Lead"].into_iter().enumerate() {
            let mut track = Track::new(TrackId(index as u32), name);
            track.add_lane(TrackLane::new(0, "Main Lane"));
            track.add_lane(TrackLane::new(1, "Automation"));
            track.add_clip_to_lane(
                0,
                Clip {
                    id: ClipId((index as u64) * 10 + 1),
                    name: format!("{} Pattern", name),
                    start_ticks: 0,
                    duration_ticks: 4 * ppq,
                    color: track.color,
                    kind: ClipKind::Pattern {
                        pattern_id: (index as u32) + 1,
                    },
                },
            );
            track.add_clip_to_lane(
                0,
                Clip {
                    id: ClipId((index as u64) * 10 + 2),
                    name: format!("{} Variation", name),
                    start_ticks: 6 * ppq,
                    duration_ticks: 4 * ppq,
                    color: track.color,
                    kind: ClipKind::Pattern {
                        pattern_id: (index as u32) + 101,
                    },
                },
            );
            track.add_clip_to_lane(
                1,
                Clip {
                    id: ClipId((index as u64) * 10 + 3),
                    name: "Filter Sweep".into(),
                    start_ticks: ppq,
                    duration_ticks: 5 * ppq,
                    color: [track.color[0], track.color[1], track.color[2], 0.7],
                    kind: ClipKind::Automation,
                },
            );
            track.rack.push(RackSlot::new(
                0,
                format!("{} Instrument", name),
                RackSlotKind::Instrument,
            ));
            track
                .rack
                .push(RackSlot::new(1, "Compressor", RackSlotKind::Insert));
            track
                .rack
                .push(RackSlot::new(2, "Delay", RackSlotKind::Send));
            self.tracks.push(track);
        }
    }
}
