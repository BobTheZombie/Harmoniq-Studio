use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use harmoniq_engine::{MidiEvent, MidiTimestamp};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use winit::keyboard::{KeyCode, ModifiersState};

use crate::config::qwerty::{KeyboardLayout, QwertyConfigFile, SustainKey, VelocityCurveSetting};

use super::MidiInputDevice;

const QWERTY_WHITE_KEYS: [KeyCode; 7] = [
    KeyCode::KeyQ,
    KeyCode::KeyW,
    KeyCode::KeyE,
    KeyCode::KeyR,
    KeyCode::KeyT,
    KeyCode::KeyY,
    KeyCode::KeyU,
];

const QWERTY_BLACK_KEYS: [KeyCode; 5] = [
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
];

const ALT_WHITE_KEYS: [KeyCode; 8] = [
    KeyCode::KeyZ,
    KeyCode::KeyX,
    KeyCode::KeyC,
    KeyCode::KeyV,
    KeyCode::KeyB,
    KeyCode::KeyN,
    KeyCode::KeyM,
    KeyCode::Comma,
];

const ALT_BLACK_KEYS: [KeyCode; 5] = [
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyJ,
];

const PANIC_CC: u8 = 123;
const SUSTAIN_CC: u8 = 64;

fn is_velocity_modifier(modifiers: ModifiersState) -> bool {
    modifiers.control_key() || modifiers.alt_key() || modifiers.super_key()
}

fn midi_timestamp_from_instant(instant: Instant) -> MidiTimestamp {
    static BASE_INSTANT: OnceLock<Instant> = OnceLock::new();
    let base = BASE_INSTANT.get_or_init(|| {
        let now_instant = Instant::now();
        let now_timestamp = MidiTimestamp::now();
        now_instant
            .checked_sub(now_timestamp.as_duration())
            .unwrap_or(now_instant)
    });
    let duration = instant
        .checked_duration_since(*base)
        .unwrap_or_else(|| Duration::from_micros(0));
    MidiTimestamp::from_duration(duration)
}

#[derive(Debug, Clone)]
struct PendingNoteOff {
    channel: u8,
    note: u8,
}

pub struct QwertyKeyboardInput {
    name: String,
    enabled: bool,
    config: QwertyConfigFile,
    octave: i8,
    channel: u8,
    held_keys: HashSet<KeyCode>,
    sustained: HashSet<KeyCode>,
    pending_off: VecDeque<PendingNoteOff>,
    sustain_latched: bool,
    velocity_index: usize,
    velocity_cycle: Vec<u8>,
    queue_tx: HeapProducer<(Instant, MidiEvent)>,
    queue_rx: HeapConsumer<(Instant, MidiEvent)>,
    note_lookup: HashMap<KeyCode, u8>,
}

impl fmt::Debug for QwertyKeyboardInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QwertyKeyboardInput")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("config", &self.config)
            .field("octave", &self.octave)
            .field("channel", &self.channel)
            .field("held_keys", &self.held_keys)
            .field("sustained", &self.sustained)
            .field("pending_off", &self.pending_off)
            .field("sustain_latched", &self.sustain_latched)
            .field("velocity_index", &self.velocity_index)
            .field("velocity_cycle", &self.velocity_cycle)
            .field("note_lookup", &self.note_lookup)
            .finish()
    }
}

impl QwertyKeyboardInput {
    pub fn new(config: QwertyConfigFile) -> Self {
        let capacity = 512;
        let ring = HeapRb::new(capacity);
        let (queue_tx, queue_rx) = ring.split();
        let mut device = Self {
            name: "QWERTY Keyboard".to_string(),
            enabled: config.enabled,
            channel: (config.channel.saturating_sub(1)).min(15),
            octave: config.octave.clamp(1, 7),
            config,
            held_keys: HashSet::new(),
            sustained: HashSet::new(),
            pending_off: VecDeque::new(),
            sustain_latched: false,
            velocity_index: 5,
            velocity_cycle: vec![16, 32, 48, 64, 80, 96, 112, 120, 124, 127],
            queue_tx,
            queue_rx,
            note_lookup: HashMap::new(),
        };
        device.rebuild_mapping();
        device
    }

    fn rebuild_mapping(&mut self) {
        self.note_lookup.clear();
        let base_note = self.octave.saturating_mul(12) as u8;
        for (index, key) in QWERTY_WHITE_KEYS.iter().enumerate() {
            self.note_lookup.insert(*key, base_note + index as u8);
        }
        for (index, key) in QWERTY_BLACK_KEYS.iter().enumerate() {
            let note = base_note + [1, 3, 6, 8, 10][index];
            self.note_lookup.insert(*key, note);
        }
        if matches!(self.config.layout, KeyboardLayout::DualManual) {
            let alt_base = base_note.saturating_sub(12);
            for (index, key) in ALT_WHITE_KEYS.iter().enumerate() {
                self.note_lookup.insert(*key, alt_base + index as u8);
            }
            for (index, key) in ALT_BLACK_KEYS.iter().enumerate() {
                let note = alt_base + [1, 3, 6, 8, 10][index];
                self.note_lookup.insert(*key, note);
            }
        }
    }

