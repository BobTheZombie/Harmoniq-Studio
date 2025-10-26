use egui::{self, RichText};

use crate::{HarmoniqPalette, Knob};
use harmoniq_plugins::ParametricEqPreset;

const TWO_PI: f32 = std::f32::consts::PI * 2.0;
const RESPONSE_POINTS: usize = 256;
const RESPONSE_MIN_FREQ: f32 = 20.0;
const RESPONSE_MAX_FREQ: f32 = 20_000.0;
const RESPONSE_MIN_DB: f32 = -24.0;
const RESPONSE_MAX_DB: f32 = 24.0;

#[derive(Debug, Clone, Copy)]
pub struct ControlRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ControlRange {
    pub const fn new(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ParametricEqBandKind {
    LowShelf,
    Peak,
    HighShelf,
}

pub struct ParametricEqBandParams<'a> {
    pub label: &'static str,
    pub kind: ParametricEqBandKind,
    pub enabled: &'a mut bool,
    pub frequency: &'a mut f32,
    pub freq_range: ControlRange,
    pub gain: &'a mut f32,
    pub gain_range: ControlRange,
    pub q: &'a mut f32,
    pub q_range: ControlRange,
    pub q_label: &'static str,
}

pub struct ParametricEqParams<'a> {
    pub output_gain: &'a mut f32,
    pub sample_rate: f32,
    pub bands: Vec<ParametricEqBandParams<'a>>,
    pub preset_slot: Option<&'a mut Option<usize>>,
}

struct BandSnapshot {
    kind: ParametricEqBandKind,
    enabled: bool,
    frequency: f32,
    gain: f32,
    q: f32,
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

pub fn show_parametric_eq_ui(
    ui: &mut egui::Ui,
    params: ParametricEqParams<'_>,
    palette: &HarmoniqPalette,
    presets: &[ParametricEqPreset],
) -> egui::Response {
    let mut preset_slot = params.preset_slot;
    let mut bands = params.bands;
    let output_gain = params.output_gain;
    let sample_rate = params.sample_rate.max(1.0);
    let mut preset_to_apply: Option<ParametricEqPreset> = None;
    let mut user_changed = false;

    let response = ui
        .group(|ui| {
            ui.vertical(|ui| {
                ui.heading("Parametric EQ");
                ui.add_space(6.0);

                if !presets.is_empty() {
                    if let Some(slot) = preset_slot.as_mut() {
                        let slot = &mut **slot;
                        let mut chosen = *slot;
                        if let Some(idx) = chosen {
                            if idx >= presets.len() {
                                *slot = None;
                                chosen = None;
                            }
                        }
                        let current_label = chosen
                            .and_then(|idx| presets.get(idx).map(|preset| preset.name))
                            .unwrap_or("Custom");

                        egui::ComboBox::from_label("Factory Preset")
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                let custom_response =
                                    ui.selectable_value(&mut chosen, None, "Custom");
                                if custom_response.clicked() {
                                    preset_to_apply = None;
                                }
                                for (idx, preset) in presets.iter().enumerate() {
                                    let response =
                                        ui.selectable_value(&mut chosen, Some(idx), preset.name);
                                    if response.clicked() {
                                        preset_to_apply = Some(*preset);
                                    }
                                }
                            });
                        if chosen != *slot {
                            *slot = chosen;
                        }
                        ui.add_space(6.0);
                    }
                }

                if let Some(preset) = preset_to_apply {
                    apply_preset_to_values(&preset, output_gain, &mut bands);
                }

                ui.horizontal(|ui| {
                    let response = ui.add(
                        Knob::new(
                            output_gain,
                            RESPONSE_MIN_DB,
                            RESPONSE_MAX_DB,
                            0.0,
                            "Output",
                            palette,
                        )
                        .with_diameter(58.0),
                    );
                    if response.changed() {
                        user_changed = true;
                    }
                });

                ui.add_space(10.0);

                let snapshots: Vec<BandSnapshot> = bands
                    .iter()
                    .map(|band| BandSnapshot {
                        kind: band.kind,
                        enabled: *band.enabled,
                        frequency: (*band.frequency)
                            .clamp(band.freq_range.min, band.freq_range.max),
                        gain: (*band.gain).clamp(band.gain_range.min, band.gain_range.max),
                        q: (*band.q).clamp(band.q_range.min, band.q_range.max),
                    })
                    .collect();
                draw_response_curve(ui, sample_rate, *output_gain, &snapshots, palette);

                ui.add_space(10.0);

                let band_colors = [
                    palette.success,
                    palette.accent,
                    palette.warning,
                    palette.accent_alt,
                ];
                ui.columns(bands.len(), |columns| {
                    for (column, (band, color)) in columns
                        .iter_mut()
                        .zip(bands.iter_mut().zip(band_colors.iter().cycle()))
                    {
                        column.vertical(|ui| {
                            let toggle = ui.toggle_value(
                                band.enabled,
                                RichText::new(band.label).color(*color).strong(),
                            );
                            if toggle.changed() {
                                user_changed = true;
                            }
                            ui.add_space(6.0);
                            let freq_response = ui.add(
                                Knob::new(
                                    band.frequency,
                                    band.freq_range.min,
                                    band.freq_range.max,
                                    band.freq_range.default,
                                    "Freq",
                                    palette,
                                )
                                .with_diameter(54.0),
                            );
                            if freq_response.changed() {
                                user_changed = true;
                            }
                            ui.label(
                                RichText::new(format!("{:.0} Hz", *band.frequency))
                                    .color(palette.text_muted),
                            );

                            let gain_response = ui.add(
                                Knob::new(
                                    band.gain,
                                    band.gain_range.min,
                                    band.gain_range.max,
                                    band.gain_range.default,
                                    "Gain",
                                    palette,
                                )
                                .with_diameter(54.0),
                            );
                            if gain_response.changed() {
                                user_changed = true;
                            }
                            ui.label(
                                RichText::new(format!("{:.1} dB", *band.gain))
                                    .color(palette.text_muted),
                            );

                            let q_response = ui.add(
                                Knob::new(
                                    band.q,
                                    band.q_range.min,
                                    band.q_range.max,
                                    band.q_range.default,
                                    band.q_label,
                                    palette,
                                )
                                .with_diameter(54.0),
                            );
                            if q_response.changed() {
                                user_changed = true;
                            }
                            ui.label(
                                RichText::new(format!("{:.2}", *band.q)).color(palette.text_muted),
                            );
                        });
                    }
                });
            });
        })
        .response;

    if user_changed {
        if let Some(slot) = preset_slot.as_mut() {
            **slot = None;
        }
    }

    response
}

