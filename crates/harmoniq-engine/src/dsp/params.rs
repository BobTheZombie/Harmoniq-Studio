#[derive(Clone, Copy, Debug)]
pub struct ParamUpdate {
    pub id: u32,
    pub value: f32,
}

impl ParamUpdate {
    #[inline]
    pub fn new(id: u32, value: f32) -> Self {
        Self { id, value }
    }
}
