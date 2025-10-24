use harmoniq_engine::{
    AddClipCommand, ArrangementClip, CommandBus, CreateTrackCommand, MixerEndpoint, MoveClipCommand,
    ProjectState, SetMixerTargetCommand, WriteAutomationPointCommand,
};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Operation {
    CreateTrack,
    AddClip { track_hint: u8, length: u8 },
    MoveClip { clip_hint: u8, track_hint: u8, offset: i8 },
    RouteTrack { track_hint: u8, bus_hint: u8 },
    RouteBus { bus_hint: u8, target_hint: u8 },
    WriteAutomation { lane_hint: u8, beat: u8, value: i16 },
}

fn operation_strategy() -> impl Strategy<Value = Operation> {
    prop_oneof![
        Just(Operation::CreateTrack),
        (any::<u8>(), any::<u8>()).prop_map(|(track_hint, length)| Operation::AddClip { track_hint, length }),
        (any::<u8>(), any::<u8>(), any::<i8>()).prop_map(|(clip_hint, track_hint, offset)| Operation::MoveClip {
            clip_hint,
            track_hint,
            offset,
        }),
        (any::<u8>(), any::<u8>()).prop_map(|(track_hint, bus_hint)| Operation::RouteTrack { track_hint, bus_hint }),
        (any::<u8>(), any::<u8>()).prop_map(|(bus_hint, target_hint)| Operation::RouteBus {
            bus_hint,
            target_hint,
        }),
        (any::<u8>(), any::<u8>(), any::<i16>()).prop_map(|(lane_hint, beat, value)| Operation::WriteAutomation {
            lane_hint,
            beat,
            value,
        }),
    ]
}

proptest! {
    #[test]
    fn random_sequences_preserve_invariants(ops in prop::collection::vec(operation_strategy(), 1..64)) {
        let mut state = ProjectState::default();
        if state.mixer.buses.is_empty() {
            state.mixer.buses.push(Default::default());
        }
        state.mixer.buses.push(Default::default());
        let mut bus = CommandBus::new(state);
        let mut next_clip_id = 100u64;
        let mut known_clips = Vec::new();

        for op in ops {
            match op {
                Operation::CreateTrack => {
                    let name = format!("Track {}", bus.state().arrangement.tracks.len() + 1);
                    let _ = bus.execute(CreateTrackCommand { name });
                }
                Operation::AddClip { track_hint, length } => {
                    if bus.state().arrangement.tracks.is_empty() {
                        continue;
                    }
                    let track_index = (track_hint as usize) % bus.state().arrangement.tracks.len();
                    let track_id = bus.state().arrangement.tracks[track_index].id;
                    let clip = ArrangementClip {
                        id: next_clip_id,
                        name: format!("Clip {next_clip_id}"),
                        start: 0.0,
                        length: (length as f32).max(1.0),
                        media: None,
                    };
                    next_clip_id += 1;
                    if bus.execute(AddClipCommand { track_id, clip }).is_ok() {
                        known_clips.push(next_clip_id - 1);
                    }
                }
                Operation::MoveClip { clip_hint, track_hint, offset } => {
                    if known_clips.is_empty() || bus.state().arrangement.tracks.is_empty() {
                        continue;
                    }
                    let clip_index = (clip_hint as usize) % known_clips.len();
                    let clip_id = known_clips[clip_index];
                    let track_index = (track_hint as usize) % bus.state().arrangement.tracks.len();
                    let track_id = bus.state().arrangement.tracks[track_index].id;
                    let current = bus.state().arrangement.clip_position(clip_id);
                    if let Some((tidx, cidx)) = current {
                        let start = bus.state().arrangement.tracks[tidx].clips[cidx].start;
                        let new_start = (start + (offset as f32)).max(0.0);
                        let _ = bus.execute(MoveClipCommand { clip_id, target_track: track_id, new_start });
                    }
                }
                Operation::RouteTrack { track_hint, bus_hint } => {
                    if bus.state().arrangement.tracks.is_empty() {
                        continue;
                    }
                    let track_index = (track_hint as usize) % bus.state().arrangement.tracks.len();
                    let track_id = bus.state().arrangement.tracks[track_index].id;
                    let target = if bus_hint % 3 == 0 {
                        harmoniq_engine::MixerTargetState::Master
                    } else {
                        let bus_count = bus.state().mixer.buses.len();
                        if bus_count == 0 { continue; }
                        harmoniq_engine::MixerTargetState::Bus((bus_hint as usize) % bus_count)
                    };
                    let _ = bus.execute(SetMixerTargetCommand { endpoint: MixerEndpoint::Track(track_id), target });
                }
                Operation::RouteBus { bus_hint, target_hint } => {
                    let bus_count = bus.state().mixer.buses.len();
                    if bus_count == 0 { continue; }
                    let index = (bus_hint as usize) % bus_count;
                    let target = if target_hint % 3 == 0 {
                        harmoniq_engine::MixerTargetState::Master
                    } else {
                        harmoniq_engine::MixerTargetState::Bus((target_hint as usize) % bus_count)
                    };
                    let _ = bus.execute(SetMixerTargetCommand { endpoint: MixerEndpoint::Bus(index), target });
                }
                Operation::WriteAutomation { lane_hint, beat, value } => {
                    if bus.state().automation.lanes.is_empty() {
                        continue;
                    }
                    let lane_index = (lane_hint as usize) % bus.state().automation.lanes.len();
                    let lane_id = bus.state().automation.lanes[lane_index].id;
                    let beat = (beat as f32) / 4.0;
                    let value = (value as f32) / 32768.0;
                    let _ = bus.execute(WriteAutomationPointCommand { lane_id, beat, value });
                }
            }
        }

        bus.state().ensure_invariants().unwrap();
    }
}
