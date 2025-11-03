use egui::{pos2, vec2, Painter, Rect, Response, Sense, Ui};

use crate::model::{Edit, EditorState};
use crate::theme::Theme;
use crate::tools;

pub struct TransportResult {
    pub response: Response,
    pub edits: Vec<Edit>,
}

pub fn ruler_ui(
    ui: &mut Ui,
    state: &mut EditorState,
    theme: &Theme,
    width: f32,
) -> TransportResult {
    let height = 28.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    paint_ruler(&painter, rect, state, theme);

    let mut edits = Vec::new();
    if let Some(pos) = response.interact_pointer_pos() {
        let ppq = screen_to_ppq(rect, state, pos.x);
        if response.clicked() {
            state.playhead_ppq = ppq;
        } else if response.dragged() {
            if (pos.x - loop_handle_x(rect, state, state.clip.loop_start_ppq)).abs() < 8.0 {
                let snapped = state.clip.loop_start_ppq.max(0).min(ppq);
                state.clip.loop_len_ppq =
                    (state.clip.loop_start_ppq + state.clip.loop_len_ppq - snapped).max(1);
                state.clip.loop_start_ppq = snapped;
                edits.push(Edit::LoopChanged {
                    start_ppq: state.clip.loop_start_ppq,
                    len_ppq: state.clip.loop_len_ppq,
                });
            } else if (pos.x
                - loop_handle_x(
                    rect,
                    state,
                    state.clip.loop_start_ppq + state.clip.loop_len_ppq,
                ))
            .abs()
                < 8.0
            {
                let new_end = ppq.max(state.clip.loop_start_ppq + 1);
                state.clip.loop_len_ppq = new_end - state.clip.loop_start_ppq;
                edits.push(Edit::LoopChanged {
                    start_ppq: state.clip.loop_start_ppq,
                    len_ppq: state.clip.loop_len_ppq,
                });
            } else {
                state.playhead_ppq = ppq;
            }
        }
    }

    TransportResult { response, edits }
}

fn paint_ruler(painter: &Painter, rect: Rect, state: &EditorState, theme: &Theme) {
    painter.rect_filled(rect, 0.0, theme.ruler_background);
    let beats_per_bar = state.beats_per_bar();
    let ppq = state.ppq();

    let start_beats = ((state.scroll_px.x.max(0.0)) / state.zoom_x)
        .floor()
        .max(0.0);
    let end_beats = start_beats + rect.width() / state.zoom_x + 4.0;
    let start_bar = (start_beats / beats_per_bar as f32).floor() as i32;
    let end_bar = (end_beats / beats_per_bar as f32).ceil() as i32;

    for bar in start_bar..=end_bar {
        let bar_ppq = bar as i64 * beats_per_bar as i64 * ppq as i64;
        let x = loop_handle_x(rect, state, bar_ppq);
        if x < rect.left() || x > rect.right() {
            continue;
        }
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            theme.grid_bar,
        );
        painter.text(
            pos2(x + 4.0, rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            format!("{}", bar + 1),
            egui::FontId::proportional(12.0),
            theme.ruler_foreground,
        );
        for beat in 1..beats_per_bar {
            let beat_ppq = bar_ppq + beat as i64 * ppq as i64;
            let bx = loop_handle_x(rect, state, beat_ppq);
            if bx < rect.left() || bx > rect.right() {
                continue;
            }
            painter.line_segment(
                [pos2(bx, rect.top() + 10.0), pos2(bx, rect.bottom())],
                theme.grid_beat,
            );
        }
    }

    let playhead_x = loop_handle_x(rect, state, state.playhead_ppq);
    painter.line_segment(
        [
            pos2(playhead_x, rect.top()),
            pos2(playhead_x, rect.bottom()),
        ],
        theme.playhead,
    );

    let loop_start_x = loop_handle_x(rect, state, state.clip.loop_start_ppq);
    let loop_end_x = loop_handle_x(
        rect,
        state,
        state.clip.loop_start_ppq + state.clip.loop_len_ppq,
    );
    let loop_rect = Rect::from_x_y_ranges(loop_start_x..=loop_end_x, rect.top()..=rect.bottom());
    painter.rect_filled(loop_rect.shrink(1.0), 0.0, theme.loop_range);
}

fn loop_handle_x(rect: Rect, state: &EditorState, ppq: i64) -> f32 {
    let beats = ppq as f32 / state.ppq() as f32;
    rect.left() + beats * state.zoom_x - state.scroll_px.x
}

fn screen_to_ppq(rect: Rect, state: &EditorState, x: f32) -> i64 {
    let local = x - rect.left();
    tools::pointer_to_ppq(&state.clip, state.zoom_x, state.scroll_px.x, local)
}
