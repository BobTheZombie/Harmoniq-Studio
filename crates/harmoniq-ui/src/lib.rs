//! Graphical user interface for the Harmoniq Studio application.

use eframe::egui::{self, Key};
use egui_extras::TableBuilder;
use harmoniq_engine::{Engine, EngineCmd, EngineHandle, EngineSnapshot};
use harmoniq_utils::db::{db_to_gain, gain_to_db};

/// Root egui application controlling the Harmoniq engine.
pub struct HarmoniqUiApp {
    engine: EngineHandle,
    #[allow(dead_code)]
    engine_runtime: EngineRuntimeGuard,
    mixer_visible: bool,
    latest_snapshot: Option<EngineSnapshot>,
    mixer_ui: MixerUiState,
    tempo_bpm: f32,
}

impl HarmoniqUiApp {
    /// Creates a new UI application from an engine handle.
    pub fn new(engine: EngineHandle, runtime: Engine) -> Self {
        Self {
            engine,
            engine_runtime: EngineRuntimeGuard::new(runtime),
            mixer_visible: true,
            latest_snapshot: None,
            mixer_ui: MixerUiState::default(),
            tempo_bpm: 128.0,
        }
    }
}

impl eframe::App for HarmoniqUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.key_pressed(Key::F9)) {
            self.mixer_visible = !self.mixer_visible;
        }

        if let Some(snapshot) = self.engine.latest_snapshot() {
            self.tempo_bpm = snapshot.transport.tempo.bpm;
            self.mixer_ui.ensure_size(snapshot.mixer.len());
            for (index, channel) in snapshot.mixer.iter().enumerate() {
                self.mixer_ui.gain_db[index] = gain_to_db(channel.gain);
                self.mixer_ui.pan[index] = channel.pan;
                self.mixer_ui.mute[index] = channel.mute;
            }
            self.latest_snapshot = Some(snapshot);
        }

        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let playing = self
                    .latest_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.transport.playing)
                    .unwrap_or(false);
                if ui.button(if playing { "Pause" } else { "Play" }).clicked() {
                    self.engine.submit(EngineCmd::TogglePlay);
                }
                if ui.button("Stop").clicked() {
                    self.engine.submit(EngineCmd::Stop);
                }
                ui.label("Tempo");
                let mut tempo = self.tempo_bpm;
                if ui
                    .add(
                        egui::DragValue::new(&mut tempo)
                            .speed(0.1)
                            .clamp_range(20.0..=300.0),
                    )
                    .changed()
                {
                    self.tempo_bpm = tempo;
                    self.engine.submit(EngineCmd::SetTempo(tempo));
                }
                if let Some(snapshot) = &self.latest_snapshot {
                    ui.separator();
                    ui.label(format!("Position: {} samples", snapshot.transport.position));
                }
            });
        });

        egui::SidePanel::left("browser").show(ctx, |ui| {
            ui.heading("Browser");
            ui.separator();
            ui.collapsing("Plugins", |ui| {
                for entry in [
                    "Analog Dreams",
                    "FM Fusion",
                    "Granular Cloud",
                    "West Coast Lead",
                    "Subharmonic 808",
                    "Edison Sampler",
                ] {
                    ui.label(entry);
                }
            });
            ui.separator();
            ui.collapsing("File System", |ui| {
                ui.label("/home/user/Music");
                ui.label("/home/user/Samples");
            });
        });

        if self.mixer_visible {
            egui::SidePanel::right("mixer")
                .default_width(360.0)
                .show(ctx, |ui| {
                    ui.heading("Mixer");
                    ui.separator();
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        if let Some(snapshot) = &self.latest_snapshot {
                            TableBuilder::new(ui)
                                .striped(true)
                                .column(egui_extras::Column::auto())
                                .header(20.0, |mut header| {
                                    header.col(|ui| {
                                        ui.label("Channel");
                                    });
                                    header.col(|ui| {
                                        ui.label("Gain");
                                    });
                                    header.col(|ui| {
                                        ui.label("Pan");
                                    });
                                    header.col(|ui| {
                                        ui.label("Mute");
                                    });
                                    header.col(|ui| {
                                        ui.label("Peak");
                                    });
                                    header.col(|ui| {
                                        ui.label("RMS");
                                    });
                                    header.col(|ui| {
                                        ui.label("Latency");
                                    });
                                })
                                .body(|mut body| {
                                    for (index, channel) in snapshot.mixer.iter().enumerate() {
                                        body.row(24.0, |mut row| {
                                            row.col(|ui| {
                                                ui.label(&channel.name);
                                            });
                                            row.col(|ui| {
                                                let mut gain = self.mixer_ui.gain_db[index];
                                                if ui
                                                    .add(
                                                        egui::Slider::new(&mut gain, -60.0..=12.0)
                                                            .text("dB"),
                                                    )
                                                    .changed()
                                                {
                                                    self.mixer_ui.gain_db[index] = gain;
                                                    self.engine.submit(EngineCmd::SetTrackGain {
                                                        track: index,
                                                        gain: db_to_gain(gain),
                                                    });
                                                }
                                            });
                                            row.col(|ui| {
                                                let mut pan = self.mixer_ui.pan[index];
                                                if ui
                                                    .add(
                                                        egui::Slider::new(&mut pan, -1.0..=1.0)
                                                            .text("Pan"),
                                                    )
                                                    .changed()
                                                {
                                                    self.mixer_ui.pan[index] = pan;
                                                    self.engine.submit(EngineCmd::SetTrackPan {
                                                        track: index,
                                                        pan,
                                                    });
                                                }
                                            });
                                            row.col(|ui| {
                                                let mut mute = self.mixer_ui.mute[index];
                                                if ui.checkbox(&mut mute, "").changed() {
                                                    self.mixer_ui.mute[index] = mute;
                                                    self.engine.submit(EngineCmd::SetTrackMute {
                                                        track: index,
                                                        mute,
                                                    });
                                                }
                                            });
                                            row.col(|ui| {
                                                ui.label(format!("{:.2}", channel.peak));
                                            });
                                            row.col(|ui| {
                                                ui.label(format!("{:.2}", channel.rms));
                                            });
                                            row.col(|ui| {
                                                ui.label(format!("{} samples", channel.latency));
                                            });
                                        });
                                    }
                                });
                        } else {
                            ui.label("Waiting for engine...");
                        }
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Playlist");
                ui.label("Pattern arranger and clip timeline will appear here.");
                ui.add_space(16.0);
                ui.heading("Piano Roll");
                ui.label("Draw and edit MIDI notes using the tools bar.");
            });
        });

        ctx.request_repaint();
    }
}

/// Cached mixer UI state.
#[derive(Default)]
struct MixerUiState {
    gain_db: Vec<f32>,
    pan: Vec<f32>,
    mute: Vec<bool>,
}

impl MixerUiState {
    fn ensure_size(&mut self, len: usize) {
        if self.gain_db.len() < len {
            self.gain_db.resize(len, 0.0);
            self.pan.resize(len, 0.0);
            self.mute.resize(len, false);
        }
    }
}

/// Keeps the audio engine alive for the duration of the UI.
pub struct EngineRuntimeGuard {
    #[allow(dead_code)]
    engine: Engine,
}

impl EngineRuntimeGuard {
    fn new(engine: Engine) -> Self {
        Self { engine }
    }
}
