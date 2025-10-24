use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveShape {
    Step,
    Linear,
}

#[derive(Debug, Clone)]
pub struct CurvePoint {
    pub sample: u64,
    pub value: f32,
    pub shape: CurveShape,
}

impl CurvePoint {
    pub fn new(sample: u64, value: f32, shape: CurveShape) -> Self {
        Self {
            sample,
            value,
            shape,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutomationCurve {
    points: Vec<CurvePoint>,
}

impl AutomationCurve {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }

    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    pub fn add_point(&mut self, point: CurvePoint) {
        match self
            .points
            .binary_search_by_key(&point.sample, |existing| existing.sample)
        {
            Ok(index) => self.points[index] = point,
            Err(index) => self.points.insert(index, point),
        }
    }

    pub fn remove_after(&mut self, sample: u64) {
        let index = self.partition_point(|point| point.sample <= sample);
        self.points.truncate(index);
    }

    pub fn value_at(&self, sample: u64) -> Option<f32> {
        if self.points.is_empty() {
            return None;
        }

        let index = self.partition_point(|point| point.sample <= sample);
        if index == 0 {
            return Some(self.points[0].value);
        }

        let prev = &self.points[index - 1];
        if prev.sample == sample || index == self.points.len() {
            return Some(prev.value);
        }

        let next = &self.points[index];
        match prev.shape {
            CurveShape::Step => Some(prev.value),
            CurveShape::Linear => {
                let span = next.sample.saturating_sub(prev.sample);
                if span == 0 {
                    return Some(next.value);
                }
                let position = sample.saturating_sub(prev.sample) as f32;
                let span = span as f32;
                let t = (position / span).clamp(0.0, 1.0);
                Some(prev.value + (next.value - prev.value) * t)
            }
        }
    }

    pub fn last_value_before(&self, sample: u64) -> Option<f32> {
        if self.points.is_empty() {
            return None;
        }
        let index = self.partition_point(|point| point.sample < sample);
        if index == 0 {
            None
        } else {
            Some(self.points[index - 1].value)
        }
    }

    pub fn range_indices(&self, start: u64, end: u64) -> Range<usize> {
        if start >= end {
            return 0..0;
        }
        let start_index = self.partition_point(|point| point.sample < start);
        let end_index = self.partition_point(|point| point.sample < end);
        start_index..end_index
    }

    fn partition_point<F>(&self, predicate: F) -> usize
    where
        F: Fn(&CurvePoint) -> bool,
    {
        self.points.partition_point(predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_points_sorted() {
        let mut curve = AutomationCurve::new();
        curve.add_point(CurvePoint::new(32, 0.5, CurveShape::Linear));
        curve.add_point(CurvePoint::new(0, 0.0, CurveShape::Step));
        curve.add_point(CurvePoint::new(64, 1.0, CurveShape::Step));
        let samples: Vec<_> = curve.points.iter().map(|point| point.sample).collect();
        assert_eq!(samples, vec![0, 32, 64]);
    }

    #[test]
    fn overwrites_existing_sample() {
        let mut curve = AutomationCurve::new();
        curve.add_point(CurvePoint::new(0, 0.0, CurveShape::Step));
        curve.add_point(CurvePoint::new(0, 1.0, CurveShape::Step));
        assert_eq!(curve.len(), 1);
        assert_eq!(curve.value_at(0), Some(1.0));
    }

    #[test]
    fn linear_interpolation() {
        let mut curve = AutomationCurve::new();
        curve.add_point(CurvePoint::new(0, 0.0, CurveShape::Linear));
        curve.add_point(CurvePoint::new(10, 1.0, CurveShape::Linear));
        assert!((curve.value_at(5).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(curve.value_at(10), Some(1.0));
    }

    #[test]
    fn step_hold() {
        let mut curve = AutomationCurve::new();
        curve.add_point(CurvePoint::new(0, 0.25, CurveShape::Step));
        curve.add_point(CurvePoint::new(32, 0.75, CurveShape::Step));
        assert_eq!(curve.value_at(0), Some(0.25));
        assert_eq!(curve.value_at(31), Some(0.25));
        assert_eq!(curve.value_at(32), Some(0.75));
    }
}
