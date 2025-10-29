//! RT-safe helpers used by audio backends (ASIO/OpenASIO/WASAPI/etc).
//! No allocations or locks; use in the audio callback.

#[inline(always)]
pub fn zero_f32(buf: &mut [f32]) {
    for x in buf {
        *x = 0.0;
    }
}

#[inline(always)]
pub fn sanitize_f32(buf: &mut [f32]) {
    for x in buf {
        if !x.is_finite() {
            *x = 0.0;
        } else if *x > 1.0 {
            *x = 1.0;
        } else if *x < -1.0 {
            *x = -1.0;
        }
    }
}

#[inline(always)]
pub fn deinterleave_f32_channel(
    src_interleaved: &[f32],
    dst_plane: &mut [f32],
    frames: usize,
    channels: usize,
    ch: usize,
) {
    let mut i = ch;
    for f in 0..frames {
        dst_plane[f] = src_interleaved[i];
        i += channels;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub fn enable_denormal_kill_once() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
            let mut csr = _mm_getcsr();
            csr |= 1 << 6; // DAZ
            csr |= 1 << 15; // FTZ
            _mm_setcsr(csr);
        }
        #[cfg(target_arch = "x86")]
        {
            use std::arch::x86::{_mm_getcsr, _mm_setcsr};
            let mut csr = _mm_getcsr();
            csr |= 1 << 6; // DAZ
            csr |= 1 << 15; // FTZ
            _mm_setcsr(csr);
        }
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[inline]
pub fn enable_denormal_kill_once() {}
