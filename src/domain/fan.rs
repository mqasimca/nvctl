//! Fan-related domain types
//!
//! Provides validated types for fan speed, curves, and policies.

use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Fan speed percentage (0-100)
///
/// Validated on construction to ensure the value is within valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct FanSpeed(u8);

impl FanSpeed {
    /// Minimum valid fan speed
    pub const MIN: u8 = 0;
    /// Maximum valid fan speed
    pub const MAX: u8 = 100;

    /// Create a new FanSpeed with validation
    ///
    /// # Errors
    /// Returns `DomainError::InvalidFanSpeed` if value > 100
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if value > Self::MAX {
            return Err(DomainError::InvalidFanSpeed(value));
        }
        Ok(Self(value))
    }

    /// Create a FanSpeed without validation (for internal use)
    ///
    /// # Safety
    /// Caller must ensure value <= 100
    pub(crate) const fn new_unchecked(value: u8) -> Self {
        Self(value)
    }

    /// Get the speed as a percentage value (0-100)
    #[inline]
    pub const fn as_percentage(&self) -> u8 {
        self.0
    }

    /// Get the speed as a fraction (0.0-1.0)
    #[inline]
    pub fn as_fraction(&self) -> f32 {
        self.0 as f32 / 100.0
    }
}

impl fmt::Display for FanSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

impl TryFrom<u8> for FanSpeed {
    type Error = DomainError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<FanSpeed> for u8 {
    fn from(speed: FanSpeed) -> Self {
        speed.0
    }
}

impl From<FanSpeed> for u32 {
    fn from(speed: FanSpeed) -> Self {
        speed.0 as u32
    }
}

/// A single point on a fan curve
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurvePoint {
    /// Temperature threshold in Celsius
    pub temperature: i32,
    /// Target fan speed at this temperature
    pub speed: FanSpeed,
}

impl FanCurvePoint {
    /// Create a new fan curve point
    pub fn new(temperature: i32, speed: FanSpeed) -> Self {
        Self { temperature, speed }
    }
}

/// A fan curve defining speed based on temperature
///
/// Points are sorted by temperature in ascending order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanCurve {
    /// Curve points sorted by temperature
    points: Vec<FanCurvePoint>,
    /// Default speed when temperature is below the first point
    default_speed: FanSpeed,
}

impl FanCurve {
    /// Create a new fan curve from points
    ///
    /// Points will be sorted by temperature. Returns error if empty.
    ///
    /// # Errors
    /// Returns `DomainError::EmptyFanCurve` if points is empty
    pub fn new(
        mut points: Vec<FanCurvePoint>,
        default_speed: FanSpeed,
    ) -> Result<Self, DomainError> {
        if points.is_empty() {
            return Err(DomainError::EmptyFanCurve);
        }

        // Sort by temperature ascending
        points.sort_by_key(|p| p.temperature);

        Ok(Self {
            points,
            default_speed,
        })
    }

    /// Get the target fan speed for a given temperature
    ///
    /// Uses step-based lookup (no interpolation):
    /// - Below first point: returns default_speed
    /// - At or above a point: returns that point's speed
    pub fn speed_for_temperature(&self, temp: i32) -> FanSpeed {
        // Find the highest point that is <= temp
        let mut result = self.default_speed;

        for point in &self.points {
            if temp >= point.temperature {
                result = point.speed;
            } else {
                break;
            }
        }

        result
    }

    /// Get the curve points
    pub fn points(&self) -> &[FanCurvePoint] {
        &self.points
    }

    /// Get the default speed
    pub fn default_speed(&self) -> FanSpeed {
        self.default_speed
    }

    /// Create a default fan curve
    pub fn default_curve() -> Self {
        Self {
            points: vec![
                FanCurvePoint::new(40, FanSpeed::new_unchecked(30)),
                FanCurvePoint::new(60, FanSpeed::new_unchecked(50)),
                FanCurvePoint::new(75, FanSpeed::new_unchecked(80)),
                FanCurvePoint::new(85, FanSpeed::new_unchecked(100)),
            ],
            default_speed: FanSpeed::new_unchecked(30),
        }
    }
}

impl Default for FanCurve {
    fn default() -> Self {
        Self::default_curve()
    }
}

/// Fan control policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum FanPolicy {
    /// GPU controls fan speed automatically
    #[default]
    Auto,
    /// Manual fan speed control
    Manual,
}

impl fmt::Display for FanPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FanPolicy::Auto => write!(f, "Auto"),
            FanPolicy::Manual => write!(f, "Manual"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_speed_valid() {
        assert!(FanSpeed::new(0).is_ok());
        assert!(FanSpeed::new(50).is_ok());
        assert!(FanSpeed::new(100).is_ok());
    }

    #[test]
    fn test_fan_speed_invalid() {
        assert!(FanSpeed::new(101).is_err());
        assert!(FanSpeed::new(255).is_err());
    }

    #[test]
    fn test_fan_speed_display() {
        let speed = FanSpeed::new(75).unwrap();
        assert_eq!(speed.to_string(), "75%");
    }

    #[test]
    fn test_fan_speed_as_fraction() {
        let speed = FanSpeed::new(50).unwrap();
        assert!((speed.as_fraction() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fan_curve_empty() {
        let result = FanCurve::new(vec![], FanSpeed::new_unchecked(30));
        assert!(matches!(result, Err(DomainError::EmptyFanCurve)));
    }

    #[test]
    fn test_fan_curve_sorting() {
        let points = vec![
            FanCurvePoint::new(80, FanSpeed::new_unchecked(100)),
            FanCurvePoint::new(40, FanSpeed::new_unchecked(30)),
            FanCurvePoint::new(60, FanSpeed::new_unchecked(50)),
        ];

        let curve = FanCurve::new(points, FanSpeed::new_unchecked(20)).unwrap();
        let temps: Vec<_> = curve.points().iter().map(|p| p.temperature).collect();
        assert_eq!(temps, vec![40, 60, 80]);
    }

    #[test]
    fn test_fan_curve_speed_lookup() {
        let curve = FanCurve::default_curve();

        // Below first point - returns default
        assert_eq!(curve.speed_for_temperature(30).as_percentage(), 30);

        // At first point
        assert_eq!(curve.speed_for_temperature(40).as_percentage(), 30);

        // Between points - returns lower point's speed
        assert_eq!(curve.speed_for_temperature(50).as_percentage(), 30);
        assert_eq!(curve.speed_for_temperature(70).as_percentage(), 50);

        // At exact points
        assert_eq!(curve.speed_for_temperature(60).as_percentage(), 50);
        assert_eq!(curve.speed_for_temperature(75).as_percentage(), 80);
        assert_eq!(curve.speed_for_temperature(85).as_percentage(), 100);

        // Above last point
        assert_eq!(curve.speed_for_temperature(95).as_percentage(), 100);
    }

    #[test]
    fn test_fan_policy_display() {
        assert_eq!(FanPolicy::Auto.to_string(), "Auto");
        assert_eq!(FanPolicy::Manual.to_string(), "Manual");
    }
}
