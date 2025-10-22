use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

use crate::editor::SharedReceiver;

/// Message flowing between the UI and the audio engine for parameter
/// automation.
#[derive(Debug, Clone)]
pub enum AutomationMessage {
    SetValue { value: f32 },
    BeginGesture,
    EndGesture,
    Touch,
    Release,
}

/// Metadata describing an automatable parameter exposed by a plugin.
#[derive(Debug, Clone)]
pub struct PluginParam {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub value: f32,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    pub automation: ParameterAutomation,
}

impl PluginParam {
    pub fn normalised(&self) -> f32 {
        if (self.max - self.min).abs() <= f32::EPSILON {
            0.0
        } else {
            (self.value - self.min) / (self.max - self.min)
        }
    }

    pub fn set_from_normalised(&mut self, value: f32) {
        self.value = self.min + value.clamp(0.0, 1.0) * (self.max - self.min);
    }
}

/// Bidirectional automation channel exposed to UI code.
#[derive(Debug, Clone)]
pub struct ParameterAutomation {
    pub(crate) to_engine: Sender<AutomationMessage>,
    pub(crate) from_engine: SharedReceiver<AutomationMessage>,
}

impl ParameterAutomation {
    pub fn send(&self, message: AutomationMessage) -> Result<(), AutomationMessage> {
        self.to_engine.try_send(message)
    }

    pub fn poll(&self) -> Vec<AutomationMessage> {
        self.from_engine.drain()
    }
}

pub(crate) struct ParameterAutomationChannels {
    pub to_engine_rx: Receiver<AutomationMessage>,
    pub from_engine_tx: Sender<AutomationMessage>,
}

pub(crate) fn create_parameter_automation() -> (ParameterAutomation, ParameterAutomationChannels) {
    let (to_engine_tx, to_engine_rx) = unbounded();
    let (from_engine_tx, from_engine_rx) = unbounded();
    let automation = ParameterAutomation {
        to_engine: to_engine_tx,
        from_engine: SharedReceiver::new(from_engine_rx),
    };
    let channels = ParameterAutomationChannels {
        to_engine_rx,
        from_engine_tx,
    };
    (automation, channels)
}

pub(crate) struct SharedSender<T> {
    inner: Arc<Mutex<Sender<T>>>,
}

impl<T> SharedSender<T> {
    pub fn new(sender: Sender<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(sender)),
        }
    }

    pub fn send(&self, value: T) -> Result<(), T> {
        self.inner.lock().try_send(value)
    }
}
