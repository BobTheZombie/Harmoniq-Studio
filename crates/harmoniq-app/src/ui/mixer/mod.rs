mod debug;
mod layout;
mod meter;
mod render;
mod strip;
mod theme;
mod widgets;

#[cfg(test)]
mod tests;

use eframe::egui::{Key, Ui};
use harmoniq_engine::mixer::api::MixerUiApi;
use harmoniq_ui::HarmoniqPalette;
use std::sync::Arc;

pub use render::MixerUiState;
pub use theme::MixerTheme;

pub struct MixerView {
    api: Arc<dyn MixerUiApi>,
    state: MixerUiState,
    theme: MixerTheme,
}

impl MixerView {
    pub fn new(api: Arc<dyn MixerUiApi>) -> Self {
        Self {
            api,
            state: MixerUiState::default(),
            theme: MixerTheme::default(),
        }
    }

    pub fn toggle_density(&mut self) {
        self.state.narrow = !self.state.narrow;
    }

    pub fn zoom_in(&mut self) {
        self.state.zoom = layout::clamp_zoom(self.state.zoom + 0.05);
    }

    pub fn zoom_out(&mut self) {
        self.state.zoom = layout::clamp_zoom(self.state.zoom - 0.05);
    }

    pub fn ui(&mut self, ui: &mut Ui, palette: &HarmoniqPalette) {
        self.theme = MixerTheme::from_palette(palette);
        self.handle_shortcuts(ui);

        let total = self.api.strips_len();
        if total == 0 {
            return;
        }

        theme::with_active_theme(&self.theme, || {
            render::mixer(ui, &mut self.state, self.api.as_ref());
        });
    }

    pub fn cpu_estimate(&self) -> f32 {
        let total = self.api.strips_len();
        if total == 0 {
            return 0.0;
        }
        let master = self.api.strip_info(total - 1);
        master.cpu_percent
    }

    pub fn master_meter(&self) -> (f32, f32) {
        let total = self.api.strips_len();
        if total == 0 {
            return (f32::NEG_INFINITY, f32::NEG_INFINITY);
        }
        let levels = self.api.level_fetch(total - 1);
        (levels.0, levels.1)
    }

    fn handle_shortcuts(&mut self, ui: &Ui) {
        let ctx = ui.ctx();
        if ctx.input(|i| i.key_pressed(Key::N)) {
            self.state.narrow = true;
        }
        if ctx.input(|i| i.key_pressed(Key::W)) {
            self.state.narrow = false;
        }
        if ctx.input(|i| {
            i.modifiers.command && (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals))
        }) {
            self.zoom_in();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Minus)) {
            self.zoom_out();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::D)) {
            self.state.debug = !self.state.debug;
        }
    }
}
