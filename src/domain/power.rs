//! Power domain types
//!
//! Provides validated types for power limits and constraints.

use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Power limit in milliwatts (stored internally) but displayed as watts
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PowerLimit(u32);

impl PowerLimit {
    /// Largest whole-watt value representable as milliwatts in a `u32`.
    pub const MAX_WATTS: u32 = u32::MAX / 1000;

    /// Create a new power limit from watts, saturating if the milliwatt value
    /// cannot fit in the underlying representation.
    ///
    /// Use [`PowerLimit::try_from_watts`] for untrusted input.
    pub const fn from_watts(watts: u32) -> Self {
        Self(watts.saturating_mul(1000))
    }

    /// Create a power limit from watts, rejecting values that overflow the
    /// milliwatt representation used by NVML.
    pub fn try_from_watts(watts: u32) -> Result<Self, DomainError> {
        watts.checked_mul(1000).map(Self).ok_or_else(|| {
            DomainError::InvalidValue(format!(
                "power limit {watts}W exceeds the representable maximum of {}W",
                Self::MAX_WATTS
            ))
        })
    }

    /// Create a new power limit from milliwatts
    pub const fn from_milliwatts(mw: u32) -> Self {
        Self(mw)
    }

    /// Get the power limit in watts
    #[inline]
    pub const fn as_watts(&self) -> u32 {
        self.0 / 1000
    }

    /// Get the power limit in milliwatts
    #[inline]
    pub const fn as_milliwatts(&self) -> u32 {
        self.0
    }

    /// Validate this power limit against constraints
    pub fn validate(&self, constraints: &PowerConstraints) -> Result<(), DomainError> {
        if !constraints.contains(self) {
            return Err(DomainError::InvalidPowerLimit {
                value: self.as_watts(),
                min: constraints.min.as_watts(),
                max: constraints.max.as_watts(),
            });
        }
        Ok(())
    }
}

impl fmt::Display for PowerLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}W", self.as_watts())
    }
}

/// Power constraints from GPU (min/max limits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerConstraints {
    /// Minimum power limit
    pub min: PowerLimit,
    /// Maximum power limit
    pub max: PowerLimit,
    /// Default power limit
    pub default: PowerLimit,
}

impl PowerConstraints {
    /// Create new power constraints
    pub fn new(min: PowerLimit, max: PowerLimit, default: PowerLimit) -> Self {
        Self { min, max, default }
    }

    /// Check if a power limit is within constraints
    pub fn contains(&self, limit: &PowerLimit) -> bool {
        limit.0 >= self.min.0 && limit.0 <= self.max.0
    }
}

impl fmt::Display for PowerConstraints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{} (default: {})", self.min, self.max, self.default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_limit_from_watts() {
        let limit = PowerLimit::from_watts(300);
        assert_eq!(limit.as_watts(), 300);
        assert_eq!(limit.as_milliwatts(), 300_000);
    }

    #[test]
    fn oversized_watt_values_do_not_wrap() {
        assert!(PowerLimit::try_from_watts(PowerLimit::MAX_WATTS).is_ok());
        assert!(PowerLimit::try_from_watts(PowerLimit::MAX_WATTS + 1).is_err());
        assert_eq!(PowerLimit::from_watts(u32::MAX).as_milliwatts(), u32::MAX);
    }

    #[test]
    fn test_power_limit_display() {
        let limit = PowerLimit::from_watts(350);
        assert_eq!(limit.to_string(), "350W");
    }

    #[test]
    fn test_power_constraints_contains() {
        let constraints = PowerConstraints::new(
            PowerLimit::from_watts(100),
            PowerLimit::from_watts(400),
            PowerLimit::from_watts(300),
        );

        assert!(constraints.contains(&PowerLimit::from_watts(200)));
        assert!(constraints.contains(&PowerLimit::from_watts(100)));
        assert!(constraints.contains(&PowerLimit::from_watts(400)));
        assert!(!constraints.contains(&PowerLimit::from_watts(50)));
        assert!(!constraints.contains(&PowerLimit::from_watts(500)));
    }

    #[test]
    fn test_power_limit_validation() {
        let constraints = PowerConstraints::new(
            PowerLimit::from_watts(100),
            PowerLimit::from_watts(400),
            PowerLimit::from_watts(300),
        );

        let valid = PowerLimit::from_watts(250);
        assert!(valid.validate(&constraints).is_ok());

        let too_low = PowerLimit::from_watts(50);
        assert!(too_low.validate(&constraints).is_err());

        let too_high = PowerLimit::from_watts(500);
        assert!(too_high.validate(&constraints).is_err());
    }

    #[test]
    fn validation_uses_exact_milliwatts() {
        let constraints = PowerConstraints::new(
            PowerLimit::from_milliwatts(100_500),
            PowerLimit::from_milliwatts(100_500),
            PowerLimit::from_milliwatts(100_500),
        );
        assert!(PowerLimit::from_milliwatts(100_999)
            .validate(&constraints)
            .is_err());
    }
}
