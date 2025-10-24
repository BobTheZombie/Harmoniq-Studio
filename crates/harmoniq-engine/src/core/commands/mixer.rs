use super::{CommandOutcome, ProjectCommand};
use crate::core::state::{ProjectState, TrackId};
use crate::core::CommandError;
use crate::mixer::{MixerTargetState, MixerTrackState};

#[derive(Clone, Copy)]
pub enum MixerEndpoint {
    Track(TrackId),
    Bus(usize),
}

#[derive(Clone)]
pub struct SetMixerTargetCommand {
    pub endpoint: MixerEndpoint,
    pub target: MixerTargetState,
}

impl ProjectCommand for SetMixerTargetCommand {
    fn label(&self) -> &'static str {
        "Set mixer routing"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        match self.endpoint {
            MixerEndpoint::Track(track_id) => {
                let index = state
                    .arrangement
                    .track_index(track_id)
                    .ok_or(CommandError::NotFound("track"))?;
                let track = state
                    .mixer
                    .tracks
                    .get_mut(index)
                    .ok_or(CommandError::NotFound("track"))?;
                let previous = track.target.clone();
                track.target = self.target.clone();
                Ok(CommandOutcome {
                    inverse: Box::new(SetMixerTargetCommand {
                        endpoint: MixerEndpoint::Track(track_id),
                        target: previous,
                    }),
                })
            }
            MixerEndpoint::Bus(index) => {
                let bus = state
                    .mixer
                    .buses
                    .get_mut(index)
                    .ok_or(CommandError::NotFound("bus"))?;
                let previous = bus.target.clone();
                bus.target = self.target.clone();
                Ok(CommandOutcome {
                    inverse: Box::new(SetMixerTargetCommand {
                        endpoint: MixerEndpoint::Bus(index),
                        target: previous,
                    }),
                })
            }
        }
    }
}
