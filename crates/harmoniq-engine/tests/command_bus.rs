use harmoniq_engine::core::state::AutomationOwner;
use harmoniq_engine::{
    AddClipCommand, ArrangementClip, CommandBus, MixerEndpoint, MoveClipCommand, ProjectState,
    SetMixerTargetCommand, WriteAutomationPointCommand,
};

#[test]
fn undo_redo_restores_clip_position() {
    let mut bus = CommandBus::default();
    let track_id = bus.state().arrangement.tracks[0].id;

    let clip = ArrangementClip {
        id: 1,
        name: "Intro".into(),
        start: 0.0,
        length: 4.0,
        media: None,
    };
    bus.execute(AddClipCommand { track_id, clip }).unwrap();

    bus.execute(MoveClipCommand {
        clip_id: 1,
        target_track: track_id,
        new_start: 8.0,
    })
    .unwrap();

    let clip = &bus.state().arrangement.tracks[0].clips[0];
    assert_eq!(clip.start, 8.0);

    bus.undo().unwrap();
    let clip = &bus.state().arrangement.tracks[0].clips[0];
    assert_eq!(clip.start, 0.0);

    bus.redo().unwrap();
    let clip = &bus.state().arrangement.tracks[0].clips[0];
    assert_eq!(clip.start, 8.0);
}

#[test]
fn command_merging_collapses_history() {
    let mut bus = CommandBus::default();
    let track_id = bus.state().arrangement.tracks[0].id;
    let clip = ArrangementClip {
        id: 7,
        name: "Lead".into(),
        start: 0.0,
        length: 8.0,
        media: None,
    };
    bus.execute(AddClipCommand { track_id, clip }).unwrap();

    bus.execute(MoveClipCommand {
        clip_id: 7,
        target_track: track_id,
        new_start: 4.0,
    })
    .unwrap();
    bus.execute(MoveClipCommand {
        clip_id: 7,
        target_track: track_id,
        new_start: 12.0,
    })
    .unwrap();

    assert!(bus.can_undo());
    bus.undo().unwrap();
    let clip = &bus.state().arrangement.tracks[0].clips[0];
    assert_eq!(clip.start, 0.0);
}

#[test]
fn mixer_routing_prevents_cycles() {
    let mut state = ProjectState::default();
    state.mixer.buses.push(Default::default());
    state.mixer.buses.push(Default::default());
    let mut bus = CommandBus::new(state);

    bus.execute(SetMixerTargetCommand {
        endpoint: MixerEndpoint::Bus(0),
        target: harmoniq_engine::MixerTargetState::Bus(1),
    })
    .unwrap();

    let error = bus
        .execute(SetMixerTargetCommand {
            endpoint: MixerEndpoint::Bus(1),
            target: harmoniq_engine::MixerTargetState::Bus(0),
        })
        .unwrap_err();
    assert!(matches!(
        error,
        harmoniq_engine::CommandError::InvariantViolation(_)
    ));
}

#[test]
fn automation_points_roundtrip() {
    let mut bus = CommandBus::default();
    let track_id = bus.state().arrangement.tracks[0].id;
    let lane_id = bus
        .state()
        .automation
        .lanes
        .iter()
        .find(|lane| lane.owner == AutomationOwner::Track(track_id))
        .map(|lane| lane.id)
        .expect("lane");

    bus.execute(WriteAutomationPointCommand {
        lane_id,
        beat: 4.0,
        value: 0.75,
    })
    .unwrap();
    bus.undo().unwrap();
    assert!(bus.state().automation.lanes[0].points.is_empty());
    bus.redo().unwrap();
    assert_eq!(bus.state().automation.lanes[0].points[0].value, 0.75);
}
