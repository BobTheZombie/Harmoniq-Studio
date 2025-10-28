/// Entry describing a MIDI learn mapping.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MidiLearnMapEntry {
    /// Raw three-byte MIDI message captured during learn.
    pub msg: [u8; 3],
    /// Target parameter (node id, parameter id).
    pub target_param: (u64, u32),
}

/// Collection of MIDI learn bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MidiLearnMap {
    /// Stored mapping entries.
    pub entries: Vec<MidiLearnMapEntry>,
}

impl MidiLearnMap {
    /// Resolve a mapping for the provided MIDI message.
    pub fn resolve(&self, msg: &[u8; 3]) -> Option<&MidiLearnMapEntry> {
        self.entries.iter().find(|entry| &entry.msg == msg)
    }

    /// Add or replace an entry in the map.
    pub fn upsert(&mut self, entry: MidiLearnMapEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|candidate| candidate.msg == entry.msg)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_existing_mapping() {
        let mut map = MidiLearnMap::default();
        map.upsert(MidiLearnMapEntry {
            msg: [0x90, 60, 100],
            target_param: (1, 2),
        });
        assert!(map.resolve(&[0x90, 60, 100]).is_some());
        assert!(map.resolve(&[0x90, 61, 100]).is_none());
    }
}
