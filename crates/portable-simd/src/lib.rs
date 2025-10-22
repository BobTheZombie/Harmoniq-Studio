use core::ops::{Add, AddAssign, Mul, Sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Simd<T, const LANES: usize>([T; LANES]);

impl<T: Copy + Default, const LANES: usize> Simd<T, LANES> {
    pub fn splat(value: T) -> Self {
        Self([value; LANES])
    }

    pub fn from_slice(slice: &[T]) -> Self {
        let mut data = [T::default(); LANES];
        data.copy_from_slice(&slice[..LANES]);
        Self(data)
    }

    pub fn from_array(array: [T; LANES]) -> Self {
        Self(array)
    }

    pub fn write_to_slice(self, slice: &mut [T]) {
        slice[..LANES].copy_from_slice(&self.0);
    }

    pub fn to_array(self) -> [T; LANES] {
        self.0
    }
}

impl<const LANES: usize> Add for Simd<f32, LANES> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut result = [0.0f32; LANES];
        for i in 0..LANES {
            result[i] = self.0[i] + rhs.0[i];
        }
        Self(result)
    }
}

impl<const LANES: usize> Sub for Simd<f32, LANES> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut result = [0.0f32; LANES];
        for i in 0..LANES {
            result[i] = self.0[i] - rhs.0[i];
        }
        Self(result)
    }
}

impl<const LANES: usize> Mul for Simd<f32, LANES> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut result = [0.0f32; LANES];
        for i in 0..LANES {
            result[i] = self.0[i] * rhs.0[i];
        }
        Self(result)
    }
}

impl<const LANES: usize> AddAssign for Simd<f32, LANES> {
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..LANES {
            self.0[i] += rhs.0[i];
        }
    }
}

impl<const LANES: usize> Simd<f32, LANES> {
    pub fn as_array(&self) -> &[f32; LANES] {
        &self.0
    }
}
