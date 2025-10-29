use crate::{PluginEntry, PluginFormat, PluginMetadata};

struct StockInstrument {
    id: &'static str,
    name: &'static str,
    vendor: &'static str,
    category: Option<&'static str>,
    description: Option<&'static str>,
}

const STOCK_INSTRUMENTS: &[StockInstrument] = &[
    StockInstrument {
        id: "harmoniq.sine",
        name: "Sine Synth",
        vendor: "Harmoniq Labs",
        category: Some("Synth"),
        description: Some("Basic anti-aliased sine oscillator for testing"),
    },
    StockInstrument {
        id: "harmoniq.analog",
        name: "Analog Synth",
        vendor: "Harmoniq Labs",
        category: Some("Synth"),
        description: Some(
            "Basic subtractive synthesizer with ADSR envelope and single pole filter",
        ),
    },
    StockInstrument {
        id: "harmoniq.fm",
        name: "FM Synth",
        vendor: "Harmoniq Labs",
        category: Some("Synth"),
        description: Some("Two-operator FM synthesizer"),
    },
    StockInstrument {
        id: "harmoniq.wavetable",
        name: "Wavetable Synth",
        vendor: "Harmoniq Labs",
        category: Some("Synth"),
        description: Some("Table driven synthesizer with morphing"),
    },
    StockInstrument {
        id: "harmoniq.sampler",
        name: "Sampler",
        vendor: "Harmoniq Labs",
        category: Some("Instrument"),
        description: Some("Single shot sample player with WAV/MP3/FLAC support"),
    },
    StockInstrument {
        id: "harmoniq.granular",
        name: "Granular Synth",
        vendor: "Harmoniq Labs",
        category: Some("Texture"),
        description: Some("Randomized grain based texture generator"),
    },
    StockInstrument {
        id: "harmoniq.grand_piano_clap",
        name: "Grand Piano Clap",
        vendor: "Harmoniq Labs",
        category: Some("Keys"),
        description: Some("Layered grand piano with expressive hand clap accompaniment"),
    },
    StockInstrument {
        id: "harmoniq.additive",
        name: "Additive Synth",
        vendor: "Harmoniq Labs",
        category: Some("Synth"),
        description: Some("8 partial harmonic resynthesis engine"),
    },
    StockInstrument {
        id: "harmoniq.organ_piano",
        name: "Organ/Piano",
        vendor: "Harmoniq Labs",
        category: Some("Keys"),
        description: Some("Hybrid organ and piano engine"),
    },
    StockInstrument {
        id: "harmoniq.bass",
        name: "Mini Moog Bass",
        vendor: "Harmoniq Labs",
        category: Some("Bass"),
        description: Some("Fat analog-inspired bass synthesizer"),
    },
    StockInstrument {
        id: "harmoniq.westcoast",
        name: "West Coast Lead",
        vendor: "Harmoniq Labs",
        category: Some("Lead"),
        description: Some("Wavefolder driven lead voice with Buchla-inspired modulation"),
    },
    StockInstrument {
        id: "harmoniq.sub808",
        name: "808 Sub Bass",
        vendor: "Harmoniq Labs",
        category: Some("Bass"),
        description: Some("Classic 808 style sub bass generator"),
    },
];

pub fn stock_instruments() -> Vec<PluginEntry> {
    STOCK_INSTRUMENTS
        .iter()
        .map(|instrument| {
            PluginMetadata {
                id: instrument.id.into(),
                name: instrument.name.into(),
                vendor: Some(instrument.vendor.into()),
                category: instrument.category.map(|value| value.into()),
                version: None,
                description: instrument.description.map(|value| value.into()),
                is_instrument: true,
                has_editor: false,
                num_inputs: 0,
                num_outputs: 2,
            }
            .into_entry(
                PluginFormat::Harmoniq,
                format!("builtin://{}", instrument.id),
            )
        })
        .collect()
}
