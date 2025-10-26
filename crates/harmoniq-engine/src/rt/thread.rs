pub unsafe fn enter_hard_rt() {
    #[cfg(target_os = "linux")]
    {
        let _ = libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE);
        use libc::{sched_param, sched_setscheduler, SCHED_FIFO};
        let sp = sched_param { sched_priority: 70 };
        let _ = sched_setscheduler(0, SCHED_FIFO, &sp);
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        #[cfg(target_arch = "x86")]
        use core::arch::x86::{_mm_getcsr, _mm_setcsr};
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        unsafe {
            _mm_setcsr(_mm_getcsr() | 0x8040);
        }
    }
}
