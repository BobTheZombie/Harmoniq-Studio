use eframe::egui::{self, ComboBox, RichText};
use harmoniq_midi::{config::MidiSettings, device::MidiInputConfig};

use crate::midi;

/// UI panel for managing MIDI devices.
pub struct MidiDevicesPanel {
    settings: MidiSettings,
    is_open: bool,
    available_ports: Vec<String>,
    ports_error: Option<String>,
    pending_refresh: bool,
}

impl Default for MidiDevicesPanel {
    fn default() -> Self {
        Self {
            settings: MidiSettings::default(),
            is_open: false,
            available_ports: Vec::new(),
            ports_error: None,
            pending_refresh: false,
        }
    }
}

impl MidiDevicesPanel {
    /// Open the panel with the provided settings snapshot.
    pub fn open(&mut self, settings: MidiSettings) {
        self.settings = settings;
        self.is_open = true;
        self.pending_refresh = true;
        self.ports_error = None;
    }

    /// Close the panel.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Whether the panel is currently visible.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Render the panel contents.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<MidiSettings> {
        if !self.is_open {
            return None;
        }

        if self.pending_refresh {
            self.refresh_ports();
        }

        let mut updated = None;
        let mut open_flag = self.is_open;
        egui::Window::new("MIDI Devices")
            .collapsible(false)
            .resizable(true)
            .open(&mut open_flag)
            .show(ctx, |ui| {
                ui.heading("MIDI inputs");
                ui.label(RichText::new(
                    "Enable or configure hardware MIDI devices available to Harmoniq Studio.",
                ))
                .wrap(true)
                .color(ui.visuals().weak_text_color());
                ui.add_space(8.0);

                if let Some(err) = &self.ports_error {
                    ui.colored_label(ui.visuals().warn_fg_color, err);
                    ui.add_space(6.0);
                }

                ui.horizontal(|ui| {
                    if ui.button("Rescan inputs").clicked() {
                        self.pending_refresh = true;
                    }
                    if self.pending_refresh {
                        ui.spinner();
                    }
                });

                ui.add_space(6.0);

                if self.available_ports.is_empty() {
                    ui.label(RichText::new("No MIDI inputs detected").italics());
                }

                let mut remove_index = None;
                for (index, input) in self.settings.inputs.iter_mut().enumerate() {
                    self.ensure_port_valid(input);
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut input.enabled, "Enabled");
                            let label = if input.name.is_empty() {
                                format!("Input #{}", index + 1)
                            } else {
                                input.name.clone()
                            };
                            ui.label(RichText::new(label).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Remove").clicked() {
                                        remove_index = Some(index);
                                    }
                                },
                            );
                        });

                        ui.add_space(4.0);

                        egui::Grid::new(format!("midi_input_grid_{index}"))
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |grid| {
                                grid.label("Port");
                                self.port_selector(grid, input, index);

                                grid.label("Transpose");
                                grid.add(
                                    egui::Slider::new(&mut input.transpose, -24..=24).suffix(" st"),
                                );

                                grid.label("Channel filter");
                                self.channel_filter_selector(grid, input, index);

                                grid.label("MPE mode");
                                grid.checkbox(&mut input.mpe, "Enable MPE messages");

                                grid.label("Aftertouch");
                                grid.checkbox(&mut input.aftertouch, "Forward channel pressure");
                            });
                    });
                    ui.add_space(8.0);
                }

                if let Some(idx) = remove_index {
                    self.settings.inputs.remove(idx);
                }

                if ui
                    .add_enabled(
                        !self.available_ports.is_empty(),
                        egui::Button::new("Add MIDI input"),
                    )
                    .clicked()
                {
                    if let Some(config) = self.create_config_for_first_port() {
                        self.settings.inputs.push(config);
                    }
                }

                ui.separator();
                ui.checkbox(
                    &mut self.settings.qwerty_enabled,
                    "Enable QWERTY keyboard fallback",
                );

                ui.add_space(8.0);

                if ui.button("Save").clicked() {
                    updated = Some(self.settings.clone());
                }
            });
        self.is_open = open_flag;
        updated
    }

    fn refresh_ports(&mut self) {
        match midi::list_midi_inputs() {
            Ok(ports) => {
                self.available_ports = ports;
                self.ports_error = None;
            }
            Err(err) => {
                self.available_ports.clear();
                self.ports_error = Some(err.to_string());
            }
        }
        self.pending_refresh = false;
    }

    fn ensure_port_valid(&self, input: &mut MidiInputConfig) {
        if self.available_ports.is_empty() {
            return;
        }

        if input.port_index >= self.available_ports.len() {
            input.port_index = self.available_ports.len() - 1;
        }

        if input.name.is_empty() {
            if let Some(name) = self.available_ports.get(input.port_index) {
                input.name = name.clone();
            }
        }
    }

    fn port_selector(&self, ui: &mut egui::Ui, input: &mut MidiInputConfig, idx: usize) {
        let selected_label = self
            .available_ports
            .get(input.port_index)
            .map(|name| format!("#{:02} {name}", input.port_index))
            .unwrap_or_else(|| format!("Port {}", input.port_index));

        ComboBox::from_id_source(("midi_port", idx))
            .selected_text(selected_label)
            .show_ui(ui, |combo| {
                for (port_index, name) in self.available_ports.iter().enumerate() {
                    if combo
                        .selectable_label(input.port_index == port_index, format!("#{:02} {name}"))
                        .clicked()
                    {
                        input.port_index = port_index;
                        input.name = name.clone();
                    }
                }
            });
    }

    fn channel_filter_selector(&self, ui: &mut egui::Ui, input: &mut MidiInputConfig, idx: usize) {
        let selected = input.channel_filter.unwrap_or(0);
        let label = if selected == 0 {
            "All channels".to_string()
        } else {
            format!("Channel {selected}")
        };

        ComboBox::from_id_source(("midi_channel", idx))
            .selected_text(label)
            .show_ui(ui, |combo| {
                if combo
                    .selectable_label(selected == 0, "All channels")
                    .clicked()
                {
                    input.channel_filter = None;
                }
                for channel in 1..=16 {
                    if combo
                        .selectable_label(selected == channel, format!("Channel {channel}"))
                        .clicked()
                    {
                        input.channel_filter = Some(channel as u8);
                    }
                }
            });
    }

    fn create_config_for_first_port(&self) -> Option<MidiInputConfig> {
        let name = self.available_ports.first()?.clone();
        Some(MidiInputConfig {
            enabled: true,
            name,
            port_index: 0,
            ..MidiInputConfig::default()
        })
    }
}
