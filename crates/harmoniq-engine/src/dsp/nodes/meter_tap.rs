use std::sync::Arc;

use parking_lot::Mutex;

use crate::buffer::AudioBuffer;

#[derive(Clone, Copy, Debug)]
pub struct MeterReadout {
    pub true_peak_dbfs: f32,
    pub short_term_lufs: f32,
    pub phase_correlation: f32,
}

impl Default for MeterReadout {
    fn default() -> Self {
        Self {
            true_peak_dbfs: f32::NEG_INFINITY,
            short_term_lufs: f32::NEG_INFINITY,
            phase_correlation: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeterHandle {
    inner: Arc<Mutex<MeterReadout>>,
}

impl MeterHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MeterReadout::default())),
        }
    }

    pub fn read(&self) -> MeterReadout {
        *self.inner.lock()
    }

    pub fn set(&self, readout: MeterReadout) {
        *self.inner.lock() = readout;
    }
}

#[derive(Clone, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn highpass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let omega = 2.0 * core::f32::consts::PI * (freq / sr);
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q.max(0.1));
        let b0 = (1.0 + cos) * 0.5;
        let b1 = -(1.0 + cos);
        let b2 = (1.0 + cos) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    fn highshelf(sample_rate: f32, freq: f32, gain_db: f32, slope: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let omega = 2.0 * core::f32::consts::PI * (freq / sr);
        let sin = omega.sin();
        let cos = omega.cos();
        let a = 10.0f32.powf(gain_db * 0.05);
        let beta = (a.sqrt() / slope.max(1e-6)).clamp(1e-6, 1e6);
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos + beta * sin);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos - beta * sin);
        let a0 = (a + 1.0) - (a - 1.0) * cos + beta * sin;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos);
        let a2 = (a + 1.0) - (a - 1.0) * cos - beta * sin;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = if a0.abs() < 1e-6 { 1.0 } else { 1.0 / a0 };
        Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let out = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * out + self.z2;
        self.z2 = self.b2 * input - self.a2 * out;
        out
    }
}

#[derive(Clone, Debug)]
struct KWeighting {
    pre_highpass: Biquad,
    high_shelf: Biquad,
}

impl KWeighting {
    fn new(sample_rate: f32) -> Self {
        let mut hp = Biquad::highpass(sample_rate, 40.0, 0.5);
        let mut shelf = Biquad::highshelf(sample_rate, 4000.0, 4.0, 1.0);
        hp.reset();
        shelf.reset();
        Self {
            pre_highpass: hp,
            high_shelf: shelf,
        }
    }

    fn reset(&mut self) {
        self.pre_highpass.reset();
        self.high_shelf.reset();
    }

    fn process(&mut self, input: f32) -> f32 {
        let filtered = self.pre_highpass.process(input);
        self.high_shelf.process(filtered)
    }
}

#[derive(Clone, Debug)]
pub struct MeterTapNode {
    handle: MeterHandle,
    sample_rate: f32,
    window_samples: usize,
    ring: Vec<f32>,
    ring_index: usize,
    ring_sum: f64,
    k_filters: Vec<KWeighting>,
    prev_samples: Vec<f32>,
    max_true_peak: f32,
}

impl MeterTapNode {
    pub fn new(sample_rate: f32, handle: MeterHandle) -> Self {
        let window = (sample_rate * 3.0).round() as usize;
        Self {
            handle,
            sample_rate,
            window_samples: window.max(1),
            ring: vec![0.0; window.max(1)],
            ring_index: 0,
            ring_sum: 0.0,
            k_filters: Vec::new(),
            prev_samples: Vec::new(),
            max_true_peak: 0.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32, channels: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.window_samples = (self.sample_rate * 3.0).round() as usize;
        if self.window_samples == 0 {
            self.window_samples = 1;
        }
        self.ring.resize(self.window_samples, 0.0);
        self.ring.fill(0.0);
        self.ring_index = 0;
        self.ring_sum = 0.0;
        self.k_filters = (0..channels)
            .map(|_| KWeighting::new(self.sample_rate))
            .collect();
        self.prev_samples = vec![0.0; channels];
        self.max_true_peak = 0.0;
    }

    pub fn handle(&self) -> MeterHandle {
        self.handle.clone()
    }

    pub fn process_buffer(&mut self, buffer: &AudioBuffer) {
        let channels = buffer.channel_count();
        if channels == 0 || buffer.is_empty() {
            return;
        }
        if self.k_filters.len() != channels {
            self.prepare(self.sample_rate, channels);
        }

        let frames = buffer.len();
        let mut sum_lr = 0.0;
        let mut sum_l2 = 0.0;
        let mut sum_r2 = 0.0;
        let mut ring_index = self.ring_index;
        let mut ring_sum = self.ring_sum;
        let ring_len = self.window_samples;

        for frame in 0..frames {
            let mut weighted_energy = 0.0;
            for ch in 0..channels {
                let sample = buffer.channel(ch)[frame];
                let prev = self.prev_samples[ch];
                let diff = sample - prev;
                for step in 1..4 {
                    let interp = prev + diff * (step as f32 / 4.0);
                    let abs = interp.abs();
                    if abs > self.max_true_peak {
                        self.max_true_peak = abs;
                    }
                }
                let abs = sample.abs();
                if abs > self.max_true_peak {
                    self.max_true_peak = abs;
                }
                self.prev_samples[ch] = sample;
                let weighted = self.k_filters[ch].process(sample);
                weighted_energy += weighted * weighted;
            }
            let average = weighted_energy / channels as f32;
            if ring_len > 0 {
                let old = self.ring[ring_index];
                ring_sum -= old as f64;
                self.ring[ring_index] = average;
                ring_sum += average as f64;
                ring_index = (ring_index + 1) % ring_len;
            }

            if channels >= 2 {
                let left = buffer.channel(0)[frame];
                let right = buffer.channel(1)[frame];
                sum_lr += (left * right) as f64;
                sum_l2 += (left * left) as f64;
                sum_r2 += (right * right) as f64;
            }
        }

        self.ring_index = ring_index;
        self.ring_sum = ring_sum;

        let mean_square = if ring_len > 0 {
            ring_sum / ring_len as f64
        } else {
            0.0
        };
        let lufs = if mean_square > 0.0 {
            -0.691 + 10.0 * mean_square.log10()
        } else {
            f64::NEG_INFINITY
        } as f32;
        let true_peak = if self.max_true_peak > 0.0 {
            20.0 * self.max_true_peak.log10()
        } else {
            f32::NEG_INFINITY
        };
        let correlation = if sum_l2 > 0.0 && sum_r2 > 0.0 {
            (sum_lr / (sum_l2 * sum_r2).sqrt()).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };
        self.handle.set(MeterReadout {
            true_peak_dbfs: true_peak,
            short_term_lufs: lufs,
            phase_correlation: correlation,
        });
        self.max_true_peak = 0.0;
    }
}
