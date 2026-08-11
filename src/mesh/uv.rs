use std::f64::consts::PI;

/// Normalize a UV value from [min, max] to [0, 2π) so that unwrap_angle works correctly.
pub fn normalize_uv(val: f64, min: f64, max: f64) -> f64 {
    let range = max - min;
    if range < 1e-12 {
        return 0.0;
    }
    (val - min) / range * 2.0 * PI
}

/// Make `angle` continuous relative to `reference` (difference in [-π, π]).
pub fn unwrap_angle(angle: f64, reference: f64) -> f64 {
    let two_pi = 2.0 * PI;
    let mut a = angle;
    while a - reference > PI {
        a -= two_pi;
    }
    while a - reference < -PI {
        a += two_pi;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unwrap_angle() {
        let two_pi = 2.0 * PI;
        assert!((unwrap_angle(0.5, 0.0) - 0.5).abs() < 1e-6);
        assert!((unwrap_angle(0.1, 6.0) - (0.1 + two_pi)).abs() < 1e-6);
        assert!((unwrap_angle(6.0, 0.1) - (6.0 - two_pi)).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_uv() {
        assert!((normalize_uv(0.0, 0.0, 1.0) - 0.0).abs() < 1e-9);
        assert!((normalize_uv(0.5, 0.0, 1.0) - PI).abs() < 1e-9);
        assert!((normalize_uv(1.0, 0.0, 1.0) - 2.0 * PI).abs() < 1e-9);
        assert_eq!(normalize_uv(3.0, 3.0, 3.0), 0.0); // zero range
    }
}
