pub fn level_to_amount(db: f32) -> f32 {
    let normalized = (db + 60.0) / 66.0;
    normalized.clamp(0.0, 1.0)
}

pub fn peak_amount(db: f32) -> f32 {
    level_to_amount(db)
}
