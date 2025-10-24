use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use harmoniq_engine::render::{
    DitherKind, FreezeSettings, RenderDuration, RenderFile, RenderFormat, RenderProject,
    RenderQueue, RenderRequest, RenderSpeed, StemSettings,
};
use harmoniq_engine::{
    nodes::{NodeNoise, NodeOsc},
    AudioProcessor, BufferConfig, ChannelLayout, GraphBuilder, HarmoniqEngine,
};
use serde::Deserialize;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Render(args) => execute_render(args),
    }
}

#[derive(Parser)]
#[command(author, version, about = "Offline rendering tools for Harmoniq Studio")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Render a project to disk using the offline renderer.
    Render(RenderArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// Path to the project description (JSON).
    #[arg(long)]
    project: PathBuf,
    /// Output path for the mixdown file.
    #[arg(long)]
    mixdown: PathBuf,
    /// Optional directory for exporting stems.
    #[arg(long)]
    stems_dir: Option<PathBuf>,
    /// Optional directory for project freeze assets.
    #[arg(long)]
    freeze_dir: Option<PathBuf>,
    /// Override render duration in seconds.
    #[arg(long)]
    duration: Option<f32>,
    /// Output format for produced audio files.
    #[arg(long, value_enum, default_value_t = OutputFormat::Wav)]
    format: OutputFormat,
    /// Enable TPDF dithering when exporting integer formats.
    #[arg(long)]
    dither: bool,
}

fn execute_render(args: RenderArgs) -> Result<()> {
    let project_data = fs::read_to_string(&args.project)
        .with_context(|| format!("failed to read project file {}", args.project.display()))?;
    let spec: ProjectSpec = serde_json::from_str(&project_data)
        .with_context(|| format!("{} is not a valid project file", args.project.display()))?;

    let duration = args
        .duration
        .map(RenderDuration::Seconds)
        .unwrap_or_else(|| RenderDuration::Seconds(spec.duration_seconds));

    let format = RenderFormat::from(args.format);
    let dither = if args.dither {
        Some(DitherKind::Tpdf)
    } else {
        None
    };

    let mixdown = RenderFile {
        path: args.mixdown.clone(),
        format,
        dither,
    };

    let stems = args.stems_dir.as_ref().map(|dir| StemSettings {
        directory: dir.clone(),
        format,
        dither,
        plugins: None,
    });

    let freeze = args.freeze_dir.as_ref().map(|dir| FreezeSettings {
        directory: dir.clone(),
        format,
        dither,
        plugins: None,
    });

    let request = RenderRequest {
        duration,
        mixdown: Some(mixdown),
        stems,
        freeze,
        speed: RenderSpeed::Offline,
    };

    let project = Arc::new(spec);
    let mut queue = RenderQueue::new();
    queue.enqueue_project(project, request);

    let reports = queue.process_all()?;
    for report in reports {
        println!(
            "Rendered project '{}' ({} frames)",
            report.project, report.duration_frames
        );
        if let Some(path) = report.mixdown {
            println!("  Mixdown: {}", path.display());
        }
        if !report.stems.is_empty() {
            println!("  Stems:");
            for stem in report.stems {
                println!("    {}", stem.display());
            }
        }
        if !report.freezes.is_empty() {
            println!("  Freezes:");
            for freeze_path in report.freezes {
                println!("    {}", freeze_path.display());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Wav,
    Flac,
}

impl From<OutputFormat> for RenderFormat {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Wav => RenderFormat::Wav,
            OutputFormat::Flac => RenderFormat::Flac,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProjectSpec {
    name: String,
    sample_rate: f32,
    block_size: usize,
    channels: usize,
    duration_seconds: f32,
    #[serde(default)]
    plugins: Vec<PluginSpec>,
}

impl RenderProject for ProjectSpec {
    fn label(&self) -> &str {
        &self.name
    }

    fn create_engine(&self) -> Result<HarmoniqEngine> {
        let layout = match self.channels {
            1 => ChannelLayout::Mono,
            2 => ChannelLayout::Stereo,
            6 => ChannelLayout::Surround51,
            other => ChannelLayout::Custom(other as u8),
        };
        let config = BufferConfig::new(self.sample_rate, self.block_size, layout);
        let mut engine = HarmoniqEngine::new(config.clone())?;

        if self.plugins.is_empty() {
            return Ok(engine);
        }

        let mut builder = GraphBuilder::new();
        for plugin in &self.plugins {
            let processor = plugin.instantiate()?;
            let id = engine.register_processor(processor)?;
            let node = builder.add_node(id);
            builder.connect_to_mixer(node, plugin.gain)?;
        }

        engine.replace_graph(builder.build())?;
        engine.reset_render_state()?;
        Ok(engine)
    }
}

#[derive(Debug, Deserialize)]
struct PluginSpec {
    #[serde(flatten)]
    kind: PluginKind,
    #[serde(default = "default_gain")]
    gain: f32,
}

impl PluginSpec {
    fn instantiate(&self) -> Result<Box<dyn AudioProcessor>> {
        let processor: Box<dyn AudioProcessor> = match &self.kind {
            PluginKind::Sine {
                frequency,
                amplitude,
            } => Box::new(NodeOsc::new(*frequency).with_amplitude(*amplitude)),
            PluginKind::Noise { amplitude } => Box::new(NodeNoise::new(*amplitude)),
        };
        Ok(processor)
    }
}

fn default_gain() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PluginKind {
    #[serde(rename = "sine")]
    Sine {
        #[serde(default = "default_frequency")]
        frequency: f32,
        #[serde(default = "default_amplitude")]
        amplitude: f32,
    },
    #[serde(rename = "noise")]
    Noise {
        #[serde(default = "default_amplitude")]
        amplitude: f32,
    },
}

fn default_frequency() -> f32 {
    440.0
}

fn default_amplitude() -> f32 {
    0.5
}
