use crate::buffer::{AudioBlock, AudioBlockMut, ChanMut, ChanRef};

#[inline]
fn process_channel(mut dst: ChanMut<'_>, src: ChanRef<'_>, frames: usize, gain: f32) {
    #[cfg(feature = "simd")]
    {
        if let (Some(src_slice), Some(dst_slice)) =
            (unsafe { src.as_slice() }, unsafe { dst.as_mut_slice() })
        {
            let frames = frames.min(src_slice.len()).min(dst_slice.len());
            crate::simd::mul_scalar_to(&mut dst_slice[..frames], &src_slice[..frames], gain);
            return;
        }
    }

    let frames = frames.min(src.frames()).min(dst.frames());
    for frame in 0..frames {
        let sample = unsafe { src.read(frame) };
        unsafe { dst.write(frame, sample * gain) };
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Gain {
    linear: f32,
}

impl Gain {
    #[inline]
    pub fn unity() -> Self {
        Self { linear: 1.0 }
    }

    #[inline]
    pub fn from_db(db: f32) -> Self {
        Self {
            linear: db_to_linear(db),
        }
    }

    #[inline]
    pub fn set_linear(&mut self, linear: f32) {
        self.linear = linear;
    }

    #[inline]
    pub fn linear(&self) -> f32 {
        self.linear
    }

    #[inline]
    pub fn process(&self, input: &AudioBlock<'_>, output: &mut AudioBlockMut<'_>) {
        let channels = output.channels().min(input.channels()) as usize;
        let frames = output.frames().min(input.frames()) as usize;
        for ch in 0..channels {
            unsafe {
                let src = input.chan(ch);
                let dst = output.chan_mut(ch);
                process_channel(dst, src, frames, self.linear);
            }
        }
        for ch in channels..output.channels() as usize {
            unsafe {
                let mut dst = output.chan_mut(ch);
                for frame in 0..frames.min(dst.frames()) {
                    dst.write(frame, 0.0);
                }
            }
        }
    }
}

#[inline]
pub fn db_to_linear(db: f32) -> f32 {
    if db <= -120.0 {
        0.0
    } else {
        10.0f32.powf(db * 0.05)
    }
}

#[inline]
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * linear.max(1e-12).log10()
    }
}
