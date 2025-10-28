#[derive(Clone, Debug)]
pub enum RackCmd {
    AddPattern,
    SelectPattern(u32),
    AddInstrument {
        name: String,
        plugin_uid: String,
    },
    AddSample {
        name: String,
        path: std::path::PathBuf,
    },
    RemoveChannel(u32),
    CloneChannel(u32),
    ToggleMute(u32, bool),
    ToggleSolo(u32, bool),
    OpenPianoRoll {
        channel_id: u32,
        pattern_id: u32,
    },
    ConvertStepsToMidi {
        channel_id: u32,
        pattern_id: u32,
    },
}
