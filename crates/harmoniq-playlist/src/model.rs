use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Snap {
    None,
    Beat,
    Bar,
}

impl Default for Snap {
    fn default() -> Self {
        Snap::Beat
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub tracks: Vec<Track>,
    pub snap: Snap,
    pub zoom: f32,
}

impl Playlist {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            snap: Snap::default(),
            zoom: 1.0,
        }
    }

    pub fn add_track(&mut self, mut track: Track) {
        if track.id == 0 {
            track.id = self.next_track_id();
        }
        self.tracks.push(track);
    }

    pub fn track_mut(&mut self, id: u32) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }

    fn next_track_id(&self) -> u32 {
        self.tracks.iter().map(|track| track.id).max().unwrap_or(0) + 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub color: [f32; 4],
    pub clips: Vec<ClipInstance>,
}

impl Track {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color: [0.2, 0.2, 0.2, 1.0],
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: ClipInstance) {
        self.clips.push(clip);
        self.clips.sort_by_key(|clip| clip.start_samples);
    }

    pub fn clips_at(&self, sample: u64) -> Vec<&ClipInstance> {
        self.clips
            .iter()
            .filter(|clip| clip.start_samples <= sample && clip.end_samples() >= sample)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipInstance {
    pub clip: Clip,
    pub start_samples: u64,
    pub length_samples: u64,
    pub muted: bool,
}

impl ClipInstance {
    pub fn new(clip: Clip, start_samples: u64, length_samples: u64) -> Self {
        Self {
            clip,
            start_samples,
            length_samples,
            muted: false,
        }
    }

    pub fn end_samples(&self) -> u64 {
        self.start_samples + self.length_samples
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Clip {
    Pattern { pattern_id: u32 },
    Audio { file: PathBuf },
    Automation { target: ParamRef, curve: Curve },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamRef {
    pub plugin_id: String,
    pub param_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curve {
    pub points: Vec<(f32, f32)>,
}

impl Curve {
    pub fn new() -> Self {
        Self {
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn adding_tracks_assigns_ids() {
        let mut playlist = Playlist::new();
        playlist.add_track(Track::new(0, "Track 1"));
        playlist.add_track(Track::new(0, "Track 2"));
        assert_eq!(playlist.tracks[0].id, 1);
        assert_eq!(playlist.tracks[1].id, 2);
    }

    #[test]
    fn clip_lookup_finds_overlaps() {
        let mut track = Track::new(1, "Track");
        track.add_clip(ClipInstance::new(Clip::Pattern { pattern_id: 10 }, 0, 100));
        track.add_clip(ClipInstance::new(
            Clip::Pattern { pattern_id: 11 },
            200,
            100,
        ));
        assert_eq!(track.clips_at(20).len(), 1);
        assert_eq!(track.clips_at(220).len(), 1);
        assert!(track.clips_at(150).is_empty());
    }
}
