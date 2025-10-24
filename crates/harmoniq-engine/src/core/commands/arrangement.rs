use super::{CommandOutcome, ProjectCommand};
use crate::core::state::{
    ArrangementClip, ArrangementTrack, AutomationLaneState, AutomationOwner, ProjectState, TrackId,
};
use crate::core::CommandError;

#[derive(Clone)]
pub struct CreateTrackCommand {
    pub name: String,
}

impl ProjectCommand for CreateTrackCommand {
    fn label(&self) -> &'static str {
        "Create track"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let mut track = ArrangementTrack::new(self.name.clone());
        let track_id = state.arrangement.allocate_track_id();
        track.id = track_id;
        state.arrangement.tracks.push(track.clone());

        state.mixer.tracks.push(Default::default());

        let lane_id = state.automation.allocate_lane_id();
        let lane = AutomationLaneState {
            id: lane_id,
            owner: AutomationOwner::Track(track_id),
            parameter: "volume".into(),
            points: Vec::new(),
        };
        state.automation.insert_lane(lane);

        Ok(CommandOutcome {
            inverse: Box::new(RemoveTrackCommand { track_id, track }),
        })
    }
}

struct RemoveTrackCommand {
    track_id: TrackId,
    track: ArrangementTrack,
}

impl ProjectCommand for RemoveTrackCommand {
    fn label(&self) -> &'static str {
        "Remove track"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let index = state
            .arrangement
            .track_index(self.track_id)
            .ok_or(CommandError::NotFound("track"))?;
        let removed_track = state.arrangement.tracks.remove(index);
        let mixer_track = state.mixer.tracks.remove(index);
        let removed_lanes = state
            .automation
            .remove_lanes_by_owner(AutomationOwner::Track(self.track_id));

        Ok(CommandOutcome {
            inverse: Box::new(RestoreTrackCommand {
                index,
                track: removed_track,
                mixer_track,
                lanes: removed_lanes,
            }),
        })
    }
}

struct RestoreTrackCommand {
    index: usize,
    track: ArrangementTrack,
    mixer_track: crate::mixer::MixerTrackState,
    lanes: Vec<AutomationLaneState>,
}

impl ProjectCommand for RestoreTrackCommand {
    fn label(&self) -> &'static str {
        "Restore track"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        state
            .arrangement
            .tracks
            .insert(self.index, self.track.clone());
        state
            .mixer
            .tracks
            .insert(self.index, self.mixer_track.clone());
        for lane in &self.lanes {
            state.automation.insert_lane(lane.clone());
        }
        Ok(CommandOutcome {
            inverse: Box::new(RemoveTrackCommand {
                track_id: self.track.id,
                track: self.track.clone(),
            }),
        })
    }
}

#[derive(Clone)]
pub struct AddClipCommand {
    pub track_id: TrackId,
    pub clip: ArrangementClip,
}

impl ProjectCommand for AddClipCommand {
    fn label(&self) -> &'static str {
        "Add clip"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let track = state
            .arrangement
            .track_mut(self.track_id)
            .ok_or(CommandError::NotFound("track"))?;
        if track.clips.iter().any(|clip| clip.id == self.clip.id) {
            return Err(CommandError::Invalid("clip already exists"));
        }
        let clip = self.clip.clone();
        track.insert_clip(clip.clone());
        state.arrangement.next_clip_id = state.arrangement.next_clip_id.max(clip.id + 1);

        Ok(CommandOutcome {
            inverse: Box::new(RemoveClipCommand {
                track_id: self.track_id,
                clip,
            }),
        })
    }
}

struct RemoveClipCommand {
    track_id: TrackId,
    clip: ArrangementClip,
    lanes: Vec<AutomationLaneState>,
}

impl ProjectCommand for RemoveClipCommand {
    fn label(&self) -> &'static str {
        "Remove clip"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let track_index = state
            .arrangement
            .track_index(self.track_id)
            .ok_or(CommandError::NotFound("track"))?;
        let track = state
            .arrangement
            .tracks
            .get_mut(track_index)
            .ok_or(CommandError::NotFound("track"))?;
        let clip_index = track
            .clips
            .iter()
            .position(|clip| clip.id == self.clip.id)
            .ok_or(CommandError::NotFound("clip"))?;
        let removed = track.clips.remove(clip_index);
        let lanes = state
            .automation
            .remove_lanes_by_owner(AutomationOwner::Clip(removed.id));
        Ok(CommandOutcome {
            inverse: Box::new(RestoreClipCommand {
                track_id: self.track_id,
                clip: removed,
                lanes,
            }),
        })
    }
}

struct RestoreClipCommand {
    track_id: TrackId,
    clip: ArrangementClip,
    lanes: Vec<AutomationLaneState>,
}

impl ProjectCommand for RestoreClipCommand {
    fn label(&self) -> &'static str {
        "Restore clip"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let track = state
            .arrangement
            .track_mut(self.track_id)
            .ok_or(CommandError::NotFound("track"))?;
        track.insert_clip(self.clip.clone());
        for lane in &self.lanes {
            state.automation.insert_lane(lane.clone());
        }
        Ok(CommandOutcome {
            inverse: Box::new(RemoveClipCommand {
                track_id: self.track_id,
                clip: self.clip.clone(),
                lanes: self.lanes.clone(),
            }),
        })
    }
}

#[derive(Clone)]
pub struct MoveClipCommand {
    pub clip_id: super::super::state::ClipId,
    pub target_track: TrackId,
    pub new_start: f32,
}

impl ProjectCommand for MoveClipCommand {
    fn label(&self) -> &'static str {
        "Move clip"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let (source_track_index, clip_index) = state
            .arrangement
            .clip_position(self.clip_id)
            .ok_or(CommandError::NotFound("clip"))?;
        let mut clip = state.arrangement.tracks[source_track_index]
            .clips
            .remove(clip_index);
        let original_track = state.arrangement.tracks[source_track_index].id;
        let original_start = clip.start;
        let delta = self.new_start - clip.start;
        clip.start = self.new_start.max(0.0);

        let target_track_index = state
            .arrangement
            .track_index(self.target_track)
            .ok_or(CommandError::NotFound("track"))?;
        state.arrangement.tracks[target_track_index].insert_clip(clip.clone());
        state.automation.shift_clip_automation(self.clip_id, delta);

        Ok(CommandOutcome {
            inverse: Box::new(MoveClipCommand {
                clip_id: self.clip_id,
                target_track: original_track,
                new_start: original_start,
            }),
        })
    }

    fn should_merge(&self, previous: &dyn ProjectCommand) -> bool {
        previous
            .as_any()
            .downcast_ref::<MoveClipCommand>()
            .map(|other| other.clip_id == self.clip_id)
            .unwrap_or(false)
    }
}
