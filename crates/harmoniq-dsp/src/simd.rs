#![allow(dead_code)]

use core::slice;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use std::arch::is_x86_feature_detected;

#[inline]
fn mul_f32(a: f32, b: f32) -> f32 {
    #[cfg(feature = "fast-math")]
    {
        a.mul_add(b, 0.0)
    }
    #[cfg(not(feature = "fast-math"))]
    {
        a * b
    }
}

#[inline]
pub fn mul_scalar_to(output: &mut [f32], input: &[f32], gain: f32) {
    assert_eq!(output.len(), input.len());
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { mul_scalar_avx2(output, input, gain) };
            return;
        }
        if is_x86_feature_detected!("sse2") {
            unsafe { mul_scalar_sse2(output, input, gain) };
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            unsafe { mul_scalar_neon(output, input, gain) };
            return;
        }
    }

    for (dst, src) in output.iter_mut().zip(input.iter()) {
        *dst = mul_f32(*src, gain);
    }
}

#[inline]
pub fn mul_scalar_in_place(buffer: &mut [f32], gain: f32) {
    let len = buffer.len();
    let input = unsafe { slice::from_raw_parts(buffer.as_ptr(), len) };
    mul_scalar_to(buffer, input, gain);
}

#[inline]
pub fn mul_buffers_to(output: &mut [f32], lhs: &[f32], rhs: &[f32]) {
    assert_eq!(output.len(), lhs.len());
    assert_eq!(lhs.len(), rhs.len());

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { mul_buffers_avx2(output, lhs, rhs) };
            return;
        }
        if is_x86_feature_detected!("sse2") {
            unsafe { mul_buffers_sse2(output, lhs, rhs) };
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            unsafe { mul_buffers_neon(output, lhs, rhs) };
            return;
        }
    }

    for ((dst, l), r) in output.iter_mut().zip(lhs.iter()).zip(rhs.iter()) {
        *dst = mul_f32(*l, *r);
    }
}

