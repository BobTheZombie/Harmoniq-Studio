use core::ptr::NonNull;
use core::slice;

pub struct AudioBuffers<'a> {
    pub ins: [&'a [f32]; 2],
    pub outs: [&'a mut [f32]; 2],
    pub nframes: u32,
}

#[inline]
fn empty_out_slice<'a>() -> &'a mut [f32] {
    unsafe { slice::from_raw_parts_mut(NonNull::<f32>::dangling().as_ptr(), 0) }
}

pub fn make<'a>(in_ptr: *const f32, out_ptr: *mut f32, frames: u32) -> AudioBuffers<'a> {
    let frames_usize = frames as usize;

    let ins = if !in_ptr.is_null() && frames_usize > 0 {
        let total = frames_usize.saturating_mul(2);
        let samples = unsafe { slice::from_raw_parts(in_ptr, total) };
        let (l, r) = samples.split_at(frames_usize);
        [l, &r[..frames_usize]]
    } else {
        [&[][..], &[][..]]
    };

    let outs = if !out_ptr.is_null() && frames_usize > 0 {
        let total = frames_usize.saturating_mul(2);
        let samples = unsafe { slice::from_raw_parts_mut(out_ptr, total) };
        let (l, r) = samples.split_at_mut(frames_usize);
        [l, &mut r[..frames_usize]]
    } else {
        [empty_out_slice(), empty_out_slice()]
    };

    AudioBuffers {
        ins,
        outs,
        nframes: frames,
    }
}

pub struct BlockBuffers {
    audio: AudioBuffers<'static>,
}

impl BlockBuffers {
    pub unsafe fn from_raw(in_ptr: *const f32, out_ptr: *mut f32, frames: u32) -> Self {
        let audio = make(in_ptr, out_ptr, frames);
        let audio_static: AudioBuffers<'static> = core::mem::transmute(audio);
        Self {
            audio: audio_static,
        }
    }

    #[inline]
    pub fn as_audio(&mut self) -> &mut AudioBuffers<'static> {
        &mut self.audio
    }
}

pub unsafe fn make_block(in_ptr: *const f32, out_ptr: *mut f32, frames: u32) -> BlockBuffers {
    BlockBuffers::from_raw(in_ptr, out_ptr, frames)
}
