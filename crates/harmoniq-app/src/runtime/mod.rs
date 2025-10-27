use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use harmoniq_engine::{BufferConfig, TransportState};
use tracing::{debug, error, info, warn};

use crate::audio::RealtimeAudio;

use self::bus::{
    DeviceRequest, DeviceStatus, EngineBus, EngineEvent, SvcEvent, TransportToggle, UiCommand,
    UiEngineCommand, UiSvcBus,
};

pub mod bus;
pub mod frame_pacing;
pub mod tick;

#[allow(dead_code)]
pub struct Runtime {
    service: Option<ServiceThread>,
    pub ui_bus: UiSvcBus,
    pub engine_bus: EngineBus,
    running: Arc<AtomicBool>,
}

impl Runtime {
    #[allow(dead_code)]
    pub fn new(buffer_config: BufferConfig, audio: Option<RealtimeAudio>) -> Self {
        let mut ui_bus = UiSvcBus::new(2048);
        let mut engine_bus = EngineBus::new(2048);
        let (ui_svc_rx, ui_svc_tx) = ui_bus
            .take_service_endpoints()
            .expect("ui service endpoints consumed");
        let (engine_svc_rx, engine_evt_tx) = engine_bus
            .take_service_endpoints()
            .expect("engine service endpoints consumed");
        let running = Arc::new(AtomicBool::new(true));
        let service = ServiceThread::spawn(
            ui_svc_rx,
            ui_svc_tx,
            engine_svc_rx,
            engine_evt_tx,
            running.clone(),
            buffer_config,
            audio,
        );
        Self {
            service: Some(service),
            ui_bus,
            engine_bus,
            running,
        }
    }

