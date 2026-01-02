//! Thermal domain types
//!
//! Provides validated types for temperature and thermal thresholds.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Temperature in degrees Celsius
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Temperature(i32);

impl Temperature {
    /// Create a new Temperature
    pub const fn new(celsius: i32) -> Self {
        Self(celsius)
    }

    /// Get the temperature in Celsius
    #[inline]
    pub const fn as_celsius(&self) -> i32 {
        self.0
    }

    /// Check if temperature is critical (above 90°C typically)
    pub fn is_critical(&self) -> bool {
        self.0 >= 90
    }

    /// Check if temperature is high (above 80°C typically)
    pub fn is_high(&self) -> bool {
        self.0 >= 80
    }
}

impl fmt::Display for Temperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}°C", self.0)
    }
}

impl From<i32> for Temperature {
    fn from(value: i32) -> Self {
        Self::new(value)
    }
}

impl From<u32> for Temperature {
    fn from(value: u32) -> Self {
        Self::new(value as i32)
    }
}

impl From<Temperature> for i32 {
    fn from(temp: Temperature) -> Self {
        temp.0
    }
}

/// GPU thermal thresholds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ThermalThresholds {
    /// Temperature at which GPU will shut down
    pub shutdown: Option<Temperature>,
    /// Temperature at which GPU will throttle
    pub slowdown: Option<Temperature>,
    /// Temperature target for GPU boost
    pub gpu_max: Option<Temperature>,
}

impl ThermalThresholds {
    /// Create new thermal thresholds
    pub fn new(
        shutdown: Option<Temperature>,
        slowdown: Option<Temperature>,
        gpu_max: Option<Temperature>,
    ) -> Self {
        Self {
            shutdown,
            slowdown,
            gpu_max,
        }
    }
}

/// Acoustic temperature limit constraints
///
/// The acoustic limit tells the GPU to throttle to maintain a target temperature.
/// This is the same setting used by GeForce Experience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AcousticLimits {
    /// Current acoustic temperature limit
    pub current: Option<Temperature>,
    /// Minimum allowed acoustic limit
    pub min: Option<Temperature>,
    /// Maximum allowed acoustic limit
    pub max: Option<Temperature>,
}

impl AcousticLimits {
    /// Create new acoustic limits
    pub fn new(
        current: Option<Temperature>,
        min: Option<Temperature>,
        max: Option<Temperature>,
    ) -> Self {
        Self { current, min, max }
    }

    /// Check if a temperature is within the valid range
    pub fn is_valid(&self, temp: Temperature) -> bool {
        match (self.min, self.max) {
            (Some(min), Some(max)) => temp >= min && temp <= max,
            (Some(min), None) => temp >= min,
            (None, Some(max)) => temp <= max,
            (None, None) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_display() {
        let temp = Temperature::new(65);
        assert_eq!(temp.to_string(), "65°C");
    }

    #[test]
    fn test_temperature_comparisons() {
        let t1 = Temperature::new(50);
        let t2 = Temperature::new(75);
        assert!(t1 < t2);
    }

    #[test]
    fn test_temperature_thresholds() {
        let temp = Temperature::new(85);
        assert!(temp.is_high());
        assert!(!temp.is_critical());

        let critical = Temperature::new(95);
        assert!(critical.is_critical());
        assert!(critical.is_high());
    }

    #[test]
    fn test_temperature_from_u32() {
        let temp: Temperature = 65u32.into();
        assert_eq!(temp.as_celsius(), 65);
    }

    #[test]
    fn test_acoustic_limits_valid_range() {
        let limits = AcousticLimits::new(
            Some(Temperature::new(75)),
            Some(Temperature::new(60)),
            Some(Temperature::new(90)),
        );

        assert!(limits.is_valid(Temperature::new(75)));
        assert!(limits.is_valid(Temperature::new(60)));
        assert!(limits.is_valid(Temperature::new(90)));
        assert!(!limits.is_valid(Temperature::new(59)));
        assert!(!limits.is_valid(Temperature::new(91)));
    }

    #[test]
    fn test_acoustic_limits_unbounded() {
        let limits = AcousticLimits::new(None, None, None);
        assert!(limits.is_valid(Temperature::new(100)));
        assert!(limits.is_valid(Temperature::new(0)));
    }
}
