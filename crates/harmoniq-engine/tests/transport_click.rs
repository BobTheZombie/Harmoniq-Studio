use harmoniq_dsp::{AudioBlock, AudioBlockMut};
use harmoniq_engine::dsp::events::TransportClock;
use harmoniq_engine::dsp::{nodes::MetronomeClickNode, DspGraph, GraphProcess};
use harmoniq_engine::{BeatInfo, LoopRegion, Tempo, TempoMap, TempoSegment, TimeSignature};

fn render_click_track(
    mut graph: DspGraph,
    clock: &TransportClock,
    sample_rate: f32,
    block_size: u32,
    blocks: usize,
) -> Vec<f32> {
    graph.prepare(sample_rate, block_size, 0, 1);
    let mut rendered = Vec::with_capacity(block_size as usize * blocks);
    for _ in 0..blocks {
        let mut block = vec![0.0f32; block_size as usize];
        let transport = clock.load();
        unsafe {
            let input = AudioBlock::empty();
            let output = AudioBlockMut::from_interleaved(block.as_mut_ptr(), 1, block_size);
            graph.process(GraphProcess {
                inputs: input,
                outputs: output,
                frames: block_size,
                transport,
                midi: &[],
            });
        }
        rendered.extend_from_slice(&block);
        clock.advance_samples(block_size);
    }
    rendered
}

fn tempo_map_with_change(sample_rate: f32) -> TempoMap {
    let first_tempo = Tempo(120.0);
    let second_tempo = Tempo(90.0);
    let first_length_beats = 4.0;
    let first_segment_samples =
        (first_tempo.samples_per_beat(sample_rate) * first_length_beats) as u64;
    TempoMap::new(vec![
        TempoSegment {
            start_sample: 0,
            tempo: first_tempo,
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
        },
        TempoSegment {
            start_sample: first_segment_samples,
            tempo: second_tempo,
            time_signature: TimeSignature {
                numerator: 3,
                denominator: 4,
            },
        },
    ])
}

fn expected_beats(map: &TempoMap, sample_rate: f32, limit_samples: u64) -> Vec<BeatInfo> {
    let mut beats = Vec::new();
    if let Some(mut beat) = map.first_beat_at_or_after(sample_rate, 0) {
        while beat.sample < limit_samples {
            beats.push(beat);
            if let Some(next) = map.beat_after(sample_rate, &beat) {
                beat = next;
            } else {
                break;
            }
        }
    }
    beats
}

#[test]
fn metronome_click_matches_tempo_map() {
    let sample_rate = 48_000.0;
    let block_size = 256;
    let blocks = 200;
    let tempo_map = tempo_map_with_change(sample_rate);
    let clock = TransportClock::with_map(tempo_map.clone());
    clock.seek(0);
    clock.start_immediately();

    let mut graph = DspGraph::new();
    let (click_id, _) = graph.add_node(Box::new(MetronomeClickNode::default()), 0);
    graph.set_topology(&[click_id]);

    let rendered = render_click_track(graph, &clock, sample_rate, block_size, blocks);
    let total_samples = rendered.len() as u64;
    let expected = expected_beats(&tempo_map, sample_rate, total_samples);

    let actual: Vec<(usize, f32)> = rendered
        .iter()
        .enumerate()
        .filter_map(|(idx, &sample)| {
            if sample.abs() > 1e-5 {
                Some((idx, sample))
            } else {
                None
            }
        })
        .collect();

    assert_eq!(actual.len(), expected.len(), "unexpected number of clicks");

    for ((index, amplitude), beat) in actual.iter().zip(expected.iter()) {
        let expected_index = beat.sample as usize;
        assert!(
            (*index as i64 - expected_index as i64).abs() <= 1,
            "click at sample {} deviates from expected {}",
            index,
            expected_index
        );
        let expected_amplitude = if beat.is_downbeat() { 1.0 } else { 0.4 };
        assert!(
            (amplitude - expected_amplitude).abs() < 1e-6,
            "click amplitude {} expected {}",
            amplitude,
            expected_amplitude
        );
    }
}

#[test]
fn transport_clock_sample_accuracy() {
    let map = TempoMap::single(Tempo(120.0), TimeSignature::four_four());
    let clock = TransportClock::with_map(map);
    clock.seek(0);
    clock.schedule_start(32);
    clock.advance_samples(128);
    let snapshot = clock.load();
    assert_eq!(snapshot.sample_position, 96);
    assert!(snapshot.is_playing);

    clock.schedule_stop(32);
    clock.advance_samples(128);
    let snapshot = clock.load();
    assert_eq!(snapshot.sample_position, 128);
    assert!(!snapshot.is_playing);

    clock.seek(0);
    clock.start_immediately();
    clock.set_loop_region(Some(LoopRegion { start: 64, end: 96 }));
    clock.advance_samples(160);
    let snapshot = clock.load();
    assert!(snapshot.sample_position >= 64 && snapshot.sample_position < 96);
}
