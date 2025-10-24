use crate::buffer::AudioBuffer;

/// Simple Mid/Side stereo width processor.
#[derive(Clone, Debug)]
pub struct StereoWidthNode {
    width: f32,
}

impl StereoWidthNode {
    pub fn new(width: f32) -> Self {
        Self { width }
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.0, 2.0);
    }

    pub fn process_buffer(&self, buffer: &mut AudioBuffer) {
        if buffer.channel_count() < 2 || buffer.is_empty() {
            return;
        }
        let frames = buffer.len();
        let width = self.width;
        let data = buffer.as_mut_slice();
        let stride = frames;
        let left = &mut data[0..frames];
        let right = &mut data[stride..stride * 2];
        for frame in 0..frames {
            let l = left[frame];
            let r = right[frame];
            let mid = 0.5 * (l + r);
            let side = 0.5 * (l - r) * width;
            left[frame] = mid + side;
            right[frame] = mid - side;
        }
    }
}

impl Default for StereoWidthNode {
    fn default() -> Self {
        Self::new(1.0)
    }
}
