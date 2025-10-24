use std::cmp::Ordering;

use super::{CommandOutcome, ProjectCommand};
use crate::core::state::{AutomationPoint, LaneId, ProjectState};
use crate::core::CommandError;

#[derive(Clone)]
pub struct WriteAutomationPointCommand {
    pub lane_id: LaneId,
    pub beat: f32,
    pub value: f32,
}

impl ProjectCommand for WriteAutomationPointCommand {
    fn label(&self) -> &'static str {
        "Write automation"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let lane = state
            .automation
            .lane_mut(self.lane_id)
            .ok_or(CommandError::NotFound("automation lane"))?;
        let beat = self.beat.max(0.0);
        for point in &mut lane.points {
            if (point.beat - beat).abs() < f32::EPSILON {
                let previous = point.clone();
                point.value = self.value;
                point.beat = beat;
                return Ok(CommandOutcome {
                    inverse: Box::new(RestoreAutomationPointCommand {
                        lane_id: self.lane_id,
                        previous: Some(previous),
                        beat,
                    }),
                });
            }
        }

        lane.points.push(AutomationPoint {
            beat,
            value: self.value,
        });
        lane.points
            .sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(Ordering::Equal));

        Ok(CommandOutcome {
            inverse: Box::new(RestoreAutomationPointCommand {
                lane_id: self.lane_id,
                previous: None,
                beat,
            }),
        })
    }
}

struct RestoreAutomationPointCommand {
    lane_id: LaneId,
    previous: Option<AutomationPoint>,
    beat: f32,
}

impl ProjectCommand for RestoreAutomationPointCommand {
    fn label(&self) -> &'static str {
        "Restore automation"
    }

    fn apply(&self, state: &mut ProjectState) -> Result<CommandOutcome, CommandError> {
        let lane = state
            .automation
            .lane_mut(self.lane_id)
            .ok_or(CommandError::NotFound("automation lane"))?;
        let mut redo_value = None;
        if let Some(previous) = &self.previous {
            for point in &mut lane.points {
                if (point.beat - self.beat).abs() < f32::EPSILON {
                    redo_value = Some(point.value);
                    *point = previous.clone();
                    break;
                }
            }
        } else if let Some(index) = lane
            .points
            .iter()
            .position(|point| (point.beat - self.beat).abs() < f32::EPSILON)
        {
            let removed = lane.points.remove(index);
            redo_value = Some(removed.value);
        }

        Ok(CommandOutcome {
            inverse: Box::new(WriteAutomationPointCommand {
                lane_id: self.lane_id,
                beat: self.beat,
                value: redo_value.unwrap_or(0.0),
            }),
        })
    }
}
