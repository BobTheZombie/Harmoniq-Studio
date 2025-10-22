use std::time::Duration;

use eframe::{egui, App, CreationContext, NativeOptions};
use harmoniq_app::MixerPanel;
use harmoniq_engine::mixer::{self, MixerEngine};

const INITIAL_CHANNELS: usize = 16;
const SNAPSHOT_CAPACITY: usize = 64;

struct HarmoniqDesktopApp {
    mixer: MixerPanel,
    _engine: MixerEngine,
}

impl HarmoniqDesktopApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.set_pixels_per_point(1.1);

        let (bus, endpoint) = mixer::rt_api::create_mixer_bus(SNAPSHOT_CAPACITY);
        let engine = MixerEngine::new(endpoint, INITIAL_CHANNELS);
        let mixer = MixerPanel::new(INITIAL_CHANNELS, bus);

        HarmoniqDesktopApp {
            mixer,
            _engine: engine,
        }
    }
}

impl App for HarmoniqDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));
        self.mixer.ui(ctx);
    }
}

fn native_options() -> NativeOptions {
    NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Harmoniq Studio")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([960.0, 540.0]),
        ..Default::default()
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Harmoniq Studio",
        native_options(),
        Box::new(|cc| Box::new(HarmoniqDesktopApp::new(cc))),
    )
}
