use std::slice;

#[derive(Clone, Copy)]
pub struct AudioView<'a> {
    channels: usize,
    frames: usize,
    kind: AudioViewKind<'a>,
}

#[derive(Clone, Copy)]
enum AudioViewKind<'a> {
    Interleaved(&'a [f32]),
    Planar(&'a [*const f32]),
    Empty,
}

impl<'a> AudioView<'a> {
    #[inline]
    pub fn empty() -> Self {
        Self {
            channels: 0,
            frames: 0,
            kind: AudioViewKind::Empty,
        }
    }

    #[inline]
    pub fn from_planes(planes: &'a [*const f32], frames: usize) -> Self {
        Self {
            channels: planes.len(),
            frames,
            kind: AudioViewKind::Planar(planes),
        }
    }

    #[inline]
    pub fn from_interleaved_view(buf: &'a [f32], channels: usize, frames: usize) -> Self {
        Self {
            channels,
            frames,
            kind: AudioViewKind::Interleaved(buf),
        }
    }

    #[inline]
    pub fn channels(&self) -> usize {
        self.channels
    }

    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }

    #[inline]
    pub fn interleaved(&self) -> Option<&'a [f32]> {
        match self.kind {
            AudioViewKind::Interleaved(buf) => Some(buf),
            _ => None,
        }
    }

    #[inline]
    pub fn planes_ptrs(&self) -> Option<&'a [*const f32]> {
        match self.kind {
            AudioViewKind::Planar(ptrs) => Some(ptrs),
            _ => None,
        }
    }

    #[inline]
    pub fn plane(&self, index: usize) -> Option<&'a [f32]> {
        match self.kind {
            AudioViewKind::Planar(ptrs) => ptrs
                .get(index)
                .map(|ptr| unsafe { slice::from_raw_parts(*ptr, self.frames) }),
            _ => None,
        }
    }
}

pub struct AudioViewMut<'a> {
    channels: usize,
    frames: usize,
    kind: AudioViewMutKind<'a>,
}

enum AudioViewMutKind<'a> {
    Interleaved(&'a mut [f32]),
    Planar(&'a mut [*mut f32]),
    Empty,
}

pub struct PlanarMut<'a> {
    planes: &'a mut [*mut f32],
    frames: usize,
}

impl<'a> PlanarMut<'a> {
    #[inline]
    pub fn planes(&mut self) -> &mut [*mut f32] {
        self.planes
    }

    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }
}

impl<'a> AudioViewMut<'a> {
    #[inline]
    pub fn empty() -> Self {
        Self {
            channels: 0,
            frames: 0,
            kind: AudioViewMutKind::Empty,
        }
    }

    #[inline]
    pub fn from_planes(planes: &'a mut [*mut f32], frames: usize) -> Self {
        Self {
            channels: planes.len(),
            frames,
            kind: AudioViewMutKind::Planar(planes),
        }
    }

    #[inline]
    pub fn from_interleaved_view(buf: &'a mut [f32], channels: usize, frames: usize) -> Self {
        Self {
            channels,
            frames,
            kind: AudioViewMutKind::Interleaved(buf),
        }
    }

    #[inline]
    pub fn channels(&self) -> usize {
        self.channels
    }

    #[inline]
    pub fn frames(&self) -> usize {
        self.frames
    }

    #[inline]
    pub fn interleaved_mut(&mut self) -> Option<&mut [f32]> {
        match &mut self.kind {
            AudioViewMutKind::Interleaved(buf) => Some(*buf),
            _ => None,
        }
    }

    #[inline]
    pub fn planar(&mut self) -> Option<PlanarMut<'_>> {
        match &mut self.kind {
            AudioViewMutKind::Planar(planes) => Some(PlanarMut {
                planes: *planes,
                frames: self.frames,
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn planes_ptrs_mut(&mut self) -> Option<&mut [*mut f32]> {
        match &mut self.kind {
            AudioViewMutKind::Planar(planes) => Some(*planes),
            _ => None,
        }
    }

    #[inline]
    pub fn plane_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        match &mut self.kind {
            AudioViewMutKind::Planar(planes) => planes
                .get_mut(index)
                .map(|ptr| unsafe { slice::from_raw_parts_mut(*ptr, self.frames) }),
            _ => None,
        }
    }
}
