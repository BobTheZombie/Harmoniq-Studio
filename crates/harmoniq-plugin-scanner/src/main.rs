use std::path::PathBuf;

use clap::Parser;
use harmoniq_plugin_db::{PluginFormat, PluginStore};
use harmoniq_plugin_scanner::{ScanOptions, Scanner};

#[derive(Parser, Debug)]
#[command(name = "harmoniq-plugin-scanner")]
struct Args {
    /// Restrict scanning to the given plugin formats
    #[arg(
        long,
        value_name = "FORMAT",
        value_parser = parse_format,
        default_values_t = default_formats(),
    )]
    formats: Vec<PluginFormat>,

    /// Additional paths to scan for plugins
    #[arg(long = "path", value_name = "PATH")]
    extra_paths: Vec<PathBuf>,
}

fn parse_format(value: &str) -> Result<PluginFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "clap" => Ok(PluginFormat::Clap),
        "vst3" => Ok(PluginFormat::Vst3),
        "ovst3" | "openvst3" => Ok(PluginFormat::Ovst3),
        "harmoniq" | "hq" | "hqplug" => Ok(PluginFormat::Harmoniq),
        other => Err(format!("unsupported format: {other}")),
    }
}

fn default_formats() -> Vec<PluginFormat> {
    vec![
        PluginFormat::Clap,
        PluginFormat::Vst3,
        PluginFormat::Ovst3,
        PluginFormat::Harmoniq,
    ]
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let path = PluginStore::default_path()?;
    let store = PluginStore::open(path)?;
    let scanner = Scanner::new(store);
    let mut options = ScanOptions::default();
    options.formats = args.formats;
    options.extra_paths = args.extra_paths;
    let results = scanner.scan(&options)?;
    for plugin in results {
        println!("{} ({:?})", plugin.name, plugin.reference.format);
    }
    Ok(())
}
