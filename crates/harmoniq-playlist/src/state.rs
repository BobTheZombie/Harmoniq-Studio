use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use rand::random;
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
    pub fade_in_ticks: u64,
    pub fade_out_ticks: u64,
    pub crossfade_with: Option<ClipId>,
    pub time_stretch_enabled: bool,
}

impl Clip {
    pub fn end_ticks(&self) -> u64 {
        self.start_ticks + self.duration_ticks
    }

    pub fn new(
        id: ClipId,
        name: impl Into<String>,
        start_ticks: u64,
        duration_ticks: u64,
        color: [f32; 4],
        kind: ClipKind,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            start_ticks,
            duration_ticks,
            color,
            kind,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            crossfade_with: None,
            time_stretch_enabled: false,
        }
    }
}

/// MIDI note data stored inside a pattern clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternNote {
    pub id: u64,
    pub start_ticks: i64,
    pub duration_ticks: i64,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
}

impl PatternNote {
    pub fn end_ticks(&self) -> i64 {
        self.start_ticks + self.duration_ticks.max(1)
    }
}

/// In-memory pattern backing a playlist clip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: u32,
    pub notes: Vec<PatternNote>,
}

impl Pattern {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            notes: Vec::new(),
        }
    }

    pub fn set_notes(&mut self, mut notes: Vec<PatternNote>) {
        notes.sort_by_key(|note| (note.start_ticks, note.pitch));
        self.notes = notes;
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
    pub patterns: HashMap<u32, Pattern>,
}

