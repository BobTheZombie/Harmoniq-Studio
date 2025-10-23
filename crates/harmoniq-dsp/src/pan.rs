#[inline]
pub fn constant_power(pan: f32) -> (f32, f32) {
    let angle = ((pan.clamp(-1.0, 1.0) + 1.0) * 0.5) * core::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}
