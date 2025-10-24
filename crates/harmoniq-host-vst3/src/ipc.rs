use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::adapter::SandboxRequest;
use crate::ring::SharedAudioRingDescriptor;

/// Commands issued by the host to the broker process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BrokerCommand {
    Hello,
    LoadPlugin {
        request: SandboxRequest,
        audio_ring: SharedAudioRingDescriptor,
    },
    ProcessBlock {
        frames: u32,
    },
    RequestState,
    RequestPresetDump,
    RegisterRtChannel,
    Shutdown,
    KillPlugin,
}

/// Events delivered from the broker back to the host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BrokerEvent {
    Acknowledge,
    PluginLoaded { name: String },
    PluginCrashed { code: Option<i32> },
    AudioProcessed { frames: u32 },
    StateDump { data: Vec<u8> },
    PresetDump { data: Vec<u8> },
    LatencyReported { samples: u32 },
    EditorWindowCreated { window_id: u64 },
}

/// Real-time safe message categories exchanged over the RT channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RtMessageKind {
    AudioAvailable { frames: u32 },
    ParameterUpdate { id: u32, value: f32 },
    LatencyChanged { samples: u32 },
}

/// Packet of real-time metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RtMessage {
    pub kind: RtMessageKind,
    pub timestamp_ns: u128,
}

impl RtMessage {
    pub fn audio(frames: u32) -> Self {
        Self {
            kind: RtMessageKind::AudioAvailable { frames },
            timestamp_ns: now_ns(),
        }
    }

    pub fn latency(samples: u32) -> Self {
        Self {
            kind: RtMessageKind::LatencyChanged { samples },
            timestamp_ns: now_ns(),
        }
    }
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// Lightweight in-process real-time channel.
#[derive(Debug, Clone)]
pub struct RtChannel {
    sender: Sender<RtMessage>,
    receiver: Receiver<RtMessage>,
}

impl RtChannel {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            sender: tx,
            receiver: rx,
        }
    }

    pub fn sender(&self) -> Sender<RtMessage> {
        self.sender.clone()
    }

    pub fn receiver(&self) -> Receiver<RtMessage> {
        self.receiver.clone()
    }
}

/// Bidirectional IPC transport backed by Read/Write streams (typically pipes).
#[derive(Debug)]
pub struct IpcTransport<R, W>
where
    R: Read + 'static,
    W: Write + 'static,
{
    reader: Arc<parking_lot::Mutex<BufReader<R>>>,
    writer: Arc<parking_lot::Mutex<BufWriter<W>>>,
}

impl<R, W> Clone for IpcTransport<R, W>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            reader: Arc::clone(&self.reader),
            writer: Arc::clone(&self.writer),
        }
    }
}

impl<R, W> IpcTransport<R, W>
where
    R: Read + 'static,
    W: Write + 'static,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: Arc::new(parking_lot::Mutex::new(BufReader::new(reader))),
            writer: Arc::new(parking_lot::Mutex::new(BufWriter::new(writer))),
        }
    }

    pub fn send<T: Serialize>(&self, value: &T) -> Result<()> {
        let mut writer = self.writer.lock();
        bincode::serialize_into(&mut *writer, value)?;
        writer.flush()?;
        Ok(())
    }

    pub fn recv<T: DeserializeOwned>(&self) -> Result<T> {
        let mut reader = self.reader.lock();
        let value = bincode::deserialize_from(&mut *reader)?;
        Ok(value)
    }
}

pub type BrokerClient<R, W> = IpcTransport<R, W>;

pub fn send_command<W: Write>(writer: &mut W, cmd: &BrokerCommand) -> Result<()> {
    bincode::serialize_into(&mut *writer, cmd).context("failed to serialize command")?;
    writer.flush().context("failed to flush command")?;
    Ok(())
}

pub fn read_event<R: Read>(reader: &mut R) -> Result<BrokerEvent> {
    let event: BrokerEvent =
        bincode::deserialize_from(reader).context("failed to read broker event")?;
    Ok(event)
}
