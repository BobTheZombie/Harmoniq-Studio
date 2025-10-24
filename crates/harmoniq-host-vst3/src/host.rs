use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::adapter::{AdapterDescriptor, SandboxRequest};
use crate::broker::{BrokerConfig, PluginBroker};
use crate::ipc::{BrokerEvent, RtChannel, RtMessage};
use crate::pdc::{PdcEvent, PluginDataCache};
use crate::ring::SharedAudioRing;
use crate::window::WindowEmbedder;

/// Abstraction over the sandbox broker implementation used by the host.
pub trait SandboxBroker {
    fn audio_ring(&self) -> &SharedAudioRing;
    fn audio_ring_mut(&mut self) -> &mut SharedAudioRing;
    fn load_plugin(&mut self, request: SandboxRequest) -> Result<()>;
    fn process_block(&mut self, frames: u32) -> Result<()>;
    fn request_state_dump(&mut self) -> Result<()>;
    fn request_preset_dump(&mut self) -> Result<()>;
    fn register_rt_channel(&mut self) -> Result<()>;
    fn kill_plugin(&mut self) -> Result<()>;
    fn try_next_event(&mut self) -> Option<BrokerEvent>;
    fn recv_event(&mut self, timeout: Duration) -> Option<BrokerEvent>;
}

impl SandboxBroker for PluginBroker {
    fn audio_ring(&self) -> &SharedAudioRing {
        PluginBroker::audio_ring(self)
    }

    fn audio_ring_mut(&mut self) -> &mut SharedAudioRing {
        PluginBroker::audio_ring_mut(self)
    }

    fn load_plugin(&mut self, request: SandboxRequest) -> Result<()> {
        PluginBroker::load_plugin(self, request)
    }

    fn process_block(&mut self, frames: u32) -> Result<()> {
        PluginBroker::process_block(self, frames)
    }

    fn request_state_dump(&mut self) -> Result<()> {
        PluginBroker::request_state_dump(self)
    }

    fn request_preset_dump(&mut self) -> Result<()> {
        PluginBroker::request_preset_dump(self)
    }

    fn register_rt_channel(&mut self) -> Result<()> {
        PluginBroker::register_rt_channel(self)
    }

    fn kill_plugin(&mut self) -> Result<()> {
        PluginBroker::kill_plugin(self)
    }

    fn try_next_event(&mut self) -> Option<BrokerEvent> {
        PluginBroker::try_next_event(self)
    }

    fn recv_event(&mut self, timeout: Duration) -> Option<BrokerEvent> {
        PluginBroker::recv_event(self, timeout)
    }
}

/// Runtime options when instantiating a VST3 host.
#[derive(Debug, Clone)]
pub struct HostOptions {
    pub adapter: AdapterDescriptor,
    pub broker: BrokerConfig,
    pub event_poll_timeout: Duration,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            adapter: AdapterDescriptor::official_sdk(),
            broker: BrokerConfig::default(),
            event_poll_timeout: Duration::from_millis(100),
        }
    }
}

/// Helper builder for constructing a VST3 host with custom options.
#[derive(Debug, Default, Clone)]
pub struct Vst3HostBuilder {
    options: HostOptions,
}

impl Vst3HostBuilder {
    pub fn new() -> Self {
        Self {
            options: HostOptions::default(),
        }
    }

    pub fn adapter(mut self, adapter: AdapterDescriptor) -> Self {
        self.options.adapter = adapter;
        self
    }

    pub fn broker_config(mut self, config: BrokerConfig) -> Self {
        self.options.broker = config;
        self
    }

    pub fn event_poll_timeout(mut self, timeout: Duration) -> Self {
        self.options.event_poll_timeout = timeout;
        self
    }

    pub fn build(self) -> Result<Vst3Host<PluginBroker>> {
        let broker = PluginBroker::spawn(self.options.broker.clone())?;
        Ok(Vst3Host::with_broker(broker, self.options))
    }

    pub fn build_with_broker<B>(self, broker: B) -> Vst3Host<B>
    where
        B: SandboxBroker,
    {
        Vst3Host::with_broker(broker, self.options)
    }
}

/// Host instance orchestrating VST3 plugin life-cycle and IPC communication.
#[derive(Debug)]
pub struct Vst3Host<B: SandboxBroker> {
    broker: B,
    adapter: AdapterDescriptor,
    event_poll_timeout: Duration,
    cache: PluginDataCache,
    rt_channel: Option<RtChannel>,
    latency_samples: AtomicU32,
    plugin_name: Option<String>,
    pending_editor_window: Option<u64>,
}

