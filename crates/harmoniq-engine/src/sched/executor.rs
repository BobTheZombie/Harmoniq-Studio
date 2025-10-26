use std::sync::atomic::Ordering;

use crate::transport::Transport;

use super::{buffer, events};

pub struct ExecCtx {
    pub sr: u32,
    pub transport: Transport,
}

pub unsafe fn process_block(
    engine: *mut crate::engine::Engine,
    in_ptr: *const f32,
    out_ptr: *mut f32,
    frames: u32,
) {
    if engine.is_null() || frames == 0 {
        return;
    }

    let e = &mut *engine;
    let mut bufs = buffer::make(in_ptr, out_ptr, frames);
    let events = events::slice_for_block(&e.event_lane, e.sample_pos, frames);

    if e.graph.order.is_empty() {
        for node in e.graph.nodes.iter_mut() {
            node.process(&mut bufs, &events);
        }
    } else {
        for id in e.graph.order.iter().copied() {
            if let Some(node) = e.graph.nodes.get_mut(id as usize) {
                node.process(&mut bufs, &events);
            }
        }
    }

    e.sample_pos = e.sample_pos.saturating_add(frames as u64);
    e.transport
        .sample_pos
        .store(e.sample_pos, Ordering::Relaxed);
}
