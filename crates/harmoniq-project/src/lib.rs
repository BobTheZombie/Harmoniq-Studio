//! Project persistence and domain model for Harmoniq Studio.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use harmoniq_graph::{Graph, NodeId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version tag stored alongside serialized projects.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectVersion(pub u32);

impl Default for ProjectVersion {
    fn default() -> Self {
        ProjectVersion(1)
    }
}

/// Top-level project container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Version of the serialized format.
    pub version: ProjectVersion,
    /// Unique identifier of the project.
    pub id: Uuid,
    /// Timestamp when the project was created.
    pub created: DateTime<Utc>,
    /// Display name.
    pub name: String,
    /// Routing graph associated with the project.
    pub graph: Graph,
    /// Track definitions.
    pub tracks: Vec<Track>,
    /// Patterns stored in the project.
    pub patterns: Vec<Pattern>,
    /// Global tempo map.
    pub tempo: TempoMap,
}

impl Project {
    /// Creates an empty project instance.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: ProjectVersion::default(),
            id: Uuid::new_v4(),
            created: Utc::now(),
            name: name.into(),
            graph: Graph::new(),
            tracks: Vec::new(),
            patterns: Vec::new(),
            tempo: TempoMap::default(),
        }
    }

    /// Serializes the project into a file on disk.
    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let mut file =
            File::create(path).with_context(|| format!("unable to create project at {path:?}"))?;
        let json = serde_json::to_vec_pretty(self)?;
        file.write_all(&json)?;
        Ok(())
    }

    /// Loads a project from disk.
    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        let mut file =
            File::open(path).with_context(|| format!("unable to open project at {path:?}"))?;
        let mut json = Vec::new();
        file.read_to_end(&mut json)?;
        let mut project: Project = serde_json::from_slice(&json)?;
        project.upgrade();
        Ok(project)
    }

    /// Upgrades in-memory structures to the latest format.
    pub fn upgrade(&mut self) {
        if self.version.0 < ProjectVersion::default().0 {
            // Insert migrations here in the future.
            self.version = ProjectVersion::default();
        }
    }
}

/// Information describing a mixer track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique track identifier.
    pub id: Uuid,
    /// Name shown in the UI.
    pub name: String,
    /// Index into the routing graph.
    pub node: Option<NodeId>,
    /// Clips arranged on this track.
    pub clips: Vec<Clip>,
    /// Automation lanes belonging to the track.
    pub automation: Vec<AutomationLane>,
}

impl Track {
    /// Creates a new track with a generated identifier.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            node: None,
            clips: Vec::new(),
            automation: Vec::new(),
        }
    }
}

/// Arrangement clip representing either audio or MIDI data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    /// Human friendly name.
    pub name: String,
    /// Start position in samples.
    pub start: u64,
    /// Length in samples.
    pub length: u64,
    /// Underlying pattern reference, if any.
    pub pattern: Option<Uuid>,
}

/// Pattern used by the step sequencer and piano roll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Identifier for the pattern.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// MIDI events stored in the pattern.
    pub midi: Vec<PatternEvent>,
}

impl Pattern {
    /// Creates a new empty pattern.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            midi: Vec::new(),
        }
    }
}

/// Single pattern event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEvent {
    /// MIDI note number.
    pub note: u8,
    /// Velocity from 0-127.
    pub velocity: u8,
    /// Start position in beats.
    pub start_beats: f32,
    /// Duration in beats.
    pub duration_beats: f32,
}

/// Automation lane associated with a track or plugin parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLane {
    /// Unique identifier of the automation lane.
    pub id: Uuid,
    /// Target parameter identifier.
    pub target: String,
    /// Envelope points ordered by time.
    pub points: Vec<AutomationPoint>,
}

/// Automation point representing a value at a specific time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationPoint {
    /// Position in samples.
    pub position: u64,
    /// Target value.
    pub value: f32,
}

/// Tempo map describing tempo changes across the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMap {
    /// Ordered events.
    pub events: Vec<TempoEvent>,
}

impl Default for TempoMap {
    fn default() -> Self {
        Self {
            events: vec![TempoEvent {
                position: 0,
                bpm: 128.0,
                time_signature_numerator: 4,
                time_signature_denominator: 4,
            }],
        }
    }
}

/// Single tempo change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoEvent {
    /// Position in samples.
    pub position: u64,
    /// Tempo in beats per minute.
    pub bpm: f32,
    /// Time signature numerator.
    pub time_signature_numerator: u32,
    /// Time signature denominator.
    pub time_signature_denominator: u32,
}
