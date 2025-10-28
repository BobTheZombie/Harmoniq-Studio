use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crate::device::MidiBackend;

/// Interval between hotplug polling iterations.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Event emitted by the hotplug watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotplugEvent {
    /// A new device list snapshot is available.
    Snapshot(Vec<String>),
}

/// Watcher that periodically queries the backend for available devices.
pub struct HotplugWatcher<B: MidiBackend + Send + 'static> {
    stop_tx: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
    rx: Receiver<HotplugEvent>,
    _marker: std::marker::PhantomData<B>,
}

impl<B: MidiBackend + Send + 'static> HotplugWatcher<B> {
    /// Spawn a new watcher.
    pub fn spawn(backend: B, interval: Duration) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("harmoniq-midi-hotplug".into())
            .spawn(move || {
                while stop_rx.try_recv().is_err() {
                    match backend.enumerate() {
                        Ok(snapshot) => {
                            let _ = event_tx.send(HotplugEvent::Snapshot(snapshot));
                        }
                        Err(err) => {
                            tracing::debug!(?err, "midi hotplug enumerate failed");
                        }
                    }
                    thread::park_timeout(interval);
                }
            })?;
        Ok(Self {
            stop_tx: Some(stop_tx),
            thread: Some(handle),
            rx: event_rx,
            _marker: std::marker::PhantomData,
        })
    }

    /// Receive the next hotplug event, if available.
    pub fn try_recv(&self) -> Option<HotplugEvent> {
        self.rx.try_recv().ok()
    }
}

impl<B: MidiBackend + Send + 'static> Drop for HotplugWatcher<B> {
    fn drop(&mut self) {
        if let Some(stop) = self.stop_tx.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.thread.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

impl<B: MidiBackend + Send + 'static> Default for HotplugWatcher<B>
where
    B: Default,
{
    fn default() -> Self {
        Self::spawn(B::default(), DEFAULT_POLL_INTERVAL).expect("failed to start midi hotplug")
    }
}
