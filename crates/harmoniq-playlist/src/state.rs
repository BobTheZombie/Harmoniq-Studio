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

/// Track within the playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub color: [f32; 4],
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(id: TrackId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color: DEFAULT_TRACK_COLORS[id.as_usize() % DEFAULT_TRACK_COLORS.len()],
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
        self.clips.sort_by_key(|clip| clip.start_ticks);
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
        let clip = track.clips.iter().find(|clip| clip.id == clip_id)?;
        Some(SelectedClip {
            track: track.id,
            clip: clip.id,
            track_name: track.name.clone(),
            clip_name: clip.name.clone(),
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
            track.add_clip(Clip {
                id: ClipId((index as u64) * 10 + 1),
                name: format!("{} Pattern", name),
                start_ticks: 0,
                duration_ticks: 4 * ppq,
                color: track.color,
                kind: ClipKind::Pattern {
                    pattern_id: (index as u32) + 1,
                },
            });
            track.add_clip(Clip {
                id: ClipId((index as u64) * 10 + 2),
                name: format!("{} Variation", name),
                start_ticks: 6 * ppq,
                duration_ticks: 4 * ppq,
                color: track.color,
                kind: ClipKind::Pattern {
                    pattern_id: (index as u32) + 101,
                },
            });
            self.tracks.push(track);
        }
    }
}
