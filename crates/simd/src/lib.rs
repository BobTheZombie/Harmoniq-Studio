#![allow(clippy::needless_range_loop)]

pub use portable_simd::Simd;

pub type F32x8 = Simd<f32, 8>;

pub fn load_f32x8(input: &[f32]) -> F32x8 {
    assert!(input.len() >= 8);
    F32x8::from_slice(&input[..8])
}

pub fn store_f32x8(value: F32x8, output: &mut [f32]) {
    assert!(output.len() >= 8);
    value.write_to_slice(&mut output[..8]);
}

pub fn mul_add(a: F32x8, b: F32x8, acc: F32x8) -> F32x8 {
    a * b + acc
}
