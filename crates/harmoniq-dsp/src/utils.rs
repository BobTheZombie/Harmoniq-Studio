#[inline]
pub fn flush_denormals() {
    #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
    #[allow(deprecated)]
    unsafe {
        use core::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        const DAZ_FTZ: u32 = 0x8040;
        let csr = _mm_getcsr();
        _mm_setcsr(csr | DAZ_FTZ);
    }
}

#[cfg(feature = "no-denormals")]
pub struct NoDenormalsGuard {
    #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
    prev: u32,
}

#[cfg(feature = "no-denormals")]
impl NoDenormalsGuard {
    #[inline]
    pub fn new() -> Self {
        #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
        #[allow(deprecated)]
        unsafe {
            use core::arch::x86_64::{_mm_getcsr, _mm_setcsr};
            const DAZ_FTZ: u32 = 0x8040;
            let prev = _mm_getcsr();
            _mm_setcsr(prev | DAZ_FTZ);
            return Self { prev };
        }
        #[cfg(not(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64"))))]
        {
            Self {}
        }
    }
}

#[cfg(feature = "no-denormals")]
impl Drop for NoDenormalsGuard {
    fn drop(&mut self) {
        #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
        #[allow(deprecated)]
        unsafe {
            use core::arch::x86_64::_mm_setcsr;
            _mm_setcsr(self.prev);
        }
    }
}

#[cfg(not(feature = "no-denormals"))]
#[derive(Clone, Copy, Debug)]
pub struct NoDenormalsGuard;

#[cfg(not(feature = "no-denormals"))]
impl NoDenormalsGuard {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}
