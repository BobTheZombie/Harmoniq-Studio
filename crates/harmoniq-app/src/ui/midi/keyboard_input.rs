use std::collections::HashMap;

use winit::keyboard::KeyCode;

/// Maps QWERTY keys to MIDI note numbers.
#[derive(Default)]
pub struct KeyboardMapper {
    mapping: HashMap<KeyCode, u8>,
}

impl KeyboardMapper {
    /// Create a mapping using the default layout.
    pub fn new() -> Self {
        let mut mapping = HashMap::new();
        mapping.insert(KeyCode::KeyZ, 60);
        mapping.insert(KeyCode::KeyS, 61);
        mapping.insert(KeyCode::KeyX, 62);
        mapping.insert(KeyCode::KeyD, 63);
        mapping.insert(KeyCode::KeyC, 64);
        mapping.insert(KeyCode::KeyV, 65);
        mapping.insert(KeyCode::KeyG, 66);
        mapping.insert(KeyCode::KeyB, 67);
        mapping.insert(KeyCode::KeyH, 68);
        mapping.insert(KeyCode::KeyN, 69);
        mapping.insert(KeyCode::KeyJ, 70);
        mapping.insert(KeyCode::KeyM, 71);
        mapping.insert(KeyCode::Comma, 72);
        Self { mapping }
    }

    /// Resolve a note number for the given key code.
    pub fn note_for_key(&self, key: KeyCode) -> Option<u8> {
        self.mapping.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwerty_mapping_returns_expected_note() {
        let mapper = KeyboardMapper::new();
        assert_eq!(mapper.note_for_key(KeyCode::KeyZ), Some(60));
        assert_eq!(mapper.note_for_key(KeyCode::KeyS), Some(61));
        assert_eq!(mapper.note_for_key(KeyCode::Slash), None);
    }
}
