use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::broker::{BrokerConfig, PluginBroker};
use crate::cache::{PluginCacheEntry, PluginScanner};
use crate::ipc::BrokerEvent;
use crate::pdc::PluginDataCache;

#[derive(Debug, Clone)]
pub struct HostOptions {
    pub scan_paths: Vec<PathBuf>,
    pub cache_path: PathBuf,
    pub broker: BrokerConfig,
    pub event_timeout: Duration,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            scan_paths: Vec::new(),
            cache_path: std::env::temp_dir().join("harmoniq-clap-cache.json"),
            broker: BrokerConfig::default(),
            event_timeout: Duration::from_millis(250),
        }
    }
}

pub struct ClapHost {
    scanner: PluginScanner,
    broker: PluginBroker,
    loaded_plugin: Option<PluginCacheEntry>,
    events: Vec<BrokerEvent>,
    options: HostOptions,
    pdc: PluginDataCache,
}

impl ClapHost {
    pub fn new(options: HostOptions) -> Result<Self> {
        let scanner = PluginScanner::new(&options.cache_path);
        let broker = PluginBroker::spawn(options.broker.clone())?;
        Ok(Self {
            scanner,
            broker,
            loaded_plugin: None,
            events: Vec::new(),
            options,
            pdc: PluginDataCache::new(),
        })
    }

    pub fn scan(&self) -> Result<Vec<PluginCacheEntry>> {
        self.scanner
            .scan_directories(&self.options.scan_paths)
            .context("failed to scan plugin directories")
    }

    pub fn load_plugin(&mut self, entry: PluginCacheEntry) -> Result<()> {
        self.broker
            .load_plugin(&entry.path)
            .context("failed to load plugin via broker")?;
        self.loaded_plugin = Some(entry);
        Ok(())
    }

    pub fn load_plugin_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let entry = PluginCacheEntry::new(
            path.as_ref().to_path_buf(),
            path.as_ref()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Plugin")
                .to_string(),
            None,
            std::time::SystemTime::now(),
        );
        self.load_plugin(entry)
    }

    pub fn process_audio(&self, frames: u32) -> Result<()> {
        self.broker.process_block(frames)
    }

    pub fn request_state(&mut self) -> Result<Vec<u8>> {
        self.broker.request_state_dump()?;
        self.wait_for_event(|event| match event {
            BrokerEvent::StateDump { data } => Some(data.clone()),
            _ => None,
        })?
        .ok_or_else(|| anyhow!("broker did not provide a state dump"))
    }

    pub fn request_preset(&mut self) -> Result<Vec<u8>> {
        self.broker.request_preset_dump()?;
        self.wait_for_event(|event| match event {
            BrokerEvent::PresetDump { data } => Some(data.clone()),
            _ => None,
        })?
        .ok_or_else(|| anyhow!("broker did not provide a preset dump"))
    }

    pub fn take_events(&mut self) -> Vec<BrokerEvent> {
        self.poll_events();
        std::mem::take(&mut self.events)
    }

    pub fn broker(&self) -> &PluginBroker {
        &self.broker
    }

    pub fn data_cache(&self) -> &PluginDataCache {
        &self.pdc
    }

    fn poll_events(&mut self) {
        while let Some(event) = self.broker.try_next_event() {
            match &event {
                BrokerEvent::StateDump { data } => self.pdc.record_state(data.clone()),
                BrokerEvent::PresetDump { data } => self.pdc.record_preset(data.clone()),
                _ => {}
            }
            self.events.push(event);
        }
    }

    fn wait_for_event<F, T>(&mut self, mut predicate: F) -> Result<Option<T>>
    where
        F: FnMut(&BrokerEvent) -> Option<T>,
    {
        let deadline = std::time::Instant::now() + self.options.event_timeout;
        loop {
            self.poll_events();
            if let Some(pos) = self
                .events
                .iter()
                .position(|event| predicate(event).is_some())
            {
                let event = self.events.remove(pos);
                if let Some(value) = predicate(&event) {
                    return Ok(Some(value));
                }
            }

            if let Some(event) = self.broker.recv_event(Duration::from_millis(10)) {
                self.events.push(event);
                continue;
            }

            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }
}
