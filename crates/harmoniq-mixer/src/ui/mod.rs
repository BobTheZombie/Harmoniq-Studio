pub mod mixer;
pub mod widgets;

#[inline(always)]
pub fn db_to_gain(db: f32) -> f32 {
    (db * 0.115129254f32).exp()
}
#[inline(always)]
pub fn gain_to_db(g: f32) -> f32 {
    (g.max(1e-9)).ln() * 8.685889638f32
}
