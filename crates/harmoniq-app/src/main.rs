mod app;
mod commands;
mod engine_bridge;
mod state;
mod ui;

use eframe::{egui, NativeOptions};

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Harmoniq Studio",
        options,
        Box::new(|cc| Box::new(app::HarmoniqApp::new(cc))),
    )
}
