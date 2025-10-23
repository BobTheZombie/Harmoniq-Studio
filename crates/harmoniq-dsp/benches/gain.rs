use criterion::{criterion_group, criterion_main, Criterion};
use harmoniq_dsp::{gain::Gain, AudioBlock, AudioBlockMut};

fn bench_gain(c: &mut Criterion) {
    let mut buffer = vec![0.5f32; 2 * 512];
    let mut output = vec![0.0f32; 2 * 512];
    let gain = Gain::from_db(-3.0);
    c.bench_function("gain 2x512", |b| {
        b.iter(|| unsafe {
            let input = AudioBlock::from_interleaved(buffer.as_ptr(), 2, 512);
            let mut out = AudioBlockMut::from_interleaved(output.as_mut_ptr(), 2, 512);
            gain.process(&input, &mut out);
        })
    });
}

criterion_group!(benches, bench_gain);
criterion_main!(benches);