impl Playlist {
    pub fn new_default(ppq: u32) -> Self {
        let mut playlist = Self {
            ppq,
            tracks: Vec::new(),
            selection: None,
            dropped_files: Vec::new(),
            patterns: HashMap::new(),
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

    pub fn clip(&self, track: TrackId, clip_id: ClipId) -> Option<&Clip> {
        self.tracks
            .iter()
            .find(|t| t.id == track)
            .and_then(|track| {
                track
                    .lanes
                    .iter()
                    .flat_map(|lane| lane.clips.iter())
                    .find(|clip| clip.id == clip_id)
            })
    }

    pub fn pattern(&self, id: u32) -> Option<&Pattern> {
        self.patterns.get(&id)
    }

    pub fn pattern_mut(&mut self, id: u32) -> Option<&mut Pattern> {
        self.patterns.get_mut(&id)
    }

    pub fn ensure_pattern(&mut self, id: u32) -> &mut Pattern {
        self.patterns.entry(id).or_insert_with(|| Pattern::new(id))
    }

    pub fn set_pattern_notes(&mut self, id: u32, notes: Vec<PatternNote>) {
        let pattern = self.ensure_pattern(id);
        pattern.set_notes(notes);
    }

    pub fn set_selection(&mut self, track: TrackId, clip: ClipId) {
        self.selection = Some((track, clip));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn track_lane_mut(&mut self, track: TrackId, lane_id: u32) -> Option<&mut TrackLane> {
        self.tracks
            .iter_mut()
            .find(|t| t.id == track)
            .and_then(|t| t.lane_mut(lane_id))
    }

    pub fn split_clip(
        &mut self,
        track: TrackId,
        lane_id: u32,
        clip_id: ClipId,
        split_tick: u64,
    ) -> Option<(ClipId, ClipId)> {
        let lane = self.track_lane_mut(track, lane_id)?;
        let index = lane.clips.iter().position(|c| c.id == clip_id)?;
        let clip = lane.clips.remove(index);
        if split_tick <= clip.start_ticks || split_tick >= clip.end_ticks() {
            lane.clips.insert(index, clip);
            return None;
        }

        let right_id = ClipId(random());
        let left_duration = split_tick - clip.start_ticks;
        let right_duration = clip.end_ticks() - split_tick;

        let mut left = Clip {
            duration_ticks: left_duration,
            ..clip.clone()
        };
        left.fade_out_ticks = left.fade_out_ticks.min(left_duration);

        let mut right = clip;
        right.id = right_id;
        right.start_ticks = split_tick;
        right.duration_ticks = right_duration;
        right.fade_in_ticks = right.fade_in_ticks.min(right_duration);
        left.crossfade_with = Some(right_id);
        right.crossfade_with = Some(left.id);
        right.time_stretch_enabled = false;

        lane.add_clip(left);
        lane.add_clip(right);
        Some((clip_id, right_id))
    }

    pub fn glue_clips(
        &mut self,
        track: TrackId,
        lane_id: u32,
        first: ClipId,
        second: ClipId,
    ) -> Option<ClipId> {
        let lane = self.track_lane_mut(track, lane_id)?;
        let first_idx = lane.clips.iter().position(|c| c.id == first)?;
        let second_idx = lane.clips.iter().position(|c| c.id == second)?;
        let (a, b) = if first_idx < second_idx {
            (lane.take_clip(first)?, lane.take_clip(second)?)
        } else {
            (lane.take_clip(second)?, lane.take_clip(first)?)
        };

        if a.end_ticks() != b.start_ticks {
            lane.add_clip(a);
            lane.add_clip(b);
            return None;
        }

        let merged_id = ClipId(random());
        let mut merged = Clip::new(
            merged_id,
            format!("{} + {}", a.name, b.name),
            a.start_ticks,
            b.end_ticks() - a.start_ticks,
            a.color,
            a.kind.clone(),
        );
        merged.fade_in_ticks = a.fade_in_ticks;
        merged.fade_out_ticks = b.fade_out_ticks;
        merged.time_stretch_enabled = false;
        lane.add_clip(merged);
        Some(merged_id)
    }

    pub fn slip_clip(
        &mut self,
        track: TrackId,
        lane_id: u32,
        clip_id: ClipId,
        delta: i64,
    ) -> Option<()> {
        let lane = self.track_lane_mut(track, lane_id)?;
        let clip = lane.clips.iter_mut().find(|c| c.id == clip_id)?;
        let start = clip.start_ticks as i64 + delta;
        clip.start_ticks = start.max(0) as u64;
        clip.fade_in_ticks = clip.fade_in_ticks.min(clip.duration_ticks);
        clip.fade_out_ticks = clip.fade_out_ticks.min(clip.duration_ticks);
        lane.clips.sort_by_key(|c| c.start_ticks);
        Some(())
    }

    pub fn duplicate_clip(
        &mut self,
        track: TrackId,
        lane_id: u32,
        clip_id: ClipId,
        target_start: u64,
    ) -> Option<ClipId> {
        let lane = self.track_lane_mut(track, lane_id)?;
        let source = lane.clips.iter().find(|c| c.id == clip_id)?.clone();
        let mut duplicate = source;
        duplicate.id = ClipId(random());
        duplicate.start_ticks = target_start;
        duplicate.crossfade_with = None;
        duplicate.time_stretch_enabled = false;
        lane.add_clip(duplicate.clone());
        Some(duplicate.id)
    }

    pub fn drop_clip_from_browser(
        &mut self,
        track: TrackId,
        lane_id: u32,
        name: impl Into<String>,
        start_ticks: u64,
        duration_ticks: u64,
        color: [f32; 4],
        kind: ClipKind,
    ) -> ClipId {
        let id = ClipId(random());
        let mut clip = Clip::new(id, name, start_ticks, duration_ticks, color, kind);
        clip.fade_in_ticks = 0;
        clip.fade_out_ticks = 0;
        clip.time_stretch_enabled = false;
        if let Some(lane) = self.track_lane_mut(track, lane_id) {
            lane.add_clip(clip);
        }
        id
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
                Clip::new(
                    ClipId((index as u64) * 10 + 1),
                    format!("{} Pattern", name),
                    0,
                    4 * ppq,
                    track.color,
                    ClipKind::Pattern {
                        pattern_id: (index as u32) + 1,
                    },
                ),
            );
            self.ensure_demo_pattern((index as u32) + 1, ppq as i64);
            track.add_clip_to_lane(
                0,
                Clip::new(
                    ClipId((index as u64) * 10 + 2),
                    format!("{} Variation", name),
                    6 * ppq,
                    4 * ppq,
                    track.color,
                    ClipKind::Pattern {
                        pattern_id: (index as u32) + 101,
                    },
                ),
            );
            self.ensure_demo_pattern((index as u32) + 101, ppq as i64);
            track.add_clip_to_lane(
                1,
                Clip::new(
                    ClipId((index as u64) * 10 + 3),
                    "Filter Sweep",
                    ppq,
                    5 * ppq,
                    [track.color[0], track.color[1], track.color[2], 0.7],
                    ClipKind::Automation,
                ),
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

    fn ensure_demo_pattern(&mut self, pattern_id: u32, ppq: i64) {
        let pattern = self.patterns.entry(pattern_id).or_insert_with(|| Pattern {
            id: pattern_id,
            notes: Vec::new(),
        });

        if pattern.notes.is_empty() {
            pattern.notes.push(PatternNote {
                id: 0,
                start_ticks: 0,
                duration_ticks: ppq.max(1),
                pitch: 60,
                velocity: 100,
                channel: 0,
            });
            pattern.notes.push(PatternNote {
                id: 1,
                start_ticks: (ppq / 2).max(1),
                duration_ticks: (ppq / 2).max(1),
                pitch: 64,
                velocity: 96,
                channel: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_glue_clips_preserve_fades() {
        let mut playlist = Playlist::new_default(960);
        let track = TrackId(0);
        let lane = 0;
        let clip_id = playlist.tracks[0].lanes[0].clips[0].id;
        let (_, right) = playlist.split_clip(track, lane, clip_id, 960).unwrap();
        let lane_ref = &playlist.tracks[0].lanes[0];
        assert!(lane_ref.clips.iter().any(|c| c.id == right));
        let merged = playlist.glue_clips(track, lane, clip_id, right).unwrap();
        let merged_clip = playlist.tracks[0].lanes[0]
            .clips
            .iter()
            .find(|c| c.id == merged)
            .unwrap();
        assert_eq!(merged_clip.fade_in_ticks, 0);
        assert_eq!(merged_clip.fade_out_ticks, 0);
    }

    #[test]
    fn slip_and_duplicate_keep_ordering() {
        let mut playlist = Playlist::new_default(960);
        let track = TrackId(1);
        let lane = 0;
        let source = playlist.tracks[1].lanes[0].clips[0].id;
        playlist.slip_clip(track, lane, source, 480).unwrap();
        let dup = playlist
            .duplicate_clip(track, lane, source, 960 * 8)
            .unwrap();
        let lane = &playlist.tracks[1].lanes[0];
        assert!(lane.clips.iter().any(|c| c.id == dup));
        assert!(lane
            .clips
            .windows(2)
            .all(|w| w[0].start_ticks <= w[1].start_ticks));
    }
}
