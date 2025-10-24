use std::collections::VecDeque;

#[derive(Debug, Default)]
pub struct AudioRing {
    left: VecDeque<f32>,
    right: VecDeque<f32>,
    capacity: usize,
}

impl AudioRing {
    pub fn with_capacity(frames: usize) -> Self {
        Self {
            left: VecDeque::with_capacity(frames),
            right: VecDeque::with_capacity(frames),
            capacity: frames,
        }
    }

    pub fn push_frame(&mut self, left: f32, right: f32) {
        if self.left.len() >= self.capacity {
            let _ = self.left.pop_front();
            let _ = self.right.pop_front();
        }
        self.left.push_back(left);
        self.right.push_back(right);
    }

    pub fn pop_frame(&mut self) -> Option<(f32, f32)> {
        let left = self.left.pop_front()?;
        let right = self.right.pop_front()?;
        Some((left, right))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_roundtrip() {
        let mut ring = AudioRing::with_capacity(1);
        ring.push_frame(0.5, -0.5);
        assert_eq!(ring.pop_frame(), Some((0.5, -0.5)));
    }
}
