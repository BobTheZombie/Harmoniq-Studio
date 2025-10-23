use eframe::egui::{self, RichText};
use harmoniq_engine::{BufferConfig, ChannelLayout};
use harmoniq_ui::HarmoniqPalette;

use crate::audio::{
    available_backends, available_output_devices, AudioBackend, AudioRuntimeOptions,
    OutputDeviceInfo,
};

#[derive(Clone, Debug)]
pub enum AudioSettingsFeedback {
    Info(String),
    Error(String),
}

impl AudioSettingsFeedback {
    pub fn message(&self) -> &str {
        match self {
            Self::Info(message) | Self::Error(message) => message,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

#[derive(Clone, Debug)]
pub struct ActiveAudioSummary {
    pub backend: AudioBackend,
    pub device_name: String,
    pub host_label: Option<String>,
}

pub enum AudioSettingsAction {
    Apply {
        config: BufferConfig,
        runtime: AudioRuntimeOptions,
    },
}

pub struct AudioSettingsPanel {
    open: bool,
    available_backends: Vec<(AudioBackend, String)>,
    available_devices: Vec<OutputDeviceInfo>,
    backend: AudioBackend,
    selected_device: Option<String>,
    enable_audio: bool,
    sample_rate: f32,
    buffer_size: usize,
    midi_input: Option<String>,
    layout: ChannelLayout,
    devices_error: Option<String>,
    pending_device_refresh: bool,
    feedback: Option<AudioSettingsFeedback>,
    feedback_dirty: bool,
}

impl AudioSettingsPanel {
    pub fn new(config: &BufferConfig, runtime: &AudioRuntimeOptions) -> Self {
        let mut panel = Self {
            open: false,
            available_backends: Vec::new(),
            available_devices: Vec::new(),
            backend: runtime.backend(),
            selected_device: runtime.output_device().map(|device| device.to_string()),
            enable_audio: runtime.is_enabled(),
            sample_rate: config.sample_rate,
            buffer_size: config.block_size,
            midi_input: runtime.midi_input.clone(),
            layout: config.layout,
            devices_error: None,
            pending_device_refresh: true,
            feedback: None,
            feedback_dirty: false,
        };
        panel.refresh_backends();
        panel
    }

    pub fn open(&mut self, config: &BufferConfig, runtime: &AudioRuntimeOptions) {
        self.open = true;
        self.backend = runtime.backend();
        self.selected_device = runtime.output_device().map(|device| device.to_string());
        self.enable_audio = runtime.is_enabled();
        self.sample_rate = config.sample_rate;
        self.buffer_size = config.block_size;
        self.midi_input = runtime.midi_input.clone();
        self.layout = config.layout;
        self.devices_error = None;
        self.pending_device_refresh = true;
        self.feedback = None;
        self.feedback_dirty = false;
        self.refresh_backends();
    }

    fn refresh_backends(&mut self) {
        self.available_backends = available_backends();
        if !self
            .available_backends
            .iter()
            .any(|(backend, _)| *backend == self.backend)
        {
            self.available_backends
                .insert(0, (self.backend, self.backend.to_string()));
        }
    }

    fn refresh_devices(&mut self) {
        match available_output_devices(self.backend) {
            Ok(devices) => {
                self.available_devices = devices;
                self.devices_error = None;
            }
            Err(err) => {
                self.available_devices.clear();
                self.devices_error = Some(format!("{}", err));
            }
        }

        if let Some(selected) = self.selected_device.clone() {
            if !self
                .available_devices
                .iter()
                .any(|device| device.id == selected)
            {
                self.selected_device = None;
            }
        }
        self.pending_device_refresh = false;
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        palette: &HarmoniqPalette,
        active_audio: Option<&ActiveAudioSummary>,
        last_error: Option<&str>,
    ) -> Option<AudioSettingsAction> {
        if !self.open {
            return None;
        }

        if self.available_backends.is_empty() {
            self.refresh_backends();
        }
        if self.pending_device_refresh {
            self.refresh_devices();
        }

        let prev_backend = self.backend;
        let mut apply_clicked = false;
        let mut open_flag = self.open;

        egui::Window::new("Audio Settings")
            .collapsible(false)
            .resizable(false)
            .open(&mut open_flag)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.heading("Configure audio devices");
                ui.add_space(8.0);

                if let Some(feedback) = &self.feedback {
                    let (text, color) = match feedback {
                        AudioSettingsFeedback::Info(message) => (message.as_str(), palette.success),
                        AudioSettingsFeedback::Error(message) => {
                            (message.as_str(), palette.warning)
                        }
                    };
                    ui.colored_label(color, text);
                    ui.add_space(6.0);
                }

                if let Some(summary) = active_audio {
                    let mut info = format!("Active: {}", summary.device_name);
                    if let Some(host) = summary.host_label.as_deref() {
                        info.push_str(&format!(" via {host}"));
                    } else {
                        info.push_str(&format!(" via {}", summary.backend));
                    }
                    ui.label(RichText::new(info).color(palette.text_muted));
                } else if !self.enable_audio {
                    ui.label(RichText::new("Audio output is disabled").color(palette.text_muted));
                } else if let Some(err) = last_error {
                    ui.colored_label(palette.warning, format!("Last error: {err}"));
                }

                ui.add_space(8.0);
                ui.checkbox(&mut self.enable_audio, "Enable realtime audio");
                ui.add_space(4.0);

                egui::Grid::new("audio_settings_device_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |grid| {
                        grid.label("Audio backend");
                        let selected_backend = self.backend_label(self.backend);
                        egui::ComboBox::from_id_source("audio_backend_combo")
                            .selected_text(selected_backend)
                            .show_ui(grid, |ui| {
                                for (backend, label) in &self.available_backends {
                                    ui.selectable_value(&mut self.backend, *backend, label);
                                }
                            });
                        grid.end_row();

                        grid.label("Output device");
                        let mut selection = self.selected_device.clone();
                        let selected_device = self.device_label();
                        egui::ComboBox::from_id_source("audio_device_combo")
                            .selected_text(selected_device)
                            .show_ui(grid, |ui| {
                                ui.selectable_value(&mut selection, None, "System default");
                                for device in &self.available_devices {
                                    ui.selectable_value(
                                        &mut selection,
                                        Some(device.id.clone()),
                                        device.label.clone(),
                                    );
                                }
                            });
                        if selection != self.selected_device {
                            self.selected_device = selection;
                        }
                        grid.end_row();

                        grid.label(" ");
                        if ui.button("Refresh devices").clicked() {
                            self.pending_device_refresh = true;
                        }
                        grid.end_row();
                    });

                if let Some(error) = &self.devices_error {
                    ui.add_space(6.0);
                    ui.colored_label(palette.warning, format!("Device query failed: {error}"));
                }

                ui.add_space(10.0);
                ui.heading("Performance");
                ui.add_space(4.0);

                egui::Grid::new("audio_settings_performance_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |grid| {
                        grid.label("Sample rate (Hz)");
                        grid.add(
                            egui::DragValue::new(&mut self.sample_rate)
                                .clamp_range(8_000.0..=192_000.0)
                                .speed(100.0),
                        );
                        grid.end_row();

                        grid.label("Buffer size (frames)");
                        grid.add(
                            egui::DragValue::new(&mut self.buffer_size)
                                .clamp_range(16..=8_192)
                                .speed(16.0),
                        );
                        grid.end_row();

                        grid.label("Estimated latency");
                        let latency_ms = if self.sample_rate > 0.0 {
                            (self.buffer_size as f32 / self.sample_rate) * 1000.0
                        } else {
                            0.0
                        };
                        grid.label(format!("{latency_ms:.1} ms"));
                        grid.end_row();
                    });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply_clicked = true;
                    }
                    if ui.button("Close").clicked() {
                        self.open = false;
                    }
                });
            });

        self.open = open_flag && self.open;

        if self.backend != prev_backend {
            self.pending_device_refresh = true;
            self.feedback = None;
            self.feedback_dirty = false;
        }

        if apply_clicked {
            return self.prepare_apply_action();
        }

        None
    }

    fn backend_label(&self, backend: AudioBackend) -> String {
        self.available_backends
            .iter()
            .find(|(candidate, _)| *candidate == backend)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| backend.to_string())
    }

    fn device_label(&self) -> String {
        match &self.selected_device {
            Some(id) => self
                .available_devices
                .iter()
                .find(|device| device.id == *id)
                .map(|device| device.label.clone())
                .unwrap_or_else(|| format!("Custom ({id})")),
            None => "System default".to_string(),
        }
    }

    fn prepare_apply_action(&mut self) -> Option<AudioSettingsAction> {
        if self.sample_rate <= 0.0 {
            self.feedback = Some(AudioSettingsFeedback::Error(
                "Sample rate must be greater than zero".to_string(),
            ));
            self.feedback_dirty = true;
            return None;
        }
        if self.buffer_size == 0 {
            self.feedback = Some(AudioSettingsFeedback::Error(
                "Buffer size must be greater than zero".to_string(),
            ));
            self.feedback_dirty = true;
            return None;
        }

        let mut runtime =
            AudioRuntimeOptions::new(self.backend, self.midi_input.clone(), self.enable_audio);
        runtime.set_output_device(self.selected_device.clone());
        let config = BufferConfig::new(self.sample_rate, self.buffer_size, self.layout);
        self.feedback = None;
        self.feedback_dirty = false;
        Some(AudioSettingsAction::Apply { config, runtime })
    }

    pub fn on_apply_result(
        &mut self,
        result: anyhow::Result<()>,
        config: &BufferConfig,
        runtime: &AudioRuntimeOptions,
    ) {
        match result {
            Ok(()) => {
                self.feedback = Some(AudioSettingsFeedback::Info(
                    "Audio configuration updated".to_string(),
                ));
                self.feedback_dirty = true;
                self.sample_rate = config.sample_rate;
                self.buffer_size = config.block_size;
                self.backend = runtime.backend();
                self.enable_audio = runtime.is_enabled();
                self.selected_device = runtime.output_device().map(|device| device.to_string());
                self.midi_input = runtime.midi_input.clone();
                self.layout = config.layout;
                self.pending_device_refresh = true;
                self.refresh_backends();
            }
            Err(err) => {
                self.feedback = Some(AudioSettingsFeedback::Error(format!(
                    "Failed to apply audio settings: {err:#}"
                )));
                self.feedback_dirty = true;
            }
        }
    }

    pub fn take_status_message(&mut self) -> Option<AudioSettingsFeedback> {
        if self.feedback_dirty {
            self.feedback_dirty = false;
            self.feedback.clone()
        } else {
            None
        }
    }
}