    fn velocity_from_modifiers(&self, modifiers: ModifiersState) -> u8 {
        let base = match self.config.velocity_curve {
            VelocityCurveSetting::Linear => self.velocity_cycle[self.velocity_index],
            VelocityCurveSetting::Soft => (self.velocity_cycle[self.velocity_index] as f32 * 0.8)
                .round()
                .clamp(1.0, 127.0) as u8,
            VelocityCurveSetting::Hard => (self.velocity_cycle[self.velocity_index] as f32 * 1.2)
                .round()
                .clamp(1.0, 127.0) as u8,
            VelocityCurveSetting::Fixed => self.config.fixed_velocity.clamp(1, 127),
        };
        if modifiers.shift_key() {
            base.saturating_add(20).min(127)
        } else {
            base
        }
    }

    fn emit(&mut self, timestamp: Instant, event: MidiEvent) {
        if let Err((ts, evt)) = self.queue_tx.push((timestamp, event)) {
            // drop oldest event if queue full
            let _ = self.queue_rx.pop();
            let _ = self.queue_tx.push((ts, evt));
        }
    }

    fn note_on(&mut self, key: KeyCode, timestamp: Instant, modifiers: ModifiersState) {
        if self.held_keys.contains(&key) {
            return;
        }
        if let Some(&note) = self.note_lookup.get(&key) {
            self.held_keys.insert(key);
            let velocity = self.velocity_from_modifiers(modifiers);
            let event = MidiEvent::NoteOn {
                channel: self.channel,
                note,
                velocity,
                sample_offset: 0,
                timestamp: Some(midi_timestamp_from_instant(timestamp)),
            };
            self.emit(timestamp, event);
        }
    }

    fn note_off(&mut self, key: KeyCode, timestamp: Instant) {
        if !self.held_keys.remove(&key) {
            return;
        }
        if let Some(&note) = self.note_lookup.get(&key) {
            if self.sustain_latched {
                self.pending_off.push_back(PendingNoteOff {
                    channel: self.channel,
                    note,
                });
                self.sustained.insert(key);
                return;
            }
            let event = MidiEvent::NoteOff {
                channel: self.channel,
                note,
                sample_offset: 0,
                timestamp: Some(midi_timestamp_from_instant(timestamp)),
            };
            self.emit(timestamp, event);
        }
        self.sustained.remove(&key);
    }

    fn adjust_octave(&mut self, delta: i8) {
        let new_octave = (self.octave + delta).clamp(1, 7);
        if new_octave != self.octave {
            self.octave = new_octave;
            self.rebuild_mapping();
        }
    }

    fn set_velocity_preset(&mut self, index: usize) {
        if index < self.velocity_cycle.len() {
            self.velocity_index = index;
        }
    }

    fn change_channel(&mut self, delta: i8) {
        let mut value = (self.channel as i8) + delta;
        if value < 0 {
            value = 0;
        }
        if value > 15 {
            value = 15;
        }
        self.channel = value as u8;
    }

    fn handle_sustain(&mut self, pressed: bool, timestamp: Instant) {
        if pressed == self.sustain_latched {
            return;
        }
        self.sustain_latched = pressed;
        let value = if pressed { 127 } else { 0 };
        let event = MidiEvent::ControlChange {
            channel: self.channel,
            control: SUSTAIN_CC,
            value,
            sample_offset: 0,
            timestamp: Some(midi_timestamp_from_instant(timestamp)),
        };
        self.emit(timestamp, event);
        if !pressed {
            while let Some(note) = self.pending_off.pop_front() {
                let event = MidiEvent::NoteOff {
                    channel: note.channel,
                    note: note.note,
                    sample_offset: 0,
                    timestamp: Some(midi_timestamp_from_instant(timestamp)),
                };
                self.emit(timestamp, event);
            }
            self.sustained.clear();
        }
    }

    fn panic_all(&mut self, timestamp: Instant) {
        let event = MidiEvent::ControlChange {
            channel: self.channel,
            control: PANIC_CC,
            value: 0,
            sample_offset: 0,
            timestamp: Some(midi_timestamp_from_instant(timestamp)),
        };
        self.emit(timestamp, event);
        let drained_keys: Vec<_> = self.held_keys.drain().collect();
        for key in drained_keys {
            if let Some(&note) = self.note_lookup.get(&key) {
                let event = MidiEvent::NoteOff {
                    channel: self.channel,
                    note,
                    sample_offset: 0,
                    timestamp: Some(midi_timestamp_from_instant(timestamp)),
                };
                self.emit(timestamp, event);
            }
        }
        self.pending_off.clear();
        self.sustained.clear();
        self.sustain_latched = false;
    }

