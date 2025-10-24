use std::collections::{hash_map::Entry, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};

use super::{
    AutomationCurve, AutomationEvent, AutomationRecorder, AutomationWriteMode, CurvePoint,
    CurveShape,
};
use crate::plugin::PluginId;

#[derive(Debug, Clone)]
pub struct ParameterSpec {
    pub index: usize,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParameterSpec {
    pub fn new(index: usize, name: impl Into<String>, min: f32, max: f32, default: f32) -> Self {
        Self {
            index,
            name: name.into(),
            min,
            max,
            default,
        }
    }

    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }
}

#[derive(Debug, Clone)]
pub enum AutomationCommand {
    RegisterParameter(ParameterSpec),
    DrawCurve {
        parameter: usize,
        sample: u64,
        value: f32,
        shape: CurveShape,
    },
    SetWriteMode {
        parameter: usize,
        mode: AutomationWriteMode,
    },
    Touch {
        parameter: usize,
        sample: u64,
        value: f32,
        shape: CurveShape,
    },
    Release {
        parameter: usize,
        sample: u64,
        value: Option<f32>,
    },
}

#[derive(Clone)]
pub struct AutomationSender {
    producer: Arc<Mutex<HeapProducer<AutomationCommand>>>,
}

impl AutomationSender {
    pub fn send(&self, command: AutomationCommand) -> Result<(), AutomationCommand> {
        let mut producer = self.producer.lock();
        producer.push(command).map_err(|command| command)
    }
}

pub struct AutomationLane {
    plugin_id: PluginId,
    producer: Arc<Mutex<HeapProducer<AutomationCommand>>>,
    consumer: HeapConsumer<AutomationCommand>,
    parameters: HashMap<usize, ParameterLane>,
}

impl AutomationLane {
    pub fn new(plugin_id: PluginId, capacity: usize) -> Self {
        let ring = HeapRb::new(capacity);
        let (producer, consumer) = ring.split();
        Self {
            plugin_id,
            producer: Arc::new(Mutex::new(producer)),
            consumer,
            parameters: HashMap::new(),
        }
    }

    pub fn sender(&self) -> AutomationSender {
        AutomationSender {
            producer: Arc::clone(&self.producer),
        }
    }

    pub fn register_parameter(&mut self, spec: ParameterSpec) {
        match self.parameters.entry(spec.index) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().update_spec(spec);
            }
            Entry::Vacant(entry) => {
                entry.insert(ParameterLane::new(spec));
            }
        }
    }

    pub fn apply_command(&mut self, command: AutomationCommand) {
        match command {
            AutomationCommand::RegisterParameter(spec) => {
                self.register_parameter(spec);
            }
            AutomationCommand::DrawCurve {
                parameter,
                sample,
                value,
                shape,
            } => {
                if let Some(lane) = self.parameters.get_mut(&parameter) {
                    lane.draw(sample, value, shape);
                }
            }
            AutomationCommand::SetWriteMode { parameter, mode } => {
                if let Some(lane) = self.parameters.get_mut(&parameter) {
                    lane.set_mode(mode);
                }
            }
            AutomationCommand::Touch {
                parameter,
                sample,
                value,
                shape,
            } => {
                if let Some(lane) = self.parameters.get_mut(&parameter) {
                    lane.touch(sample, value, shape);
                }
            }
            AutomationCommand::Release {
                parameter,
                sample,
                value,
            } => {
                if let Some(lane) = self.parameters.get_mut(&parameter) {
                    lane.release(sample, value);
                }
            }
        }
    }

    pub fn render(&mut self, block_start: u64, block_len: u32, output: &mut Vec<AutomationEvent>) {
        while let Some(command) = self.consumer.pop() {
            self.apply_command(command);
        }

        for lane in self.parameters.values_mut() {
            lane.render_into(self.plugin_id, block_start, block_len, output);
        }

        output.sort_by_key(|event| (event.sample_offset, event.parameter));
    }

    pub fn parameter_index_by_name(&self, name: &str) -> Option<usize> {
        self.parameters.iter().find_map(|(index, lane)| {
            if lane.spec().name.eq_ignore_ascii_case(name) {
                Some(*index)
            } else {
                None
            }
        })
    }

    pub fn parameter_spec(&self, parameter: usize) -> Option<ParameterSpec> {
        self.parameters
            .get(&parameter)
            .map(|lane| lane.spec().clone())
    }
}

struct ParameterLane {
    spec: ParameterSpec,
    curve: AutomationCurve,
    recorder: AutomationRecorder,
    last_value: Option<f32>,
    needs_initial_event: bool,
}

impl ParameterLane {
    fn new(spec: ParameterSpec) -> Self {
        Self {
            spec,
            curve: AutomationCurve::new(),
            recorder: AutomationRecorder::new(AutomationWriteMode::Read),
            last_value: None,
            needs_initial_event: true,
        }
    }

    fn update_spec(&mut self, spec: ParameterSpec) {
        self.spec = spec;
        self.last_value = None;
        self.needs_initial_event = true;
        self.curve = AutomationCurve::new();
        self.recorder = AutomationRecorder::new(AutomationWriteMode::Read);
    }

    fn set_mode(&mut self, mode: AutomationWriteMode) {
        self.recorder.set_mode(mode);
    }

    fn spec(&self) -> &ParameterSpec {
        &self.spec
    }

    fn draw(&mut self, sample: u64, value: f32, shape: CurveShape) {
        let value = self.spec.clamp(value);
        self.curve.add_point(CurvePoint::new(sample, value, shape));
        self.last_value = None;
    }

    fn touch(&mut self, sample: u64, value: f32, shape: CurveShape) {
        if self.recorder.begin_touch() {
            self.draw(sample, value, shape);
        }
    }

    fn release(&mut self, sample: u64, value: Option<f32>) {
        if self.recorder.end_touch() {
            let value = value.unwrap_or(self.spec.default);
            self.draw(sample, value, CurveShape::Step);
        }
    }

    fn value_at_or_default(&self, sample: u64) -> f32 {
        let first_point_sample = self.curve.points().first().map(|point| point.sample);
        if let Some(first_sample) = first_point_sample {
            if sample < first_sample {
                return self.spec.default;
            }
        }
        self.curve
            .value_at(sample)
            .or_else(|| self.curve.last_value_before(sample))
            .unwrap_or(self.spec.default)
    }

    fn render_into(
        &mut self,
        plugin_id: PluginId,
        block_start: u64,
        block_len: u32,
        output: &mut Vec<AutomationEvent>,
    ) {
        if block_len == 0 {
            return;
        }

        const EPSILON: f32 = 1e-6;
        let mut previous_value = if let Some(last) = self.last_value {
            last
        } else if block_start == 0 {
            self.spec.default
        } else {
            self.value_at_or_default(block_start.saturating_sub(1))
        };

        for offset in 0..block_len {
            let sample = block_start + offset as u64;
            let value = self.value_at_or_default(sample);
            let should_emit = if offset == 0 {
                self.needs_initial_event || (value - previous_value).abs() > EPSILON
            } else {
                (value - previous_value).abs() > EPSILON
            };

            if should_emit {
                output.push(AutomationEvent {
                    plugin_id,
                    parameter: self.spec.index,
                    value,
                    sample_offset: offset,
                });
                previous_value = value;
            }
        }

        self.last_value = Some(previous_value);
        self.needs_initial_event = false;
    }
}
