use std::time::Instant;

use eframe::egui::{self, Context, Key, PointerButton, Rect};

use crate::ui::workspace::WorkspacePane;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    Pane(WorkspacePane),
    Transport,
    None,
}

impl FocusTarget {
    pub fn matches_pane(&self, pane: &WorkspacePane) -> bool {
        matches!(self, FocusTarget::Pane(active) if active == pane)
    }
}

#[derive(Debug)]
pub struct InputFocus {
    active: FocusTarget,
    last_change: Instant,
}

impl Default for InputFocus {
    fn default() -> Self {
        Self {
            active: FocusTarget::None,
            last_change: Instant::now(),
        }
    }
}

impl InputFocus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_target(&self) -> FocusTarget {
        self.active
    }

    pub fn set_active(&mut self, target: FocusTarget) {
        if self.active != target {
            self.active = target;
            self.last_change = Instant::now();
        }
    }

    pub fn clear(&mut self) {
        self.set_active(FocusTarget::None);
    }

    pub fn has_focus(&self, target: FocusTarget) -> bool {
        self.active == target
    }

    pub fn has_pane_focus(&self, pane: &WorkspacePane) -> bool {
        self.has_focus(FocusTarget::Pane(*pane))
    }

    pub fn track_pane_interaction(&mut self, ctx: &Context, rect: Rect, pane: WorkspacePane) {
        let pointer_pressed_inside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .map(|pos| rect.contains(pos))
                    .unwrap_or(false)
        });
        if pointer_pressed_inside {
            self.set_active(FocusTarget::Pane(pane));
        } else if ctx.input(|input| input.pointer.any_pressed()) {
            let pointer_pos = ctx.input(|input| input.pointer.interact_pos());
            if pointer_pos
                .flatten()
                .map(|pos| !rect.contains(pos))
                .unwrap_or(false)
                && self.active == FocusTarget::Pane(pane)
            {
                self.clear();
            }
        }
    }

    pub fn promote_transport(&mut self) {
        self.set_active(FocusTarget::Transport);
    }

    pub fn maybe_release_on_escape(&mut self, ctx: &Context) {
        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            self.clear();
        }
    }

    pub fn last_change(&self) -> Instant {
        self.last_change
    }

    pub fn wants_pointer_text_input(&self, ctx: &Context) -> bool {
        ctx.input(|input| input.pointer.button_down(PointerButton::Primary))
    }
}