impl<B: SandboxBroker> Vst3Host<B> {
    pub fn with_broker(broker: B, options: HostOptions) -> Self {
        Self {
            broker,
            adapter: options.adapter,
            event_poll_timeout: options.event_poll_timeout,
            cache: PluginDataCache::new(),
            rt_channel: None,
            latency_samples: AtomicU32::new(0),
            plugin_name: None,
            pending_editor_window: None,
        }
    }

    pub fn audio_ring(&self) -> &SharedAudioRing {
        self.broker.audio_ring()
    }

    pub fn audio_ring_mut(&mut self) -> &mut SharedAudioRing {
        self.broker.audio_ring_mut()
    }

    pub fn load_plugin(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let request = SandboxRequest::new(path, self.adapter.clone());
        self.broker
            .load_plugin(request)
            .context("failed to instruct broker to load VST3 plugin")
    }

    pub fn process_block(&mut self, frames: u32) -> Result<()> {
        self.broker
            .process_block(frames)
            .context("failed to request audio processing")
    }

    pub fn request_state_dump(&mut self) -> Result<()> {
        self.broker
            .request_state_dump()
            .context("failed to request state dump")
    }

    pub fn request_preset_dump(&mut self) -> Result<()> {
        self.broker
            .request_preset_dump()
            .context("failed to request preset dump")
    }

    pub fn kill_plugin(&mut self) -> Result<()> {
        self.broker
            .kill_plugin()
            .context("failed to signal plugin termination")
    }

    pub fn register_rt_channel(&mut self) -> Result<RtChannel> {
        if let Some(channel) = &self.rt_channel {
            return Ok(channel.clone());
        }

        let channel = RtChannel::new();
        self.broker
            .register_rt_channel()
            .context("failed to register real-time channel")?;
        self.rt_channel = Some(channel.clone());
        Ok(channel)
    }

    pub fn rt_channel(&self) -> Option<RtChannel> {
        self.rt_channel.clone()
    }

    pub fn drain_events(&mut self) {
        while let Some(event) = self.broker.try_next_event() {
            self.handle_event(event);
        }
    }

    pub fn poll_blocking(&mut self) {
        if let Some(event) = self.broker.recv_event(self.event_poll_timeout) {
            self.handle_event(event);
        }
    }

    pub fn plugin_name(&self) -> Option<&str> {
        self.plugin_name.as_deref()
    }

    pub fn latency_samples(&self) -> u32 {
        self.latency_samples.load(Ordering::SeqCst)
    }

    pub fn latest_state(&self) -> Option<&[u8]> {
        self.cache.latest_state()
    }

    pub fn latest_preset(&self) -> Option<&[u8]> {
        self.cache.latest_preset()
    }

    pub fn editor_window_id(&self) -> Option<u64> {
        self.pending_editor_window
    }

    pub fn pdc_history(&self) -> impl Iterator<Item = &PdcEvent> {
        self.cache.history()
    }

    pub fn attach_editor<E: WindowEmbedder>(&mut self, embedder: &E) -> Result<()> {
        let window_id = self
            .pending_editor_window
            .context("plugin has not provided an editor window id")?;
        embedder.attach(window_id)
    }

    pub fn detach_editor<E: WindowEmbedder>(&mut self, embedder: &E) -> Result<()> {
        if self.pending_editor_window.is_some() {
            embedder.detach()?;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: BrokerEvent) {
        match event {
            BrokerEvent::PluginLoaded { name } => {
                self.plugin_name = Some(name);
            }
            BrokerEvent::PluginCrashed { .. } => {
                self.pending_editor_window = None;
            }
            BrokerEvent::AudioProcessed { frames } => {
                if let Some(channel) = &self.rt_channel {
                    let _ = channel.sender().try_send(RtMessage::audio(frames));
                }
            }
            BrokerEvent::StateDump { data } => {
                self.cache.record_state(data);
            }
            BrokerEvent::PresetDump { data } => {
                self.cache.record_preset(data);
            }
            BrokerEvent::LatencyReported { samples } => {
                self.latency_samples.store(samples, Ordering::SeqCst);
                if let Some(channel) = &self.rt_channel {
                    let _ = channel.sender().try_send(RtMessage::latency(samples));
                }
            }
            BrokerEvent::EditorWindowCreated { window_id } => {
                self.pending_editor_window = Some(window_id);
            }
            BrokerEvent::Acknowledge => {}
        }
    }
}

impl Vst3Host<PluginBroker> {
    pub fn new() -> Result<Self> {
        Vst3HostBuilder::new().build()
    }
}
