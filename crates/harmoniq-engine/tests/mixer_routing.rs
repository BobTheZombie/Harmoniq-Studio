use harmoniq_engine::buffer::AudioBuffer;
use harmoniq_engine::mixer::{
    MixerAuxSendState, MixerAuxState, MixerEngine, MixerModel, MixerState, MixerTrackState,
};

fn make_buffer(channels: usize, frames: usize, value: f32) -> AudioBuffer {
    let mut buffer = AudioBuffer::new(channels, frames);
    buffer.as_mut_slice().fill(value);
    buffer
}

#[test]
fn track_routes_to_master() {
    let mut state = MixerState::default();
    state.tracks = vec![MixerTrackState {
        name: "Test".into(),
        ..MixerTrackState::default()
    }];
    state.auxes.clear();
    let model = MixerModel::new(state.clone());
    let mut engine = MixerEngine::from_model(&model, 48_000.0, 64);
    let track_inputs = vec![make_buffer(2, 64, 0.5)];
    let mut output = AudioBuffer::new(2, 64);
    engine.process(&track_inputs, &mut output);
    for sample in output.as_slice() {
        assert!((sample - 0.5).abs() < 1e-3);
    }
}

#[test]
fn pre_fader_send_survives_mute() {
    let mut state = MixerState::default();
    state.tracks = vec![MixerTrackState {
        name: "Send".into(),
        fader_db: -120.0,
        aux_sends: vec![MixerAuxSendState {
            aux_index: 0,
            level_db: 0.0,
            pre_fader: true,
        }],
        ..MixerTrackState::default()
    }];
    state.auxes = vec![MixerAuxState {
        name: "Return".into(),
        return_db: 0.0,
    }];
    let model = MixerModel::new(state);
    let mut engine = MixerEngine::from_model(&model, 48_000.0, 64);
    let track_inputs = vec![make_buffer(2, 64, 0.25)];
    let mut output = AudioBuffer::new(2, 64);
    engine.process(&track_inputs, &mut output);
    let energy: f32 = output.as_slice().iter().map(|s| s.abs()).sum();
    assert!(
        energy > 0.01,
        "pre-fader send should feed the return even when muted"
    );
}

#[test]
fn inverted_tracks_cancel() {
    let mut state = MixerState::default();
    state.tracks = vec![
        MixerTrackState {
            name: "A".into(),
            ..MixerTrackState::default()
        },
        MixerTrackState {
            name: "B".into(),
            phase_invert: true,
            ..MixerTrackState::default()
        },
    ];
    state.auxes.clear();
    let model = MixerModel::new(state);
    let mut engine = MixerEngine::from_model(&model, 48_000.0, 64);
    let track_inputs = vec![make_buffer(2, 64, 0.5), make_buffer(2, 64, 0.5)];
    let mut output = AudioBuffer::new(2, 64);
    engine.process(&track_inputs, &mut output);
    let residual: f32 = output.as_slice().iter().map(|s| s.abs()).sum();
    assert!(
        residual < 1e-3,
        "out-of-phase tracks should cancel each other"
    );
}
