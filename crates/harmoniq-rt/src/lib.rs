#![cfg_attr(not(test), warn(clippy::pedantic))]

#[derive(Copy, Clone, Debug)]
pub enum RtEvent {
    Xrun { count: u32 },
    EngineLoad { pct: u16 },
    MaxBlockMicros { us: u32 },
}

impl RtEvent {
    #[inline]
    pub fn kind_id(&self) -> u8 {
        match self {
            RtEvent::Xrun { .. } => 1,
            RtEvent::EngineLoad { .. } => 2,
            RtEvent::MaxBlockMicros { .. } => 3,
        }
    }
}