fn draw_response_curve(
    ui: &mut egui::Ui,
    sample_rate: f32,
    output_gain_db: f32,
    bands: &[BandSnapshot],
    palette: &HarmoniqPalette,
) {
    let width = ui.available_width();
    let height = 220.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, palette.meter_background);
    painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, palette.toolbar_outline));

    draw_grid(&painter, rect, palette);

    let log_min = RESPONSE_MIN_FREQ.log10();
    let log_max = RESPONSE_MAX_FREQ.log10();
    let db_range = RESPONSE_MAX_DB - RESPONSE_MIN_DB;

    let mut points = Vec::with_capacity(RESPONSE_POINTS + 1);
    for i in 0..=RESPONSE_POINTS {
        let t = i as f32 / RESPONSE_POINTS as f32;
        let freq = RESPONSE_MIN_FREQ * (RESPONSE_MAX_FREQ / RESPONSE_MIN_FREQ).powf(t);
        let omega = TWO_PI * (freq / sample_rate);
        let mut magnitude = db_to_gain(output_gain_db);
        for band in bands.iter().filter(|band| band.enabled) {
            let coeffs = compute_coeffs(band.kind, band.frequency, band.gain, band.q, sample_rate);
            magnitude *= biquad_magnitude(&coeffs, omega);
        }
        let log_f = freq.log10();
        let x = rect.left() + ((log_f - log_min) / (log_max - log_min)) * rect.width();
        let db = 20.0 * magnitude.max(1e-6).log10();
        let y = rect.bottom() - ((db - RESPONSE_MIN_DB) / db_range) * rect.height();
        points.push(egui::pos2(x, y));
    }

    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, palette.accent_alt),
    ));

    let band_colors = [
        palette.success,
        palette.accent,
        palette.warning,
        palette.accent_alt,
    ];
    for (idx, band) in bands.iter().enumerate() {
        if !band.enabled {
            continue;
        }
        let freq = band.frequency.clamp(RESPONSE_MIN_FREQ, RESPONSE_MAX_FREQ);
        let omega = TWO_PI * (freq / sample_rate);
        let coeffs = compute_coeffs(band.kind, band.frequency, band.gain, band.q, sample_rate);
        let band_mag = biquad_magnitude(&coeffs, omega);
        let db = 20.0 * band_mag.max(1e-6).log10() + output_gain_db;
        let log_f = freq.log10();
        let x = rect.left() + ((log_f - log_min) / (log_max - log_min)) * rect.width();
        let y = rect.bottom() - ((db - RESPONSE_MIN_DB) / db_range) * rect.height();
        let pos = egui::pos2(x, y);
        let color = band_colors[idx % band_colors.len()];
        painter.circle_filled(pos, 6.0, color);
        painter.circle_stroke(pos, 6.0, egui::Stroke::new(1.4, palette.toolbar_outline));
    }
}

