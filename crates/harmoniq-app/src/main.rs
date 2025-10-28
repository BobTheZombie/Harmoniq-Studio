#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod engine_bridge;
mod state;
mod ui;

use app::HarmoniqApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Harmoniq Studio",
        native_options,
        Box::new(|cc| Box::new(HarmoniqApp::new(cc))),
    )
}
