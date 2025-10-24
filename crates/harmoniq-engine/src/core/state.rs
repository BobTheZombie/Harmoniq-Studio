use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::mixer::{MixerBusState, MixerState, MixerTargetState};

use super::CommandError;

pub type TrackId = u32;
pub type ClipId = u64;
pub type LaneId = u32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectState {
    pub arrangement: ArrangementState,
    pub mixer: MixerState,
    pub automation: AutomationState,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            arrangement: ArrangementState::default(),
            mixer: MixerState::default(),
            automation: AutomationState::default(),
        }
    }
}

impl ProjectState {
    pub fn ensure_invariants(&self) -> Result<(), CommandError> {
        self.validate_arrangement()?;
        self.validate_automation()?;
        self.validate_mixer_routing()?;
        Ok(())
    }

    fn validate_arrangement(&self) -> Result<(), CommandError> {
        let mut seen = HashSet::new();
        for track in &self.arrangement.tracks {
            for clip in &track.clips {
                if !seen.insert(clip.id) {
                    return Err(CommandError::invariant("duplicate clip id"));
                }
            }
        }
        Ok(())
    }

    fn validate_automation(&self) -> Result<(), CommandError> {
        let track_ids: HashSet<_> = self
            .arrangement
            .tracks
            .iter()
            .map(|track| track.id)
            .collect();
        let clip_ids: HashSet<_> = self
            .arrangement
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter().map(|clip| clip.id))
            .collect();
        for lane in &self.automation.lanes {
            match lane.owner {
                AutomationOwner::Track(id) => {
                    if !track_ids.contains(&id) {
                        return Err(CommandError::invariant(
                            "automation references missing track",
                        ));
                    }
                }
                AutomationOwner::Clip(id) => {
                    if !clip_ids.contains(&id) {
                        return Err(CommandError::invariant(
                            "automation references missing clip",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_mixer_routing(&self) -> Result<(), CommandError> {
        let bus_count = self.mixer.buses.len();
        for track in &self.mixer.tracks {
            if let MixerTargetState::Bus(index) = track.target {
                if index >= bus_count {
                    return Err(CommandError::invariant("track routes to missing bus"));
                }
            }
        }
        for bus in &self.mixer.buses {
            if let MixerTargetState::Bus(index) = bus.target {
                if index >= bus_count {
                    return Err(CommandError::invariant("bus routes to missing bus"));
                }
            }
        }
        let mut visiting = vec![VisitState::Unvisited; bus_count];
        for index in 0..bus_count {
            dfs_bus(&self.mixer.buses, index, &mut visiting)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArrangementState {
    pub tracks: Vec<ArrangementTrack>,
    pub next_track_id: TrackId,
    pub next_clip_id: ClipId,
}

impl Default for ArrangementState {
    fn default() -> Self {
        let mut state = Self {
            tracks: Vec::new(),
            next_track_id: 1,
            next_clip_id: 1,
        };
        for name in ["Drums", "Bass", "Lead", "Pads"] {
            state.push_track(ArrangementTrack::new(name));
        }
        state
    }
}

impl ArrangementState {
    pub fn push_track(&mut self, mut track: ArrangementTrack) {
        if track.id == 0 {
            track.id = self.allocate_track_id();
        } else {
            self.next_track_id = self.next_track_id.max(track.id + 1);
        }
        self.tracks.push(track);
    }

    pub fn allocate_track_id(&mut self) -> TrackId {
        let id = self.next_track_id;
        self.next_track_id += 1;
        id
    }

    pub fn allocate_clip_id(&mut self) -> ClipId {
        let id = self.next_clip_id;
        self.next_clip_id += 1;
        id
    }

    pub fn track_index(&self, id: TrackId) -> Option<usize> {
        self.tracks.iter().position(|track| track.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut ArrangementTrack> {
        let index = self.track_index(id)?;
        self.tracks.get_mut(index)
    }

    pub fn clip_position(&self, clip_id: ClipId) -> Option<(usize, usize)> {
        for (track_index, track) in self.tracks.iter().enumerate() {
            if let Some(index) = track.clips.iter().position(|clip| clip.id == clip_id) {
                return Some((track_index, index));
            }
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArrangementTrack {
    pub id: TrackId,
    pub name: String,
    pub clips: Vec<ArrangementClip>,
}

impl ArrangementTrack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            clips: Vec::new(),
        }
    }

    pub fn insert_clip(&mut self, clip: ArrangementClip) -> usize {
        let position = self
            .clips
            .iter()
            .position(|existing| existing.start > clip.start)
            .unwrap_or(self.clips.len());
        self.clips.insert(position, clip);
        position
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArrangementClip {
    pub id: ClipId,
    pub name: String,
    pub start: f32,
    pub length: f32,
    pub media: Option<String>,
}

impl ArrangementClip {
    pub fn end(&self) -> f32 {
        self.start + self.length
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct AutomationState {
    pub lanes: Vec<AutomationLaneState>,
    pub next_lane_id: LaneId,
}

impl AutomationState {
    pub fn allocate_lane_id(&mut self) -> LaneId {
        let id = self.next_lane_id;
        self.next_lane_id += 1;
        id
    }

    pub fn insert_lane(&mut self, lane: AutomationLaneState) {
        self.next_lane_id = self.next_lane_id.max(lane.id + 1);
        self.lanes.push(lane);
    }

    pub fn remove_lanes_by_owner(&mut self, owner: AutomationOwner) -> Vec<AutomationLaneState> {
        let mut removed = Vec::new();
        let mut retained = Vec::with_capacity(self.lanes.len());
        for lane in self.lanes.drain(..) {
            if lane.owner == owner {
                removed.push(lane);
            } else {
                retained.push(lane);
            }
        }
        self.lanes = retained;
        removed
    }

    pub fn lane_mut(&mut self, id: LaneId) -> Option<&mut AutomationLaneState> {
        self.lanes.iter_mut().find(|lane| lane.id == id)
    }

    pub fn shift_clip_automation(&mut self, clip: ClipId, delta: f32) {
        if delta.abs() <= f32::EPSILON {
            return;
        }
        for lane in &mut self.lanes {
            if lane.owner == AutomationOwner::Clip(clip) {
                for point in &mut lane.points {
                    point.beat += delta;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutomationLaneState {
    pub id: LaneId,
    pub owner: AutomationOwner,
    pub parameter: String,
    pub points: Vec<AutomationPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AutomationOwner {
    Track(TrackId),
    Clip(ClipId),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutomationPoint {
    pub beat: f32,
    pub value: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

fn dfs_bus(
    buses: &[MixerBusState],
    index: usize,
    visiting: &mut [VisitState],
) -> Result<(), CommandError> {
    match visiting[index] {
        VisitState::Visited => return Ok(()),
        VisitState::Visiting => {
            return Err(CommandError::invariant("mixer routing contains a loop"));
        }
        VisitState::Unvisited => {}
    }
    visiting[index] = VisitState::Visiting;
    if let MixerTargetState::Bus(next) = buses[index].target {
        dfs_bus(buses, next, visiting)?;
    }
    visiting[index] = VisitState::Visited;
    Ok(())
}