#[inline]
pub fn mul_buffers_in_place(buffer: &mut [f32], rhs: &[f32]) {
    assert_eq!(buffer.len(), rhs.len());

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { mul_buffers_in_place_avx2(buffer, rhs) };
            return;
        }
        if is_x86_feature_detected!("sse2") {
            unsafe { mul_buffers_in_place_sse2(buffer, rhs) };
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            unsafe { mul_buffers_in_place_neon(buffer, rhs) };
            return;
        }
    }

    for (dst, factor) in buffer.iter_mut().zip(rhs.iter()) {
        *dst = mul_f32(*dst, *factor);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_scalar_sse2(output: &mut [f32], input: &[f32], gain: f32) {
    use core::arch::x86_64::*;

    let gain_vec = unsafe { _mm_set1_ps(gain) };
    let mut index = 0usize;
    let len = output.len().min(input.len());
    while index + 4 <= len {
        unsafe {
            let src = _mm_loadu_ps(input.as_ptr().add(index));
            let mul = _mm_mul_ps(src, gain_vec);
            _mm_storeu_ps(output.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        output[index] = mul_f32(input[index], gain);
        index += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_scalar_avx2(output: &mut [f32], input: &[f32], gain: f32) {
    use core::arch::x86_64::*;

    let gain_vec = unsafe { _mm256_set1_ps(gain) };
    let mut index = 0usize;
    let len = output.len().min(input.len());
    while index + 8 <= len {
        unsafe {
            let src = _mm256_loadu_ps(input.as_ptr().add(index));
            let mul = _mm256_mul_ps(src, gain_vec);
            _mm256_storeu_ps(output.as_mut_ptr().add(index), mul);
        }
        index += 8;
    }
    while index < len {
        output[index] = mul_f32(input[index], gain);
        index += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_buffers_sse2(output: &mut [f32], lhs: &[f32], rhs: &[f32]) {
    use core::arch::x86_64::*;

    let mut index = 0usize;
    let len = output.len().min(lhs.len()).min(rhs.len());
    while index + 4 <= len {
        unsafe {
            let left = _mm_loadu_ps(lhs.as_ptr().add(index));
            let right = _mm_loadu_ps(rhs.as_ptr().add(index));
            let mul = _mm_mul_ps(left, right);
            _mm_storeu_ps(output.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        output[index] = mul_f32(lhs[index], rhs[index]);
        index += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_buffers_avx2(output: &mut [f32], lhs: &[f32], rhs: &[f32]) {
    use core::arch::x86_64::*;

    let mut index = 0usize;
    let len = output.len().min(lhs.len()).min(rhs.len());
    while index + 8 <= len {
        unsafe {
            let left = _mm256_loadu_ps(lhs.as_ptr().add(index));
            let right = _mm256_loadu_ps(rhs.as_ptr().add(index));
            let mul = _mm256_mul_ps(left, right);
            _mm256_storeu_ps(output.as_mut_ptr().add(index), mul);
        }
        index += 8;
    }
    while index < len {
        output[index] = mul_f32(lhs[index], rhs[index]);
        index += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_buffers_in_place_sse2(buffer: &mut [f32], rhs: &[f32]) {
    use core::arch::x86_64::*;

    let mut index = 0usize;
    let len = buffer.len().min(rhs.len());
    while index + 4 <= len {
        unsafe {
            let left = _mm_loadu_ps(buffer.as_ptr().add(index));
            let right = _mm_loadu_ps(rhs.as_ptr().add(index));
            let mul = _mm_mul_ps(left, right);
            _mm_storeu_ps(buffer.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        let value = mul_f32(buffer[index], rhs[index]);
        buffer[index] = value;
        index += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
unsafe fn mul_buffers_in_place_avx2(buffer: &mut [f32], rhs: &[f32]) {
    use core::arch::x86_64::*;

    let mut index = 0usize;
    let len = buffer.len().min(rhs.len());
    while index + 8 <= len {
        unsafe {
            let left = _mm256_loadu_ps(buffer.as_ptr().add(index));
            let right = _mm256_loadu_ps(rhs.as_ptr().add(index));
            let mul = _mm256_mul_ps(left, right);
            _mm256_storeu_ps(buffer.as_mut_ptr().add(index), mul);
        }
        index += 8;
    }
    while index < len {
        let value = mul_f32(buffer[index], rhs[index]);
        buffer[index] = value;
        index += 1;
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn mul_scalar_neon(output: &mut [f32], input: &[f32], gain: f32) {
    use core::arch::aarch64::*;

    let gain_vec = unsafe { vdupq_n_f32(gain) };
    let mut index = 0usize;
    let len = output.len().min(input.len());
    while index + 4 <= len {
        unsafe {
            let src = vld1q_f32(input.as_ptr().add(index));
            let mul = vmulq_f32(src, gain_vec);
            vst1q_f32(output.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        output[index] = mul_f32(input[index], gain);
        index += 1;
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn mul_buffers_neon(output: &mut [f32], lhs: &[f32], rhs: &[f32]) {
    use core::arch::aarch64::*;

    let mut index = 0usize;
    let len = output.len().min(lhs.len()).min(rhs.len());
    while index + 4 <= len {
        unsafe {
            let left = vld1q_f32(lhs.as_ptr().add(index));
            let right = vld1q_f32(rhs.as_ptr().add(index));
            let mul = vmulq_f32(left, right);
            vst1q_f32(output.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        output[index] = mul_f32(lhs[index], rhs[index]);
        index += 1;
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn mul_buffers_in_place_neon(buffer: &mut [f32], rhs: &[f32]) {
    use core::arch::aarch64::*;

    let mut index = 0usize;
    let len = buffer.len().min(rhs.len());
    while index + 4 <= len {
        unsafe {
            let left = vld1q_f32(buffer.as_ptr().add(index));
            let right = vld1q_f32(rhs.as_ptr().add(index));
            let mul = vmulq_f32(left, right);
            vst1q_f32(buffer.as_mut_ptr().add(index), mul);
        }
        index += 4;
    }
    while index < len {
        let value = mul_f32(buffer[index], rhs[index]);
        buffer[index] = value;
        index += 1;
    }
}
