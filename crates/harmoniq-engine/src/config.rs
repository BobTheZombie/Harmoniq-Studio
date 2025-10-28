#[derive(Clone, Debug)]
pub struct RtParallelCfg {
    pub workers: u32,
    pub pin_rt_core: Option<usize>,
    pub worker_cores: Vec<usize>,
    pub avoid_smt: bool,
}

impl Default for RtParallelCfg {
    fn default() -> Self {
        let phys = num_cpus::get_physical().max(2);
        Self {
            workers: (phys - 1) as u32,
            pin_rt_core: None,
            worker_cores: Vec::new(),
            avoid_smt: true,
        }
    }
}
