use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use harmoniq_engine::{BufferConfig, PluginId, TransportState};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum UiCommand {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ScanPlugins,
    ChangeDevice(DeviceRequest),
    AddTrack {
        name: String,
    },
    AddPluginToTrack {
        track_id: u32,
        plugin: PluginId,
    },
    OpenPluginEditor {
        instance_id: u64,
    },
    ClosePluginEditor {
        instance_id: u64,
    },
    ToggleTransport(TransportToggle),
    SetTransportState(TransportState),
    SetTempo(f32),
    SetTimeSignature {
        numerator: u32,
        denominator: u32,
    },
    AutomationGesture {
        plugin: PluginId,
        parameter: u32,
        value: f32,
    },
    MidiInput(Vec<u8>),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum SvcEvent {
    ProjectLoaded { path: PathBuf },
    ProjectSaved { path: PathBuf },
    PluginScanProgress { scanned: usize, total: usize },
    PluginScanFinished,
    DeviceChanged(DeviceStatus),
    TransportState(TransportState),
    Toast { title: String, body: String },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum UiEngineCommand {
    Transport(TransportToggle),
    SetTempo(f32),
    SetTimeSignature {
        numerator: u32,
        denominator: u32,
    },
    Automation {
        plugin: PluginId,
        parameter: u32,
        value: f32,
    },
    RackMutation(RackCommand),
    Midi(Vec<u8>),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum EngineEventKind {
    Transport,
    Metrics,
    Meter,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EngineEvent {
    pub kind: EngineEventKind,
    pub payload: EngineEventPayload,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum EngineEventPayload {
    TransportState(TransportState),
    BlockTiming(BlockTiming),
    PeakMeter { channel: usize, peak: f32 },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct BlockTiming {
    pub period: Duration,
    pub elapsed: Duration,
    pub xruns: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum RackCommand {
    CreateChannel { index: usize },
    RemoveChannel { index: usize },
    InsertPlugin { track_id: u32, plugin: PluginId },
    RemovePlugin { instance_id: u64 },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeviceRequest {
    pub name: Option<String>,
    pub config: BufferConfig,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeviceStatus {
    pub name: String,
    pub config: BufferConfig,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportToggle {
    Play,
    Stop,
    Record,
}

#[allow(dead_code)]
pub struct BusSender<T> {
    inner: HeapProducer<T>,
}

#[allow(dead_code)]
pub struct BusReceiver<T> {
    inner: HeapConsumer<T>,
}

impl<T> fmt::Debug for BusSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BusSender").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for BusReceiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BusReceiver").finish_non_exhaustive()
    }
}

#[allow(dead_code)]
pub fn channel<T>(capacity: usize) -> (BusSender<T>, BusReceiver<T>) {
    let ring = HeapRb::new(capacity.max(2));
    let (inner_tx, inner_rx) = ring.split();
    (
        BusSender { inner: inner_tx },
        BusReceiver { inner: inner_rx },
    )
}

impl<T> BusSender<T> {
    #[allow(dead_code)]
    pub fn try_send(&mut self, value: T) -> Result<(), T> {
        self.inner.push(value)
    }

    #[allow(dead_code)]
    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }
}

impl<T> BusReceiver<T> {
    #[allow(dead_code)]
    pub fn try_recv(&mut self) -> Option<T> {
        self.inner.pop()
    }

    #[allow(dead_code)]
    pub fn drain(&mut self) -> Drain<'_, T> {
        Drain { receiver: self }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[allow(dead_code)]
pub struct Drain<'a, T> {
    receiver: &'a mut BusReceiver<T>,
}

impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.try_recv()
    }
}

#[allow(dead_code)]
pub struct UiSvcBus {
    pub ui_tx: BusSender<UiCommand>,
    pub ui_rx: BusReceiver<SvcEvent>,
    svc_rx: Option<BusReceiver<UiCommand>>,
    svc_tx: Option<BusSender<SvcEvent>>,
}

impl UiSvcBus {
    #[allow(dead_code)]
    pub fn new(capacity: usize) -> Self {
        let (ui_tx, svc_rx) = channel(capacity);
        let (svc_tx, ui_rx) = channel(capacity);
        Self {
            ui_tx,
            ui_rx,
            svc_rx: Some(svc_rx),
            svc_tx: Some(svc_tx),
        }
    }

    #[allow(dead_code)]
    pub fn take_service_endpoints(
        &mut self,
    ) -> Option<(BusReceiver<UiCommand>, BusSender<SvcEvent>)> {
        match (self.svc_rx.take(), self.svc_tx.take()) {
            (Some(rx), Some(tx)) => Some((rx, tx)),
            _ => None,
        }
    }
}

#[allow(dead_code)]
pub struct EngineBus {
    pub ui_tx: BusSender<UiEngineCommand>,
    pub ui_rx: BusReceiver<EngineEvent>,
    svc_rx: Option<BusReceiver<UiEngineCommand>>,
    engine_tx: Option<BusSender<EngineEvent>>,
}

impl EngineBus {
    #[allow(dead_code)]
    pub fn new(capacity: usize) -> Self {
        let (ui_tx, svc_rx) = channel(capacity);
        let (engine_tx, ui_rx) = channel(capacity);
        Self {
            ui_tx,
            ui_rx,
            svc_rx: Some(svc_rx),
            engine_tx: Some(engine_tx),
        }
    }

    #[allow(dead_code)]
    pub fn take_service_endpoints(
        &mut self,
    ) -> Option<(BusReceiver<UiEngineCommand>, BusSender<EngineEvent>)> {
        match (self.svc_rx.take(), self.engine_tx.take()) {
            (Some(rx), Some(tx)) => Some((rx, tx)),
            _ => None,
        }
    }
}
