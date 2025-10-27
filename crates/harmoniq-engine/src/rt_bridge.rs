use harmoniq_rt::RtEvent;
use rtrb::{Producer, PushError};

pub struct RtBridge {
    prod: Producer<RtEvent>,
    drop_count: u64,
}

impl RtBridge {
    pub fn new(prod: Producer<RtEvent>) -> Self {
        Self {
            prod,
            drop_count: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, ev: RtEvent) {
        match self.prod.push(ev) {
            Ok(()) => {}
            Err(PushError::Full(_)) => {
                self.drop_count = self.drop_count.wrapping_add(1);
            }
        }
    }

    pub fn dropped(&self) -> u64 {
        self.drop_count
    }
}
