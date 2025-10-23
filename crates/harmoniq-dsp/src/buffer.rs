use core::marker::PhantomData;
use core::slice;

#[derive(Clone, Copy)]
enum BlockLayout<'a> {
    Interleaved {
        ptr: *const f32,
        stride: usize,
    },
    Planar {
        planes: &'a [*const f32],
        stride: usize,
    },
    Empty,
}

#[derive(Clone, Copy)]
pub struct AudioBlock<'a> {
    chans: u32,
    frames: u32,
    layout: BlockLayout<'a>,
}

enum BlockLayoutMut<'a> {
    Interleaved {
        ptr: *mut f32,
        stride: usize,
    },
    Planar {
        planes: &'a mut [*mut f32],
        stride: usize,
    },
    Empty,
}

pub struct AudioBlockMut<'a> {
    chans: u32,
    frames: u32,
    layout: BlockLayoutMut<'a>,
}

#[derive(Clone, Copy)]
pub struct ChanRef<'a> {
    ptr: *const f32,
    frames: usize,
    stride: usize,
    _pd: PhantomData<&'a f32>,
}

pub struct ChanMut<'a> {
    ptr: *mut f32,
    frames: usize,
    stride: usize,
    _pd: PhantomData<&'a mut f32>,
}

impl<'a> AudioBlock<'a> {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            chans: 0,
            frames: 0,
            layout: BlockLayout::Empty,
        }
    }

    #[inline]
    pub unsafe fn from_interleaved(ptr: *const f32, chans: u32, frames: u32) -> Self {
        debug_assert!(!ptr.is_null());
        Self {
            chans,
            frames,
            layout: BlockLayout::Interleaved {
                ptr,
                stride: chans as usize,
            },
        }
    }

    #[inline]
    pub unsafe fn from_planar(planes: &'a [*const f32], chans: u32, frames: u32) -> Self {
        debug_assert_eq!(planes.len(), chans as usize);
        Self {
            chans,
            frames,
            layout: BlockLayout::Planar { planes, stride: 1 },
        }
    }

    #[inline]
    pub fn channels(&self) -> u32 {
        self.chans
    }

    #[inline]
    pub fn frames(&self) -> u32 {
        self.frames
    }

    #[inline]
    pub fn is_interleaved(&self) -> bool {
        matches!(self.layout, BlockLayout::Interleaved { .. })
    }

    #[inline]
    pub unsafe fn interleaved_ptr(&self) -> Option<*const f32> {
        match self.layout {
            BlockLayout::Interleaved { ptr, .. } => Some(ptr),
            _ => None,
        }
    }

    #[inline]
    pub fn planes_ptrs(&self) -> Option<&'a [*const f32]> {
        match self.layout {
            BlockLayout::Planar { planes, .. } => Some(planes),
            _ => None,
        }
    }

    #[inline]
    pub unsafe fn read_sample(&self, channel: usize, frame: usize) -> f32 {
        debug_assert!(channel < self.chans as usize);
        debug_assert!(frame < self.frames as usize);
        match self.layout {
            BlockLayout::Interleaved { ptr, stride } => unsafe {
                *ptr.add(frame * stride + channel)
            },
            BlockLayout::Planar { planes, stride } => {
                let plane = unsafe { *planes.get_unchecked(channel) };
                unsafe { *plane.add(frame * stride) }
            }
            BlockLayout::Empty => 0.0,
        }
    }

    #[inline]
    pub unsafe fn chan(&self, idx: usize) -> ChanRef<'a> {
        debug_assert!(idx < self.chans as usize);
        match self.layout {
            BlockLayout::Interleaved { ptr, stride } => ChanRef {
                ptr: unsafe { ptr.add(idx) },
                frames: self.frames as usize,
                stride,
                _pd: PhantomData,
            },
            BlockLayout::Planar { planes, stride } => ChanRef {
                ptr: unsafe { *planes.get_unchecked(idx) },
                frames: self.frames as usize,
                stride,
                _pd: PhantomData,
            },
            BlockLayout::Empty => ChanRef {
                ptr: core::ptr::null(),
                frames: 0,
                stride: 0,
                _pd: PhantomData,
            },
        }
    }
}

