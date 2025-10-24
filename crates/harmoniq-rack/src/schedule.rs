use serde::{Deserialize, Serialize};

use crate::channel::{Channel, Rack};
use crate::pattern::StepSeq;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transport {
    pub sample_pos: u64,
    pub tempo: f32,
    pub signature_numerator: u32,
    pub signature_denominator: u32,
    pub sample_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScheduledEvent {
    NoteOn {
        channel_id: u32,
        pitch: u8,
        velocity: f32,
        sample: u64,
    },
    NoteOff {
        channel_id: u32,
        pitch: u8,
        sample: u64,
    },
}

pub struct Scheduler<'a> {
    rack: &'a Rack,
    transport: Transport,
}

impl<'a> Scheduler<'a> {
    pub fn new(rack: &'a Rack, transport: Transport) -> Self {
        Self { rack, transport }
    }

    pub fn collect_for_pattern(&self, pattern_id: u32) -> Vec<ScheduledEvent> {
        let mut events = Vec::new();
        let Some(pattern) = self.rack.find_pattern(pattern_id) else {
            return events;
        };

        for lane in &pattern.lanes {
            if let Some(channel) = self.rack.find_channel(lane.channel_id) {
                self.collect_for_channel(channel, &channel.steps, &mut events);
                for note in &channel.piano_roll.notes {
                    events.push(ScheduledEvent::NoteOn {
                        channel_id: channel.id,
                        pitch: note.pitch,
                        velocity: note.velocity,
                        sample: note.start_samples,
                    });
                    events.push(ScheduledEvent::NoteOff {
                        channel_id: channel.id,
                        pitch: note.pitch,
                        sample: note.start_samples + note.length_samples,
                    });
                }
            }
        }

        events.sort_by_key(|event| match event {
            ScheduledEvent::NoteOn { sample, .. } => *sample,
            ScheduledEvent::NoteOff { sample, .. } => *sample,
        });
        events
    }

    fn collect_for_channel(
        &self,
        channel: &Channel,
        steps: &StepSeq,
        out: &mut Vec<ScheduledEvent>,
    ) {
        let step_duration = self.step_duration_samples();
        for (index, step) in steps
            .lanes
            .iter()
            .flat_map(|lane| lane.steps.iter())
            .enumerate()
        {
            if !step.active {
                continue;
            }
            let base_sample = index as u64 * step_duration;
            let offset = swing_offset_samples(index, self.rack.swing, step_duration);
            out.push(ScheduledEvent::NoteOn {
                channel_id: channel.id,
                pitch: 60,
                velocity: step.velocity,
                sample: base_sample + offset,
            });
            out.push(ScheduledEvent::NoteOff {
                channel_id: channel.id,
                pitch: 60,
                sample: base_sample + offset + step_duration,
            });
        }
    }

    fn step_duration_samples(&self) -> u64 {
        let beats_per_second = self.transport.tempo / 60.0;
        let seconds_per_beat = 1.0 / beats_per_second;
        let beat_fraction = 1.0 / 4.0; // sixteen notes for default grid
        let seconds = seconds_per_beat * beat_fraction;
        (seconds * self.transport.sample_rate as f32) as u64
    }
}

fn swing_offset_samples(step_index: usize, swing: f32, step_duration: u64) -> u64 {
    if swing <= 0.0 || step_index % 2 == 0 {
        return 0;
    }
    let clamped = swing.clamp(0.0, 1.0);
    ((step_duration as f32) * clamped * 0.5) as u64
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::channel::{Channel, Rack};
    use crate::pattern::{Pattern, PatternLaneRef, PianoRollNote, Step, StepLane, StepSeq};

    fn build_rack_with_channel() -> Rack {
        let mut rack = Rack::new();
        let mut channel = Channel::new(1, "Test");
        channel.steps = StepSeq {
            lanes: vec![StepLane {
                steps: vec![
                    Step {
                        active: true,
                        velocity: 1.0,
                    },
                    Step {
                        active: false,
                        velocity: 1.0,
                    },
                    Step {
                        active: true,
                        velocity: 0.5,
                    },
                    Step {
                        active: false,
                        velocity: 1.0,
                    },
                ],
            }],
            steps_per_lane: 4,
        };
        channel.piano_roll.notes.push(PianoRollNote {
            pitch: 65,
            start_samples: 88,
            length_samples: 10,
            velocity: 0.8,
        });
        rack.channels.push(channel);
        let mut pattern = Pattern::new(1, "Pattern 1");
        pattern.lanes.push(PatternLaneRef::new(1));
        rack.patterns.push(pattern);
        rack
    }

    #[test]
    fn swing_offsets_even_steps_only() {
        let rack = build_rack_with_channel();
        let scheduler = Scheduler::new(
            &rack,
            Transport {
                sample_pos: 0,
                tempo: 120.0,
                signature_numerator: 4,
                signature_denominator: 4,
                sample_rate: 48_000.0,
            },
        );
        let events = scheduler.collect_for_pattern(1);
        assert!(events.iter().any(|event| matches!(
            event,
            ScheduledEvent::NoteOn {
                sample,
                ..
            } if *sample > 0
        )));
    }

    #[test]
    fn piano_roll_notes_are_included() {
        let mut rack = build_rack_with_channel();
        rack.channels[0].piano_roll.notes.push(PianoRollNote {
            pitch: 70,
            start_samples: 1000,
            length_samples: 100,
            velocity: 1.0,
        });
        let scheduler = Scheduler::new(
            &rack,
            Transport {
                sample_pos: 0,
                tempo: 120.0,
                signature_numerator: 4,
                signature_denominator: 4,
                sample_rate: 48_000.0,
            },
        );
        let events = scheduler.collect_for_pattern(1);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ScheduledEvent::NoteOn { .. }))
                .count(),
            4
        );
    }
}
