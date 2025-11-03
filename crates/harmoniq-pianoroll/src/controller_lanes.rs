use egui::{pos2, vec2, Painter, Pos2, Rect, Response, Sense, Ui, Vec2};

use crate::model::{ControllerPoint, Edit, EditorState, Lane, LaneKind};
use crate::theme::Theme;
use crate::tools;

pub struct LanesResult {
    pub response: Response,
    pub edits: Vec<Edit>,
}

pub fn lanes_ui(ui: &mut Ui, state: &mut EditorState, theme: &Theme, width: f32) -> LanesResult {
    let mut edits = Vec::new();
    let mut total_response = ui.allocate_response(vec2(width, 0.0), Sense::hover());
    let mut interactions: Vec<(usize, Rect, Pos2)> = Vec::new();
    for index in 0..state.lanes.len() {
        if !state.lanes[index].visible {
            continue;
        }
        let lane_height = state.lanes[index].height.max(56.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, lane_height), Sense::click_and_drag());
        {
            let lane = &state.lanes[index];
            draw_lane(
                ui.painter_at(rect),
                rect,
                lane,
                &state.clip,
                state.zoom_x,
                state.scroll_px,
                theme,
            );
        }
        if response.hovered() {
            total_response = total_response.union(response.clone());
        }
        if let Some(pos) = response.interact_pointer_pos() {
            if response.dragged() || response.clicked() {
                interactions.push((index, rect, pos));
            }
        }
    }
    for (index, rect, pos) in interactions {
        if let Some(edit) = handle_lane_interaction(index, state, pos, rect) {
            edits.push(edit);
        }
    }
    LanesResult {
        response: total_response,
        edits,
    }
}

fn draw_lane(
    painter: Painter,
    rect: Rect,
    lane: &Lane,
    clip: &crate::model::Clip,
    zoom_x: f32,
    scroll_px: Vec2,
    theme: &Theme,
) {
    painter.rect_filled(rect, 4.0, theme.lane_background);
    painter.rect_stroke(rect, 0.0, theme.lane_border);
    match lane.kind {
        LaneKind::Velocity => paint_velocity_lane(painter, rect, clip, zoom_x, scroll_px, theme),
        _ => paint_curve_lane(painter, rect, lane, clip, zoom_x, scroll_px, theme),
    }
}

fn paint_velocity_lane(
    painter: Painter,
    rect: Rect,
    clip: &crate::model::Clip,
    zoom_x: f32,
    scroll_px: Vec2,
    theme: &Theme,
) {
    let baseline = rect.bottom() - 6.0;
    for note in &clip.notes {
        let time = note.start_ppq as f32 / clip.ppq() as f32;
        let x = rect.left() + time * zoom_x - scroll_px.x;
        if x < rect.left() - 20.0 || x > rect.right() + 20.0 {
            continue;
        }
        let value = note.vel as f32 / 127.0;
        let top = baseline - value * (rect.height() - 12.0);
        let stem_rect = Rect::from_two_pos(pos2(x - 1.0, baseline), pos2(x + 1.0, top));
        painter.rect_filled(stem_rect, 1.0, theme.note_border.color);
        let head = Rect::from_center_size(pos2(x, top), vec2(6.0, 6.0));
        painter.rect_filled(head, 2.0, theme.note_fill);
    }
}

fn paint_curve_lane(
    painter: Painter,
    rect: Rect,
    lane: &Lane,
    clip: &crate::model::Clip,
    zoom_x: f32,
    scroll_px: Vec2,
    theme: &Theme,
) {
    if lane.points.is_empty() {
        return;
    }
    let mut last = None;
    for point in &lane.points {
        let x = rect.left() + point.ppq as f32 / clip.ppq() as f32 * zoom_x - scroll_px.x;
        let y = rect.bottom() - point.value * rect.height();
        let pos = pos2(x, y);
        if let Some(prev) = last {
            painter.line_segment([prev, pos], theme.grid_beat);
        }
        painter.circle_filled(pos, 3.5, theme.note_border.color);
        last = Some(pos);
    }
}

fn handle_lane_interaction(
    index: usize,
    state: &mut EditorState,
    pointer: Pos2,
    rect: Rect,
) -> Option<Edit> {
    let ppq = state.ppq();
    let lane = state.lanes.get_mut(index)?;
    match lane.kind {
        LaneKind::Velocity => {
            let local_x = pointer.x - rect.left();
            let ppq = tools::pointer_to_ppq(&state.clip, state.zoom_x, state.scroll_px.x, local_x);
            let mut closest = None;
            let mut best_dist = f32::MAX;
            for note in &state.clip.notes {
                let dist = (note.start_ppq - ppq).abs() as f32;
                if dist < best_dist {
                    best_dist = dist;
                    closest = Some(note.id);
                }
            }
            if let Some(id) = closest {
                if let Some(note) = state.clip.notes.iter_mut().find(|n| n.id == id) {
                    let value = ((rect.bottom() - pointer.y) / rect.height()).clamp(0.0, 1.0);
                    note.vel = (value * 127.0).round() as u8;
                    return Some(Edit::Update {
                        id,
                        start_ppq: note.start_ppq,
                        dur_ppq: note.dur_ppq,
                        pitch: note.pitch,
                        vel: note.vel,
                        chan: note.chan,
                    });
                }
            }
            None
        }
        _ => {
            let local_x = pointer.x - rect.left();
            let time = tools::pointer_to_ppq(&state.clip, state.zoom_x, state.scroll_px.x, local_x);
            let value = ((rect.bottom() - pointer.y) / rect.height()).clamp(0.0, 1.0);
            if let Some(point) = lane
                .points
                .iter_mut()
                .find(|p| (p.ppq - time).abs() < ppq as i64 / 16)
            {
                point.value = value;
            } else {
                lane.points.push(ControllerPoint { ppq: time, value });
                lane.points.sort_by_key(|p| p.ppq);
            }
            for point in &mut lane.points {
                point.clamp();
            }
            Some(Edit::ControllerChange {
                lane: lane.kind,
                points: lane.points.clone(),
            })
        }
    }
}