impl<'a> AudioBlockMut<'a> {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            chans: 0,
            frames: 0,
            layout: BlockLayoutMut::Empty,
        }
    }

    #[inline]
    pub unsafe fn from_interleaved(ptr: *mut f32, chans: u32, frames: u32) -> Self {
        debug_assert!(!ptr.is_null());
        Self {
            chans,
            frames,
            layout: BlockLayoutMut::Interleaved {
                ptr,
                stride: chans as usize,
            },
        }
    }

    #[inline]
    pub unsafe fn from_planar(planes: &'a mut [*mut f32], chans: u32, frames: u32) -> Self {
        debug_assert_eq!(planes.len(), chans as usize);
        Self {
            chans,
            frames,
            layout: BlockLayoutMut::Planar { planes, stride: 1 },
        }
    }

    #[inline]
    pub fn channels(&self) -> u32 {
        self.chans
    }

    #[inline]
    pub fn frames(&self) -> u32 {
        self.frames
    }

    #[inline]
    pub fn is_interleaved(&self) -> bool {
        matches!(self.layout, BlockLayoutMut::Interleaved { .. })
    }

    #[inline]
    pub unsafe fn interleaved_ptr_mut(&mut self) -> Option<*mut f32> {
        match &mut self.layout {
            BlockLayoutMut::Interleaved { ptr, .. } => Some(*ptr),
            _ => None,
        }
    }

    #[inline]
    pub fn planes_ptrs_mut(&mut self) -> Option<&mut [*mut f32]> {
        match &mut self.layout {
            BlockLayoutMut::Planar { planes, .. } => Some(*planes),
            _ => None,
        }
    }

    #[inline]
    pub unsafe fn write_sample(&mut self, channel: usize, frame: usize, value: f32) {
        debug_assert!(channel < self.chans as usize);
        debug_assert!(frame < self.frames as usize);
        match &mut self.layout {
            BlockLayoutMut::Interleaved { ptr, stride } => {
                unsafe { *ptr.add(frame * *stride + channel) = value };
            }
            BlockLayoutMut::Planar { planes, stride } => {
                let plane = unsafe { *planes.get_unchecked_mut(channel) };
                unsafe { *plane.add(frame * *stride) = value };
            }
            BlockLayoutMut::Empty => {}
        }
    }

    #[inline]
    pub unsafe fn chan_mut(&mut self, idx: usize) -> ChanMut<'a> {
        debug_assert!(idx < self.chans as usize);
        match &mut self.layout {
            BlockLayoutMut::Interleaved { ptr, stride } => ChanMut {
                ptr: unsafe { ptr.add(idx) },
                frames: self.frames as usize,
                stride: *stride,
                _pd: PhantomData,
            },
            BlockLayoutMut::Planar { planes, stride } => ChanMut {
                ptr: unsafe { *planes.get_unchecked_mut(idx) },
                frames: self.frames as usize,
                stride: *stride,
                _pd: PhantomData,
            },
            BlockLayoutMut::Empty => ChanMut {
                ptr: core::ptr::null_mut(),
                frames: 0,
                stride: 0,
                _pd: PhantomData,
            },
        }
    }

    #[inline]
    pub fn fill(&mut self, value: f32) {
        for frame in 0..self.frames as usize {
            for channel in 0..self.chans as usize {
                unsafe { self.write_sample(channel, frame, value) };
            }
        }
    }
}

impl<'a> ChanRef<'a> {
    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub unsafe fn read(&self, frame: usize) -> f32 {
        debug_assert!(frame < self.frames);
        unsafe { *self.ptr.add(frame * self.stride) }
    }

    #[inline]
    pub unsafe fn as_slice(&self) -> Option<&'a [f32]> {
        if self.stride == 1 {
            Some(unsafe { slice::from_raw_parts(self.ptr, self.frames) })
        } else {
            None
        }
    }
}

impl<'a> ChanMut<'a> {
    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub unsafe fn read(&self, frame: usize) -> f32 {
        debug_assert!(frame < self.frames);
        unsafe { *self.ptr.add(frame * self.stride) }
    }

    #[inline]
    pub unsafe fn write(&mut self, frame: usize, value: f32) {
        debug_assert!(frame < self.frames);
        unsafe { *self.ptr.add(frame * self.stride) = value };
    }

    #[inline]
    pub unsafe fn as_mut_slice(&mut self) -> Option<&'a mut [f32]> {
        if self.stride == 1 {
            Some(unsafe { slice::from_raw_parts_mut(self.ptr, self.frames) })
        } else {
            None
        }
    }
}
