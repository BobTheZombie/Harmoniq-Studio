use std::collections::HashMap;

use eframe::egui::{self, Event, Key, Modifiers};
use harmoniq_engine::MidiEvent;

const LOWER_BASE_NOTE: u8 = 48; // C3
const UPPER_BASE_NOTE: u8 = 60; // C4
const DEFAULT_VELOCITY: u8 = 100;
const ACCENT_VELOCITY: u8 = 127;
const MIDI_CHANNEL: u8 = 0;

#[derive(Default)]
pub struct TypingKeyboard {
    active_notes: HashMap<Key, u8>,
}

impl TypingKeyboard {
    pub fn collect_midi_events(&mut self, ctx: &egui::Context) -> Vec<MidiEvent> {
        if ctx.wants_keyboard_input() {
            return self.release_all();
        }

        ctx.input(|input| {
            if !input.focused {
                return self.release_all();
            }

            let mut events = Vec::new();
            for event in &input.events {
                if let Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } = event
                {
                    if *repeat {
                        continue;
                    }

                    if let Some(note) = self.map_key_to_note(*key, modifiers) {
                        if *pressed {
                            if self.active_notes.insert(*key, note).is_none() {
                                let velocity = Self::velocity_from_modifiers(modifiers);
                                events.push(MidiEvent::NoteOn {
                                    channel: MIDI_CHANNEL,
                                    note,
                                    velocity,
                                });
                            }
                        } else if let Some(note) = self.active_notes.remove(key) {
                            events.push(MidiEvent::NoteOff {
                                channel: MIDI_CHANNEL,
                                note,
                            });
                        }
                    } else if !pressed {
                        self.active_notes.remove(key);
                    }
                }
            }
            events
        })
    }

    fn map_key_to_note(&self, key: Key, modifiers: &Modifiers) -> Option<u8> {
        if modifiers.command || modifiers.ctrl || modifiers.mac_cmd || modifiers.alt {
            return None;
        }

        Some(match key {
            Key::Z => Self::lower_note(0),
            Key::S => Self::lower_note(1),
            Key::X => Self::lower_note(2),
            Key::D => Self::lower_note(3),
            Key::C => Self::lower_note(4),
            Key::V => Self::lower_note(5),
            Key::G => Self::lower_note(6),
            Key::B => Self::lower_note(7),
            Key::H => Self::lower_note(8),
            Key::N => Self::lower_note(9),
            Key::J => Self::lower_note(10),
            Key::M => Self::lower_note(11),
            Key::Q => Self::upper_note(0),
            Key::Num2 => Self::upper_note(1),
            Key::W => Self::upper_note(2),
            Key::Num3 => Self::upper_note(3),
            Key::E => Self::upper_note(4),
            Key::R => Self::upper_note(5),
            Key::Num5 => Self::upper_note(6),
            Key::T => Self::upper_note(7),
            Key::Num6 => Self::upper_note(8),
            Key::Y => Self::upper_note(9),
            Key::Num7 => Self::upper_note(10),
            Key::U => Self::upper_note(11),
            Key::I => Self::upper_note(12),
            Key::Num9 => Self::upper_note(13),
            Key::O => Self::upper_note(14),
            Key::Num0 => Self::upper_note(15),
            Key::P => Self::upper_note(16),
            _ => return None,
        })
    }

    fn velocity_from_modifiers(modifiers: &Modifiers) -> u8 {
        if modifiers.shift {
            ACCENT_VELOCITY
        } else {
            DEFAULT_VELOCITY
        }
    }

    fn lower_note(offset: u8) -> u8 {
        LOWER_BASE_NOTE.saturating_add(offset)
    }

    fn upper_note(offset: u8) -> u8 {
        UPPER_BASE_NOTE.saturating_add(offset)
    }

    fn release_all(&mut self) -> Vec<MidiEvent> {
        if self.active_notes.is_empty() {
            Vec::new()
        } else {
            self.active_notes
                .drain()
                .map(|(_, note)| MidiEvent::NoteOff {
                    channel: MIDI_CHANNEL,
                    note,
                })
                .collect()
        }
    }
}
