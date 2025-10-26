use std::time::{Duration, Instant};

#[allow(dead_code)]
pub struct FramePacer {
    target_interval: Duration,
    last_tick: Instant,
    vsync_enabled: bool,
}

impl FramePacer {
    #[allow(dead_code)]
    pub fn new(target_hz: f32) -> Self {
        let interval = if target_hz <= 0.0 {
            Duration::from_millis(16)
        } else {
            Duration::from_secs_f32(1.0 / target_hz)
        };
        Self {
            target_interval: interval,
            last_tick: Instant::now(),
            vsync_enabled: true,
        }
    }

    #[allow(dead_code)]
    pub fn set_vsync(&mut self, enabled: bool) {
        self.vsync_enabled = enabled;
    }

    #[allow(dead_code)]
    pub fn target_interval(&self) -> Duration {
        self.target_interval
    }

    #[allow(dead_code)]
    pub fn update_interval(&mut self, hz: f32) {
        if hz > 0.0 {
            self.target_interval = Duration::from_secs_f32(1.0 / hz);
        }
    }

    #[allow(dead_code)]
    pub fn should_tick(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_tick) >= self.target_interval {
            self.last_tick = now;
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn sleep_hint(&self) -> Duration {
        if self.vsync_enabled {
            Duration::ZERO
        } else {
            self.target_interval
        }
    }
}
