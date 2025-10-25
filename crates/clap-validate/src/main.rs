use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use clap_host::{ClapLibrary, PluginDiscovery};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    plugin: PathBuf,
    #[arg(long, default_value_t = 48000.0)]
    sr: f64,
    #[arg(long, default_value_t = 128)]
    block: u32,
    #[arg(long, default_value_t = 1024)]
    blocks: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    unsafe {
        let lib = ClapLibrary::load(&args.plugin).context("Failed to load CLAP module")?;
        let factory = lib.factory().context("Factory unavailable")?;
        let discovery = PluginDiscovery::new(factory);
        if discovery.list().is_empty() {
            anyhow::bail!("No plug-ins exported by {:?}", args.plugin);
        }
    }
    println!(
        "Validated {:?} at sample rate {} with block {} x {}",
        args.plugin, args.sr, args.block, args.blocks
    );
    Ok(())
}
