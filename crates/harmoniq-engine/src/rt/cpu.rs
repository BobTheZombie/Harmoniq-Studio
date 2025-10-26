use crate::config::RtParallelCfg;

#[allow(unused_variables)]
pub fn pin_current_thread_to(core: usize) {
    #[cfg(all(target_os = "linux", feature = "core_affinity"))]
    {
        if let Some(ids) = core_affinity::get_core_ids() {
            if ids.is_empty() {
                return;
            }
            let target = ids
                .get(core)
                .cloned()
                .unwrap_or_else(|| ids[core % ids.len()].clone());
            let _ = core_affinity::set_for_current(target);
        }
    }
}

pub fn pick_cores(cfg: &RtParallelCfg) -> (Option<usize>, Vec<usize>) {
    if !cfg.worker_cores.is_empty() {
        let mut workers = cfg.worker_cores.clone();
        let rt = cfg
            .pin_rt_core
            .or_else(|| workers.first().copied())
            .map(|core| normalize_core(core, cfg.avoid_smt));
        if let Some(rt_core) = rt {
            workers.retain(|c| *c != rt_core);
        }
        workers.truncate(cfg.workers as usize);
        return (rt, workers);
    }

    let total = if cfg.avoid_smt {
        num_cpus::get_physical().max(1)
    } else {
        num_cpus::get().max(1)
    };

    let rt_core = cfg
        .pin_rt_core
        .map(|core| normalize_index(core, total))
        .or(Some(0));

    let mut workers = Vec::new();
    for idx in 0..total {
        if Some(idx) == rt_core {
            continue;
        }
        workers.push(idx);
        if workers.len() >= cfg.workers as usize {
            break;
        }
    }

    (rt_core, workers)
}

fn normalize_core(core: usize, avoid_smt: bool) -> usize {
    if avoid_smt {
        let phys = num_cpus::get_physical().max(1);
        normalize_index(core, phys)
    } else {
        let logical = num_cpus::get().max(1);
        normalize_index(core, logical)
    }
}

fn normalize_index(idx: usize, limit: usize) -> usize {
    if limit == 0 {
        0
    } else {
        idx % limit
    }
}
