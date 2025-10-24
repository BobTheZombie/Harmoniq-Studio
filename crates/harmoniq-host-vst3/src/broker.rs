use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use tracing::debug;

use crate::adapter::SandboxRequest;
use crate::ipc::{BrokerClient, BrokerCommand, BrokerEvent, IpcTransport};
use crate::ring::SharedAudioRing;

#[derive(Debug, Clone)]
pub struct BrokerConfig {
    pub executable: PathBuf,
    pub frames: u32,
    pub channels: u32,
    pub handshake_timeout: Duration,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            executable: std::env::var_os("HARMONIQ_HOST_VST3_BROKER")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("harmoniq-host-vst3-broker")),
            frames: 512,
            channels: 2,
            handshake_timeout: Duration::from_secs(2),
        }
    }
}

#[derive(Debug)]
pub struct PluginBroker {
    _config: BrokerConfig,
    child: Child,
    client: BrokerClient<ChildStdout, ChildStdin>,
    ring: SharedAudioRing,
    event_rx: Receiver<BrokerEvent>,
    _event_thread: thread::JoinHandle<()>,
}

impl PluginBroker {
    pub fn spawn(config: BrokerConfig) -> Result<Self> {
        let ring = SharedAudioRing::create(config.frames, config.channels)?;
        let mut command = Command::new(&config.executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn broker at {:?}", config.executable))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("broker stdout not captured"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("broker stdin not captured"))?;

        let client = IpcTransport::new(stdout, stdin);
        debug!(
            "waiting for broker handshake (timeout {:?})",
            config.handshake_timeout
        );
        client.send(&BrokerCommand::Hello)?;
        match client.recv::<BrokerEvent>()? {
            BrokerEvent::Acknowledge => debug!("broker acknowledged"),
            other => return Err(anyhow!("unexpected broker response: {:?}", other)),
        }

        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let reader_client = client.clone();
        let join_handle = thread::spawn(move || {
            while let Ok(event) = reader_client.recv::<BrokerEvent>() {
                if event_tx.send(event).is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            _config: config,
            child,
            client,
            ring,
            event_rx,
            _event_thread: join_handle,
        })
    }

    pub fn audio_ring(&self) -> &SharedAudioRing {
        &self.ring
    }

    pub fn audio_ring_mut(&mut self) -> &mut SharedAudioRing {
        &mut self.ring
    }

    pub fn load_plugin(&self, request: SandboxRequest) -> Result<()> {
        let cmd = BrokerCommand::LoadPlugin {
            request,
            audio_ring: self.ring.descriptor().clone(),
        };
        self.client.send(&cmd)?;
        Ok(())
    }

    pub fn process_block(&self, frames: u32) -> Result<()> {
        self.client
            .send(&BrokerCommand::ProcessBlock { frames })
            .context("failed to request audio processing")
    }

    pub fn request_state_dump(&self) -> Result<()> {
        self.client
            .send(&BrokerCommand::RequestState)
            .context("failed to request state dump")
    }

    pub fn request_preset_dump(&self) -> Result<()> {
        self.client
            .send(&BrokerCommand::RequestPresetDump)
            .context("failed to request preset dump")
    }

    pub fn register_rt_channel(&self) -> Result<()> {
        self.client
            .send(&BrokerCommand::RegisterRtChannel)
            .context("failed to register RT channel")
    }

    pub fn kill_plugin(&self) -> Result<()> {
        self.client
            .send(&BrokerCommand::KillPlugin)
            .context("failed to send kill command")
    }

    pub fn shutdown(&mut self) -> Result<()> {
        let _ = self.client.send(&BrokerCommand::Shutdown);
        match self.child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(anyhow!("broker exited with status {:?}", status)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn try_next_event(&self) -> Option<BrokerEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn recv_event(&self, timeout: Duration) -> Option<BrokerEvent> {
        match self.event_rx.recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
}

impl Drop for PluginBroker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
