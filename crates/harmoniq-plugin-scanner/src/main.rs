use clap::Parser;
use harmoniq_plugin_db::PluginStore;
use harmoniq_plugin_scanner::{ScanOptions, Scanner};

#[derive(Parser, Debug)]
#[command(name = "harmoniq-plugin-scanner")]
struct Args {
    #[arg(long)]
    no_verify: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let path = PluginStore::default_path()?;
    let store = PluginStore::open(path)?;
    let scanner = Scanner::new(store);
    let mut options = ScanOptions::default();
    options.verify = !args.no_verify;
    let results = scanner.scan(&options)?;
    for plugin in results {
        println!("{} ({:?})", plugin.name, plugin.id.format);
    }
    Ok(())
}