    fn handle_velocity_cycle(&mut self, forward: bool) {
        if forward {
            self.velocity_index = (self.velocity_index + 1) % self.velocity_cycle.len();
        } else if self.velocity_index == 0 {
            self.velocity_index = self.velocity_cycle.len() - 1;
        } else {
            self.velocity_index -= 1;
        }
    }

    fn sustain_key(&self) -> KeyCode {
        match self.config.sustain_key {
            SustainKey::Space => KeyCode::Space,
            SustainKey::CapsLock => KeyCode::CapsLock,
        }
    }
}

impl MidiInputDevice for QwertyKeyboardInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    fn push_key_event(
        &mut self,
        key: KeyCode,
        pressed: bool,
        modifiers: ModifiersState,
        time: Instant,
    ) {
        if !self.enabled {
            return;
        }
        if key == self.sustain_key() {
            self.handle_sustain(pressed, time);
            return;
        }
        match key {
            KeyCode::KeyZ => {
                if pressed {
                    self.adjust_octave(-1);
                }
                return;
            }
            KeyCode::KeyX => {
                if pressed {
                    self.adjust_octave(1);
                }
                return;
            }
            KeyCode::KeyC => {
                let require_mod = matches!(self.config.layout, KeyboardLayout::DualManual);
                if pressed && (!require_mod || is_velocity_modifier(modifiers)) {
                    self.handle_velocity_cycle(false);
                    return;
                }
                if require_mod {
                    // fallthrough to note handling when modifier not pressed.
                } else {
                    return;
                }
            }
            KeyCode::KeyV => {
                let require_mod = matches!(self.config.layout, KeyboardLayout::DualManual);
                if pressed && (!require_mod || is_velocity_modifier(modifiers)) {
                    self.handle_velocity_cycle(true);
                    return;
                }
                if require_mod {
                    // fallthrough to note handling
                } else {
                    return;
                }
            }
            KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::Digit3
            | KeyCode::Digit4
            | KeyCode::Digit5
            | KeyCode::Digit6
            | KeyCode::Digit7
            | KeyCode::Digit8
            | KeyCode::Digit9
            | KeyCode::Digit0 => {
                if pressed && is_velocity_modifier(modifiers) {
                    let index = match key {
                        KeyCode::Digit1 => 0,
                        KeyCode::Digit2 => 1,
                        KeyCode::Digit3 => 2,
                        KeyCode::Digit4 => 3,
                        KeyCode::Digit5 => 4,
                        KeyCode::Digit6 => 5,
                        KeyCode::Digit7 => 6,
                        KeyCode::Digit8 => 7,
                        KeyCode::Digit9 => 8,
                        KeyCode::Digit0 => 9,
                        _ => 0,
                    };
                    self.set_velocity_preset(index);
                    return;
                }
            }
            KeyCode::BracketLeft => {
                if pressed {
                    self.change_channel(-1);
                }
                return;
            }
            KeyCode::Slash => {
                if pressed {
                    self.change_channel(1);
                }
                return;
            }
            KeyCode::Escape => {
                if pressed {
                    self.panic_all(time);
                }
                return;
            }
            _ => {}
        }

        if pressed {
            self.note_on(key, time, modifiers);
        } else {
            self.note_off(key, time);
        }
    }

    fn drain_events<'a>(&'a mut self, out: &mut dyn FnMut(MidiEvent, Instant)) {
        while let Some((timestamp, event)) = self.queue_rx.pop() {
            out(event, timestamp);
        }
    }

    fn panic(&mut self, time: Instant) {
        self.panic_all(time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn note_on_then_off_is_enqueued_once() {
        let mut device = QwertyKeyboardInput::new(QwertyConfigFile::default());
        let now = Instant::now();
        device.push_key_event(KeyCode::KeyQ, true, ModifiersState::empty(), now);
        device.push_key_event(KeyCode::KeyQ, false, ModifiersState::empty(), now);

        let mut events = Vec::new();
        device.drain_events(&mut |event, _| events.push(event));

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MidiEvent::NoteOn { note: 48, .. }));
        assert!(matches!(events[1], MidiEvent::NoteOff { note: 48, .. }));
    }

    #[test]
    fn panic_clears_active_notes() {
        let mut device = QwertyKeyboardInput::new(QwertyConfigFile::default());
        let now = Instant::now();
        device.push_key_event(KeyCode::KeyQ, true, ModifiersState::empty(), now);
        device.panic(now);

        let mut events = Vec::new();
        device.drain_events(&mut |event, _| events.push(event));

        assert!(events.iter().any(|event| matches!(
            event,
            MidiEvent::ControlChange {
                control: PANIC_CC,
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event, MidiEvent::NoteOff { note: 48, .. })));
    }
}
