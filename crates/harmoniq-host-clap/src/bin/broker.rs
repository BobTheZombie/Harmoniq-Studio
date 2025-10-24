use std::io::{stdin, stdout};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};
use harmoniq_host_clap::ipc::{BrokerCommand, BrokerEvent, IpcTransport};
use harmoniq_host_clap::ring::SharedAudioRingDescriptor;

fn main() -> Result<()> {
    let stdin = stdin();
    let stdout = stdout();
    let mut transport = IpcTransport::new(stdin.lock(), stdout.lock());
    let mut runtime = BrokerRuntime::default();

    loop {
        let cmd: BrokerCommand = transport.recv()?;
        let continue_running = runtime.handle_command(&mut transport, cmd)?;
        runtime.poll_plugin(&mut transport)?;
        if !continue_running {
            break;
        }
    }

    Ok(())
}

#[derive(Default)]
struct BrokerRuntime {
    plugin: Option<Child>,
    ring: Option<SharedAudioRingDescriptor>,
    last_state: Option<Vec<u8>>,
    last_preset: Option<Vec<u8>>,
}

impl BrokerRuntime {
    fn handle_command<R, W>(
        &mut self,
        transport: &mut IpcTransport<R, W>,
        cmd: BrokerCommand,
    ) -> Result<bool>
    where
        R: std::io::Read + 'static,
        W: std::io::Write + 'static,
    {
        match cmd {
            BrokerCommand::Hello => {
                transport.send(&BrokerEvent::Acknowledge)?;
            }
            BrokerCommand::LoadPlugin { path, audio_ring } => {
                self.load_plugin(&path, audio_ring, transport)?;
            }
            BrokerCommand::ProcessBlock { frames } => {
                transport.send(&BrokerEvent::AudioProcessed { frames })?;
            }
            BrokerCommand::RequestState => {
                let state = self
                    .snapshot_ring()
                    .or_else(|| self.last_state.clone())
                    .unwrap_or_default();
                transport.send(&BrokerEvent::StateDump { data: state })?;
            }
            BrokerCommand::RequestPresetDump => {
                let preset = self.last_preset.clone().unwrap_or_default();
                transport.send(&BrokerEvent::PresetDump { data: preset })?;
            }
            BrokerCommand::KillPlugin => {
                if let Some(mut child) = self.plugin.take() {
                    let _ = child.kill();
                    transport.send(&BrokerEvent::PluginCrashed { code: None })?;
                }
            }
            BrokerCommand::Shutdown => {
                if let Some(mut child) = self.plugin.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                return Ok(false);
            }
            BrokerCommand::RegisterRtChannel => {
                transport.send(&BrokerEvent::Acknowledge)?;
            }
        }
        Ok(true)
    }

    fn poll_plugin<R, W>(&mut self, transport: &mut IpcTransport<R, W>) -> Result<()>
    where
        R: std::io::Read + 'static,
        W: std::io::Write + 'static,
    {
        if let Some(child) = self.plugin.as_mut() {
            if let Some(status) = child.try_wait()? {
                let code = status.code();
                transport.send(&BrokerEvent::PluginCrashed { code })?;
                self.plugin = None;
            }
        }
        Ok(())
    }

    fn load_plugin<R, W>(
        &mut self,
        path: &PathBuf,
        audio_ring: SharedAudioRingDescriptor,
        transport: &mut IpcTransport<R, W>,
    ) -> Result<()>
    where
        R: std::io::Read + 'static,
        W: std::io::Write + 'static,
    {
        let mut command = Command::new(path);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .env("HARMONIQ_PLUGIN_RING_PATH", &audio_ring.path)
            .env("HARMONIQ_PLUGIN_RING_FRAMES", audio_ring.frames.to_string())
            .env(
                "HARMONIQ_PLUGIN_RING_CHANNELS",
                audio_ring.channels.to_string(),
            );

        let child = command
            .spawn()
            .with_context(|| format!("failed to spawn plugin at {:?}", path))?;
        self.plugin = Some(child);
        self.ring = Some(audio_ring.clone());
        transport.send(&BrokerEvent::PluginLoaded {
            name: path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Plugin")
                .to_string(),
        })?;
        Ok(())
    }

    fn snapshot_ring(&mut self) -> Option<Vec<u8>> {
        let descriptor = self.ring.clone()?;
        match descriptor.read_latest_block() {
            Ok((data, _generation)) => {
                let mut bytes = Vec::with_capacity(data.len() * std::mem::size_of::<f32>());
                for sample in data {
                    bytes.extend_from_slice(&sample.to_ne_bytes());
                }
                self.last_state = Some(bytes.clone());
                Some(bytes)
            }
            Err(_) => None,
        }
    }
}
