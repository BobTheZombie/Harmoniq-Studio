use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use harmoniq_engine::clips::{crossfade, AudioClip, CrossfadeSpec, FadeCurve};

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
fn equal_power_crossfade_matches_golden() {
    let clip_a = AudioClip::with_sample_rate(48_000.0, vec![vec![0.0, 0.25, 0.5, 0.75, 1.0]]);
    let clip_b = AudioClip::with_sample_rate(48_000.0, vec![vec![1.0, 0.75, 0.5, 0.25, 0.0]]);
    let spec = CrossfadeSpec::new(3, FadeCurve::EqualPower);

    let result = crossfade(&clip_a, &clip_b, spec).expect("crossfade");
    assert_eq!(result.channels(), 1);

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/audio/clip_crossfade_equal_power.txt");
    let golden = read_golden(golden_path);

    assert_eq!(golden.len(), result.frames());
    let channel = result.channel(0).expect("channel");
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
