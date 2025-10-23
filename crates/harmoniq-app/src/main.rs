//! Harmoniq Studio desktop application entry point.

use harmoniq_engine::Engine;
use harmoniq_ui::HarmoniqUiApp;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let (mut engine, handle) = Engine::new(None).expect("engine");
    engine.start().expect("engine start");

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Harmoniq Studio",
        native_options,
        Box::new(move |_cc| Box::new(HarmoniqUiApp::new(handle, engine))),
    )
}
