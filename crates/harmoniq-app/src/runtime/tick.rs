use super::bus::{BusReceiver, EngineEvent, SvcEvent};

#[allow(dead_code)]
pub struct FrameDrain<'a> {
    svc_rx: &'a mut BusReceiver<SvcEvent>,
    engine_rx: &'a mut BusReceiver<EngineEvent>,
}

impl<'a> FrameDrain<'a> {
    #[allow(dead_code)]
    pub fn new(
        svc_rx: &'a mut BusReceiver<SvcEvent>,
        engine_rx: &'a mut BusReceiver<EngineEvent>,
    ) -> Self {
        Self { svc_rx, engine_rx }
    }

    #[allow(dead_code)]
    pub fn drain<F, G>(&mut self, mut svc_handler: F, mut engine_handler: G)
    where
        F: FnMut(SvcEvent),
        G: FnMut(EngineEvent),
    {
        for evt in self.svc_rx.drain() {
            svc_handler(evt);
        }
        for evt in self.engine_rx.drain() {
            engine_handler(evt);
        }
    }
}
