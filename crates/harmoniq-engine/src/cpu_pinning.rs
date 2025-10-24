#[cfg(feature = "core_affinity")]
pub fn pin_render_thread() {
    use core_affinity::{get_core_ids, set_for_current, CoreId};

    let requested = std::env::var("HARMONIQ_RENDER_CORE")
        .ok()
        .and_then(|value| {
            value
                .parse::<usize>()
                .ok()
                .map(|index| CoreId { id: index })
        });

    if let Some(core) = requested {
        let _ = set_for_current(core);
        return;
    }

    if let Some(mut cores) = get_core_ids() {
        cores.sort_by_key(|core| core.id);
        if let Some(core) = cores.into_iter().rev().next() {
            let _ = set_for_current(core);
        }
    }
}

#[cfg(not(feature = "core_affinity"))]
pub fn pin_render_thread() {}
