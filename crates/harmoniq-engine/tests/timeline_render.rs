use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use harmoniq_engine::clips::{AudioClip, FadeCurve, FadeSpec};
use harmoniq_engine::timeline::{ClipEvent, Timeline};

fn read_golden(path: PathBuf) -> Vec<f32> {
    let file = File::open(path).expect("failed to open golden");
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| line.parse::<f32>().ok())
        .collect()
}

#[test]
fn timeline_render_is_deterministic() {
    let clip_a = AudioClip::with_sample_rate(48_000.0, vec![vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]]);
    let clip_b =
        AudioClip::with_sample_rate(48_000.0, vec![vec![-1.0, -0.8, -0.6, -0.4, -0.2, 0.0]]);

    let fade_in = FadeSpec::new(2, FadeCurve::Linear);
    let fade_out = FadeSpec::new(2, FadeCurve::Linear);

    let mut timeline = Timeline::new(48_000.0, 1);
    timeline.add_clip(ClipEvent::new(clip_a.clone(), 0).with_fade_out(fade_out));
    timeline.add_clip(ClipEvent::new(clip_b.clone(), 3).with_fade_in(fade_in));

    let render_first = timeline.render().expect("render");
    let render_second = timeline.render().expect("render again");

    assert_eq!(render_first.samples(), render_second.samples());

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/audio/timeline_deterministic_mix.txt");
    let golden = read_golden(golden_path);

    let channel = render_first.channel(0).expect("channel");
    assert_eq!(golden.len(), channel.len());

    for (idx, (&expected, &actual)) in golden.iter().zip(channel.iter()).enumerate() {
        let diff = (expected - actual).abs();
        assert!(
            diff < 1e-4,
            "sample {} differs: expected {}, got {}",
            idx,
            expected,
            actual
        );
    }
}
