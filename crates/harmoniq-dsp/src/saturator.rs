#[inline]
pub fn soft_clip(sample: f32) -> f32 {
    let x = sample;
    let a = x.abs();
    (x * (27.0 + a * a)) / (27.0 + 9.0 * a * a)
}
