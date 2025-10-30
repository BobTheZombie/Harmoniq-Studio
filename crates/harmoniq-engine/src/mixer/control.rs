use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use ringbuf::{Consumer, HeapRb, Producer};

pub type ChannelId = u32;
pub type SendId = u8;

#[derive(Debug, Clone)]
pub enum MixerCommand {
    SetGainPan {
        ch: ChannelId,
        gain_db: f32,
        pan: f32,
    },
    SetMute {
        ch: ChannelId,
        mute: bool,
    },
    SetSolo {
        ch: ChannelId,
        solo: bool,
    },
    OpenInsertBrowser {
        ch: ChannelId,
        slot: Option<usize>,
    },
    OpenInsertUi {
        ch: ChannelId,
        slot: usize,
    },
    SetInsertBypass {
        ch: ChannelId,
        slot: usize,
        bypass: bool,
    },
    RemoveInsert {
        ch: ChannelId,
        slot: usize,
    },
    ConfigureSend {
        ch: ChannelId,
        id: SendId,
        level: f32,
    },
    ReorderInsert {
        ch: ChannelId,
        from: usize,
        to: usize,
    },
    ApplyRouting {
        set: Vec<(ChannelId, String, f32)>,
        remove: Vec<(ChannelId, String)>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct MeterEvent {
    pub ch: ChannelId,
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    pub clip_l: bool,
    pub clip_r: bool,
}

pub trait MixerBackend {
    fn set_gain_pan(&mut self, ch: ChannelId, gain_db: f32, pan: f32);
    fn set_mute(&mut self, ch: ChannelId, mute: bool);
    fn set_solo(&mut self, ch: ChannelId, solo: bool);
    fn open_insert_browser(&mut self, ch: ChannelId, slot: Option<usize>);
    fn open_insert_ui(&mut self, ch: ChannelId, slot: usize);
    fn set_insert_bypass(&mut self, ch: ChannelId, slot: usize, bypass: bool);
    fn remove_insert(&mut self, ch: ChannelId, slot: usize);
    fn configure_send(&mut self, ch: ChannelId, id: SendId, level: f32);
    fn reorder_insert(&mut self, ch: ChannelId, from: usize, to: usize);
    fn apply_routing(&mut self, set: &[(ChannelId, String, f32)], remove: &[(ChannelId, String)]);
}

#[derive(Debug)]
pub struct EngineMixerHandle {
    command_tx: Sender<MixerCommand>,
    command_rx: Receiver<MixerCommand>,
    meter_tx: Producer<MeterEvent, HeapRb<MeterEvent>>,
    meter_shared: Arc<MeterShared>,
}

#[derive(Debug)]
struct MeterShared {
    consumer: Mutex<Consumer<MeterEvent, HeapRb<MeterEvent>>>,
}

#[derive(Debug, Clone)]
pub struct GuiMeterReceiver {
    inner: Arc<MeterShared>,
}

impl EngineMixerHandle {
    pub fn new(meter_capacity: usize) -> Self {
        let (command_tx, command_rx) = unbounded();
        let ring = HeapRb::new(meter_capacity.max(1));
        let (meter_tx, meter_rx) = ring.split();
        let meter_shared = Arc::new(MeterShared {
            consumer: Mutex::new(meter_rx),
        });
        Self {
            command_tx,
            command_rx,
            meter_tx,
            meter_shared,
        }
    }

    pub fn ui_sender(&self) -> Sender<MixerCommand> {
        self.command_tx.clone()
    }

    pub fn ui_meter_receiver(&self) -> GuiMeterReceiver {
        GuiMeterReceiver {
            inner: Arc::clone(&self.meter_shared),
        }
    }

    pub fn drain_commands_and_apply<B: MixerBackend>(&mut self, backend: &mut B) {
        while let Ok(command) = self.command_rx.try_recv() {
            match command {
                MixerCommand::SetGainPan { ch, gain_db, pan } => {
                    backend.set_gain_pan(ch, gain_db, pan);
                }
                MixerCommand::SetMute { ch, mute } => {
                    backend.set_mute(ch, mute);
                }
                MixerCommand::SetSolo { ch, solo } => {
                    backend.set_solo(ch, solo);
                }
                MixerCommand::OpenInsertBrowser { ch, slot } => {
                    backend.open_insert_browser(ch, slot);
                }
                MixerCommand::OpenInsertUi { ch, slot } => {
                    backend.open_insert_ui(ch, slot);
                }
                MixerCommand::SetInsertBypass { ch, slot, bypass } => {
                    backend.set_insert_bypass(ch, slot, bypass);
                }
                MixerCommand::RemoveInsert { ch, slot } => {
                    backend.remove_insert(ch, slot);
                }
                MixerCommand::ConfigureSend { ch, id, level } => {
                    backend.configure_send(ch, id, level);
                }
                MixerCommand::ReorderInsert { ch, from, to } => {
                    backend.reorder_insert(ch, from, to);
                }
                MixerCommand::ApplyRouting { set, remove } => {
                    backend.apply_routing(&set, &remove);
                }
            }
        }
    }

    pub fn push_meter(&mut self, event: MeterEvent) {
        let _ = self.meter_tx.push(event);
    }
}

impl GuiMeterReceiver {
    pub fn drain<F>(&self, mut visitor: F)
    where
        F: FnMut(MeterEvent),
    {
        let mut consumer = self.inner.consumer.lock();
        while let Some(event) = consumer.pop() {
            visitor(event);
        }
    }
}
