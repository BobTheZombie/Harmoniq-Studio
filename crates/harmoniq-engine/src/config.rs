#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePowerPolicy {
    RunWhenStopped,
    SuspendWhenSafe,
}

impl Default for EnginePowerPolicy {
    fn default() -> Self {
        EnginePowerPolicy::SuspendWhenSafe
    }
}

#[derive(Clone, Debug)]
pub struct RtParallelCfg {
    pub workers: u32,
    pub power: EnginePowerPolicy,
    pub pin_rt_core: Option<usize>,
    pub worker_cores: Vec<usize>,
    pub avoid_smt: bool,
}

impl Default for RtParallelCfg {
    fn default() -> Self {
        let phys = num_cpus::get_physical().saturating_sub(1).max(1);
        Self {
            workers: phys as u32,
            power: EnginePowerPolicy::default(),
            pin_rt_core: None,
            worker_cores: Vec::new(),
            avoid_smt: true,
        }
    }
}