    #[allow(dead_code)]
    pub fn shutdown(&mut self) {
        if let Some(service) = self.service.take() {
            service.stop();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct ServiceThread {
    handle: JoinHandle<()>,
    running: Arc<AtomicBool>,
}

impl ServiceThread {
    fn spawn(
        mut ui_rx: bus::BusReceiver<UiCommand>,
        mut ui_tx: bus::BusSender<SvcEvent>,
        mut engine_cmd_rx: bus::BusReceiver<UiEngineCommand>,
        mut engine_evt_tx: bus::BusSender<EngineEvent>,
        running: Arc<AtomicBool>,
        buffer_config: BufferConfig,
        audio: Option<RealtimeAudio>,
    ) -> Self {
        let thread_running = running.clone();
        let handle = thread::Builder::new()
            .name("harmoniq-service".into())
            .spawn(move || {
                let mut state = ServiceState::new(buffer_config, audio);
                while thread_running.load(Ordering::Acquire) {
                    let mut idle = true;

                    while let Some(cmd) = ui_rx.try_recv() {
                        idle = false;
                        if let Err(err) =
                            state.handle_ui_command(cmd, &mut ui_tx, &mut engine_evt_tx)
                        {
                            warn!(?err, "service command failed");
                        }
                    }

                    while let Some(cmd) = engine_cmd_rx.try_recv() {
                        idle = false;
                        if let Err(err) = state.handle_engine_command(cmd) {
                            warn!(?err, "engine command dispatch failed");
                        }
                    }

                    state.tick(&mut ui_tx, &mut engine_evt_tx);

                    if idle {
                        thread::park_timeout(Duration::from_millis(2));
                    }
                }
                state.shutdown();
            })
            .expect("service thread");
        Self { handle, running }
    }

    fn stop(self) {
        self.running.store(false, Ordering::Release);
        self.handle.thread().unpark();
        if let Err(err) = self.handle.join() {
            error!(?err, "failed to join service thread");
        }
    }
}

struct ServiceState {
    buffer_config: BufferConfig,
    audio: Option<RealtimeAudio>,
    last_metrics: Instant,
}

impl ServiceState {
    fn new(buffer_config: BufferConfig, audio: Option<RealtimeAudio>) -> Self {
        Self {
            buffer_config,
            audio,
            last_metrics: Instant::now(),
        }
    }

    fn handle_ui_command(
        &mut self,
        cmd: UiCommand,
        ui_tx: &mut bus::BusSender<SvcEvent>,
        engine_tx: &mut bus::BusSender<EngineEvent>,
    ) -> anyhow::Result<()> {
        match cmd {
            UiCommand::OpenProject(path) => {
                debug!(?path, "open project requested");
                ui_tx
                    .try_send(SvcEvent::ProjectLoaded { path })
                    .map_err(|_| anyhow::anyhow!("service bus saturated"))?;
            }
            UiCommand::SaveProject(path) => {
                debug!(?path, "save project requested");
                ui_tx
                    .try_send(SvcEvent::ProjectSaved { path })
                    .map_err(|_| anyhow::anyhow!("service bus saturated"))?;
            }
            UiCommand::ScanPlugins => {
                info!("plugin scan requested");
                ui_tx
                    .try_send(SvcEvent::PluginScanFinished)
                    .map_err(|_| anyhow::anyhow!("service bus saturated"))?;
            }
            UiCommand::ChangeDevice(request) => {
                self.apply_device_request(request.clone(), ui_tx)?;
                info!(?request, "device change queued");
            }
            UiCommand::ToggleTransport(toggle) => {
                engine_tx
                    .try_send(EngineEvent {
                        kind: bus::EngineEventKind::Transport,
                        payload: bus::EngineEventPayload::TransportState(match toggle {
                            TransportToggle::Play => TransportState::Playing,
                            TransportToggle::Stop => TransportState::Stopped,
                            TransportToggle::Record => TransportState::Recording,
                        }),
                    })
                    .map_err(|_| anyhow::anyhow!("engine bus saturated"))?;
            }
            UiCommand::SetTransportState(state) => {
                engine_tx
                    .try_send(EngineEvent {
                        kind: bus::EngineEventKind::Transport,
                        payload: bus::EngineEventPayload::TransportState(state),
                    })
                    .map_err(|_| anyhow::anyhow!("engine bus saturated"))?;
            }
            UiCommand::SetTempo(value) => {
                engine_tx
                    .try_send(EngineEvent {
                        kind: bus::EngineEventKind::Metrics,
                        payload: bus::EngineEventPayload::BlockTiming(bus::BlockTiming {
                            period: Duration::from_secs_f32(60.0 / value.max(1.0)),
                            elapsed: Duration::from_secs(0),
                            xruns: 0,
                        }),
                    })
                    .map_err(|_| anyhow::anyhow!("engine bus saturated"))?;
            }
            _ => {
                debug!(?cmd, "ui command routed without effect");
            }
        }
        Ok(())
    }

    fn handle_engine_command(&mut self, cmd: UiEngineCommand) -> anyhow::Result<()> {
        match cmd {
            UiEngineCommand::Transport(toggle) => {
                debug!(?toggle, "service forwarding transport command");
            }
            UiEngineCommand::SetTempo(value) => {
                debug!(?value, "tempo update");
            }
            UiEngineCommand::SetTimeSignature {
                numerator,
                denominator,
            } => {
                debug!(numerator, denominator, "time signature update");
            }
            UiEngineCommand::Automation {
                plugin,
                parameter,
                value,
            } => {
                debug!(?plugin, parameter, value, "automation gesture");
            }
            UiEngineCommand::RackMutation(cmd) => {
                debug!(?cmd, "rack mutation queued");
            }
            UiEngineCommand::Midi(bytes) => {
                debug!(len = bytes.len(), "service received midi batch");
            }
        }
        Ok(())
    }

    fn apply_device_request(
        &mut self,
        request: DeviceRequest,
        ui_tx: &mut bus::BusSender<SvcEvent>,
    ) -> anyhow::Result<()> {
        let name = request.name.unwrap_or_else(|| "default device".to_string());
        self.buffer_config = request.config;
        ui_tx
            .try_send(SvcEvent::DeviceChanged(DeviceStatus {
                name,
                config: self.buffer_config.clone(),
            }))
            .map_err(|_| anyhow::anyhow!("service bus saturated"))?;
        Ok(())
    }

    fn tick(
        &mut self,
        ui_tx: &mut bus::BusSender<SvcEvent>,
        engine_tx: &mut bus::BusSender<EngineEvent>,
    ) {
        let now = Instant::now();
        if now.duration_since(self.last_metrics) > Duration::from_millis(500) {
            self.last_metrics = now;
            let _ = engine_tx.try_send(EngineEvent {
                kind: bus::EngineEventKind::Metrics,
                payload: bus::EngineEventPayload::BlockTiming(bus::BlockTiming {
                    period: Duration::from_secs_f64(
                        self.buffer_config.block_size as f64
                            / f64::from(self.buffer_config.sample_rate),
                    ),
                    elapsed: Duration::from_millis(0),
                    xruns: 0,
                }),
            });
            let _ = ui_tx.try_send(SvcEvent::TransportState(TransportState::Stopped));
        }
    }

    fn shutdown(&mut self) {
        if let Some(audio) = self.audio.take() {
            debug!("shutting down realtime audio");
            drop(audio);
        }
    }
}

#[cfg(test)]
mod tests;
