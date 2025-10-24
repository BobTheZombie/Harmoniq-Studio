#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationWriteMode {
    Read,
    Write,
    Touch,
    Latch,
}

#[derive(Debug, Clone)]
pub struct AutomationRecorder {
    mode: AutomationWriteMode,
    touching: bool,
    latched: bool,
}

impl AutomationRecorder {
    pub fn new(mode: AutomationWriteMode) -> Self {
        Self {
            mode,
            touching: false,
            latched: false,
        }
    }

    pub fn mode(&self) -> AutomationWriteMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: AutomationWriteMode) {
        self.mode = mode;
        if !matches!(mode, AutomationWriteMode::Latch) {
            self.latched = false;
        }
        if !matches!(
            mode,
            AutomationWriteMode::Touch | AutomationWriteMode::Latch
        ) {
            self.touching = false;
        }
    }

    pub fn begin_touch(&mut self) -> bool {
        match self.mode {
            AutomationWriteMode::Read => false,
            AutomationWriteMode::Write => true,
            AutomationWriteMode::Touch => {
                self.touching = true;
                true
            }
            AutomationWriteMode::Latch => {
                self.latched = true;
                self.touching = true;
                true
            }
        }
    }

    pub fn end_touch(&mut self) -> bool {
        match self.mode {
            AutomationWriteMode::Read => false,
            AutomationWriteMode::Write => false,
            AutomationWriteMode::Touch => {
                let was_touching = self.touching;
                self.touching = false;
                was_touching
            }
            AutomationWriteMode::Latch => {
                self.touching = false;
                false
            }
        }
    }

    pub fn is_touching(&self) -> bool {
        self.touching
    }

    pub fn is_latched(&self) -> bool {
        self.latched
    }

    pub fn can_write(&self) -> bool {
        match self.mode {
            AutomationWriteMode::Read => false,
            AutomationWriteMode::Write => true,
            AutomationWriteMode::Touch => self.touching,
            AutomationWriteMode::Latch => self.touching || self.latched,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_mode_requires_active_touch() {
        let mut recorder = AutomationRecorder::new(AutomationWriteMode::Touch);
        assert!(!recorder.can_write());
        assert!(recorder.begin_touch());
        assert!(recorder.can_write());
        assert!(recorder.end_touch());
        assert!(!recorder.can_write());
    }

    #[test]
    fn latch_mode_sticks_after_touch() {
        let mut recorder = AutomationRecorder::new(AutomationWriteMode::Latch);
        assert!(recorder.begin_touch());
        assert!(recorder.can_write());
        assert!(!recorder.end_touch());
        assert!(recorder.can_write());
    }
}
