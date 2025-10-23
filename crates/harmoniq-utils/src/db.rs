//! Conversions between linear gain and decibels.

use std::f32::consts::LN_10;

use crate::Decibels;

/// Minimum linear value treated as silence to avoid numerical issues.
const MIN_LINEAR: f32 = 1e-7;

/// Converts a linear gain factor to decibels.
#[inline]
pub fn gain_to_db(gain: f32) -> Decibels {
    if gain <= MIN_LINEAR {
        f32::NEG_INFINITY
    } else {
        20.0 * (gain.max(MIN_LINEAR)).ln() / LN_10
    }
}

/// Converts decibels to a linear gain factor.
#[inline]
pub fn db_to_gain(db: Decibels) -> f32 {
    if db <= f32::NEG_INFINITY {
        0.0
    } else {
        (db * LN_10 / 20.0).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_roundtrip() {
        let values = [0.1, 0.5, 1.0, 2.0, 10.0];
        for value in values {
            let db = gain_to_db(value);
            let round = db_to_gain(db);
            assert!((round - value).abs() < 1e-6);
        }
    }
}