fn draw_grid(painter: &egui::Painter, rect: egui::Rect, palette: &HarmoniqPalette) {
    let db_lines = [-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0];
    let db_range = RESPONSE_MAX_DB - RESPONSE_MIN_DB;
    for db in db_lines {
        let y = rect.bottom() - ((db - RESPONSE_MIN_DB) / db_range) * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, palette.toolbar_outline.gamma_multiply(0.35)),
        );
        painter.text(
            egui::pos2(rect.left() + 6.0, y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{db:.0} dB"),
            egui::FontId::proportional(11.0),
            palette.text_muted,
        );
    }

    let freqs: [f32; 10] = [
        20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0,
    ];
    let log_min = RESPONSE_MIN_FREQ.log10();
    let log_max = RESPONSE_MAX_FREQ.log10();
    for freq in freqs {
        let log_f = freq.log10();
        let x = rect.left() + ((log_f - log_min) / (log_max - log_min)) * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, palette.toolbar_outline.gamma_multiply(0.3)),
        );
        painter.text(
            egui::pos2(x, rect.bottom() - 4.0),
            egui::Align2::CENTER_BOTTOM,
            if freq >= 1_000.0 {
                format!("{:.0}k", freq / 1_000.0)
            } else {
                format!("{freq:.0}")
            },
            egui::FontId::proportional(11.0),
            palette.text_muted,
        );
    }
}

fn compute_coeffs(
    kind: ParametricEqBandKind,
    freq: f32,
    gain_db: f32,
    q: f32,
    sample_rate: f32,
) -> BiquadCoeffs {
    match kind {
        ParametricEqBandKind::LowShelf => compute_low_shelf(freq, gain_db, q, sample_rate),
        ParametricEqBandKind::HighShelf => compute_high_shelf(freq, gain_db, q, sample_rate),
        ParametricEqBandKind::Peak => compute_peak(freq, gain_db, q, sample_rate),
    }
}

fn compute_peak(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let omega = TWO_PI * (frequency / sample_rate.max(1.0));
    let cos = omega.cos();
    let sin = omega.sin();
    let q = q.max(0.05);
    let alpha = sin / (2.0 * q);
    let a = 10.0_f32.powf(gain_db / 40.0);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos;
    let a2 = 1.0 - alpha / a;

    normalize_coeffs(b0, b1, b2, a0, a1, a2)
}

fn compute_low_shelf(freq: f32, gain_db: f32, slope: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let omega = TWO_PI * (frequency / sample_rate.max(1.0));
    let cos = omega.cos();
    let sin = omega.sin();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let alpha = shelf_alpha(a, slope, sin);
    let sqrt_a = a.sqrt();
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
    let a2 = (a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha;

    normalize_coeffs(b0, b1, b2, a0, a1, a2)
}

fn compute_high_shelf(freq: f32, gain_db: f32, slope: f32, sample_rate: f32) -> BiquadCoeffs {
    let frequency = freq.max(10.0);
    let omega = TWO_PI * (frequency / sample_rate.max(1.0));
    let cos = omega.cos();
    let sin = omega.sin();
    let a = 10.0_f32.powf(gain_db / 40.0);
    let alpha = shelf_alpha(a, slope, sin);
    let sqrt_a = a.sqrt();
    let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos);
    let a2 = (a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha;

    normalize_coeffs(b0, b1, b2, a0, a1, a2)
}

fn shelf_alpha(a: f32, slope: f32, sin: f32) -> f32 {
    let slope = slope.max(0.1);
    let s = ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).max(0.0);
    sin / 2.0 * s.sqrt()
}

fn normalize_coeffs(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> BiquadCoeffs {
    let inv_a0 = 1.0 / a0.max(1e-6);
    BiquadCoeffs {
        b0: b0 * inv_a0,
        b1: b1 * inv_a0,
        b2: b2 * inv_a0,
        a1: a1 * inv_a0,
        a2: a2 * inv_a0,
    }
}

fn biquad_magnitude(coeffs: &BiquadCoeffs, omega: f32) -> f32 {
    let cos = omega.cos();
    let sin = omega.sin();
    let cos2 = (2.0 * cos * cos) - 1.0;
    let sin2 = 2.0 * sin * cos;

    let nr = coeffs.b0 + coeffs.b1 * cos + coeffs.b2 * cos2;
    let ni = -(coeffs.b1 * sin + coeffs.b2 * sin2);
    let dr = 1.0 + coeffs.a1 * cos + coeffs.a2 * cos2;
    let di = -(coeffs.a1 * sin + coeffs.a2 * sin2);

    let num = nr * nr + ni * ni;
    let den = dr * dr + di * di;
    (num / den.max(1e-12)).sqrt()
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

pub fn apply_preset_to_values(
    preset: &ParametricEqPreset,
    output_gain: &mut f32,
    bands: &mut [ParametricEqBandParams<'_>],
) {
    *output_gain = preset.output_gain;
    for (band, preset_band) in bands.iter_mut().zip(preset.bands.iter()) {
        *band.enabled = preset_band.enabled;
        *band.frequency = preset_band.frequency;
        *band.gain = preset_band.gain;
        *band.q = preset_band.q;
    }
}
