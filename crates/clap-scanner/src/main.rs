mod probe;
mod scan;

use anyhow::Result;
use clap::Parser;
use scan::scan_directories;

#[derive(Parser, Debug)]
#[command(name = "clap-scanner")]
struct Args {
    /// Additional directories to scan
    #[arg(short, long)]
    paths: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let directories = scan_directories(args.paths)?;
    println!("{{}}", serde_json::to_string_pretty(&directories)?);
    Ok(())
}
