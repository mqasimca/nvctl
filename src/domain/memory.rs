//! Memory domain types including ECC error tracking
//!
//! This module provides validated types for memory-related GPU metrics,
//! including ECC (Error Correcting Code) error tracking which is critical
//! for detecting memory degradation and hardware issues.

use serde::{Deserialize, Serialize};
use std::fmt;

/// ECC error statistics for GPU memory
///
/// Tracks both correctable (single-bit) and uncorrectable (double-bit) ECC errors.
/// Monitoring trends in correctable errors can indicate degrading memory modules.
///
/// # Examples
///
/// ```
/// use nvctl::domain::memory::EccErrors;
///
/// let errors = EccErrors {
///     correctable_current: 5,
///     correctable_lifetime: 142,
///     uncorrectable_current: 0,
///     uncorrectable_lifetime: 0,
/// };
///
/// assert!(!errors.has_uncorrectable());
/// assert_eq!(errors.correctable_rate_per_hour(3600), 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EccErrors {
    /// Correctable errors since last boot
    pub correctable_current: u64,
    /// Correctable errors over GPU lifetime
    pub correctable_lifetime: u64,
    /// Uncorrectable errors since last boot
    pub uncorrectable_current: u64,
    /// Uncorrectable errors over GPU lifetime
    pub uncorrectable_lifetime: u64,
}

impl EccErrors {
    /// Create new ECC error statistics
    pub fn new(
        correctable_current: u64,
        correctable_lifetime: u64,
        uncorrectable_current: u64,
        uncorrectable_lifetime: u64,
    ) -> Self {
        Self {
            correctable_current,
            correctable_lifetime,
            uncorrectable_current,
            uncorrectable_lifetime,
        }
    }

    /// Create zero-initialized ECC errors (no errors detected)
    pub fn zero() -> Self {
        Self {
            correctable_current: 0,
            correctable_lifetime: 0,
            uncorrectable_current: 0,
            uncorrectable_lifetime: 0,
        }
    }

    /// Check if any uncorrectable errors have occurred
    ///
    /// Uncorrectable errors indicate serious hardware issues that may require
    /// GPU replacement.
    pub fn has_uncorrectable(&self) -> bool {
        self.uncorrectable_current > 0 || self.uncorrectable_lifetime > 0
    }

    /// Check if correctable errors exceed recommended threshold
    ///
    /// Industry guideline: >10 correctable errors per hour warrants investigation
    pub fn correctable_exceeds_threshold(&self, uptime_seconds: u64) -> bool {
        if uptime_seconds == 0 {
            return false;
        }
        self.correctable_rate_per_hour(uptime_seconds) > 10
    }

    /// Calculate correctable error rate per hour
    pub fn correctable_rate_per_hour(&self, uptime_seconds: u64) -> u64 {
        if uptime_seconds == 0 {
            return 0;
        }
        let hours = uptime_seconds as f64 / 3600.0;
        (self.correctable_current as f64 / hours).round() as u64
    }

    /// Get health status based on ECC errors
    pub fn health_status(&self, uptime_seconds: u64) -> EccHealthStatus {
        if self.has_uncorrectable() {
            EccHealthStatus::Critical
        } else if self.correctable_exceeds_threshold(uptime_seconds) {
            EccHealthStatus::Warning
        } else if self.correctable_current > 0 {
            EccHealthStatus::Fair
        } else {
            EccHealthStatus::Healthy
        }
    }
}

impl Default for EccErrors {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for EccErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Correctable: {} (lifetime: {}), Uncorrectable: {} (lifetime: {})",
            self.correctable_current,
            self.correctable_lifetime,
            self.uncorrectable_current,
            self.uncorrectable_lifetime
        )
    }
}

/// ECC memory health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EccHealthStatus {
    /// No ECC errors detected
    Healthy,
    /// Some correctable errors, but below threshold
    Fair,
    /// Correctable errors exceed recommended threshold
    Warning,
    /// Uncorrectable errors detected - serious hardware issue
    Critical,
}

impl fmt::Display for EccHealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "Healthy"),
            Self::Fair => write!(f, "Fair"),
            Self::Warning => write!(f, "Warning"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

/// ECC mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EccMode {
    /// ECC is enabled
    Enabled,
    /// ECC is disabled
    Disabled,
}

impl fmt::Display for EccMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Enabled => write!(f, "Enabled"),
            Self::Disabled => write!(f, "Disabled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecc_errors_new() {
        let errors = EccErrors::new(5, 100, 0, 0);
        assert_eq!(errors.correctable_current, 5);
        assert_eq!(errors.correctable_lifetime, 100);
        assert_eq!(errors.uncorrectable_current, 0);
        assert_eq!(errors.uncorrectable_lifetime, 0);
    }

    #[test]
    fn test_ecc_errors_zero() {
        let errors = EccErrors::zero();
        assert_eq!(errors.correctable_current, 0);
        assert_eq!(errors.uncorrectable_current, 0);
        assert!(!errors.has_uncorrectable());
    }

    #[test]
    fn test_has_uncorrectable() {
        let no_errors = EccErrors::zero();
        assert!(!no_errors.has_uncorrectable());

        let current_uncorrectable = EccErrors::new(0, 0, 1, 0);
        assert!(current_uncorrectable.has_uncorrectable());

        let lifetime_uncorrectable = EccErrors::new(0, 0, 0, 1);
        assert!(lifetime_uncorrectable.has_uncorrectable());
    }

    #[test]
    fn test_correctable_rate_per_hour() {
        let errors = EccErrors::new(10, 100, 0, 0);

        // 1 hour uptime: 10 errors/hour
        assert_eq!(errors.correctable_rate_per_hour(3600), 10);

        // 2 hours uptime: 5 errors/hour
        assert_eq!(errors.correctable_rate_per_hour(7200), 5);

        // 30 minutes uptime: 20 errors/hour
        assert_eq!(errors.correctable_rate_per_hour(1800), 20);

        // 0 uptime: 0 errors/hour (avoid division by zero)
        assert_eq!(errors.correctable_rate_per_hour(0), 0);
    }

    #[test]
    fn test_correctable_exceeds_threshold() {
        // 10 errors in 1 hour = exactly 10/hour (at threshold)
        let at_threshold = EccErrors::new(10, 100, 0, 0);
        assert!(!at_threshold.correctable_exceeds_threshold(3600));

        // 11 errors in 1 hour = 11/hour (exceeds threshold)
        let exceeds = EccErrors::new(11, 100, 0, 0);
        assert!(exceeds.correctable_exceeds_threshold(3600));

        // 50 errors in 2 hours = 25/hour (exceeds threshold)
        let high_rate = EccErrors::new(50, 100, 0, 0);
        assert!(high_rate.correctable_exceeds_threshold(7200));
    }

    #[test]
    fn test_health_status() {
        // No errors = Healthy
        let healthy = EccErrors::zero();
        assert_eq!(healthy.health_status(3600), EccHealthStatus::Healthy);

        // Few correctable errors = Fair
        let fair = EccErrors::new(5, 100, 0, 0);
        assert_eq!(fair.health_status(3600), EccHealthStatus::Fair);

        // Many correctable errors = Warning
        let warning = EccErrors::new(50, 100, 0, 0);
        assert_eq!(warning.health_status(3600), EccHealthStatus::Warning);

        // Any uncorrectable errors = Critical
        let critical = EccErrors::new(0, 0, 1, 1);
        assert_eq!(critical.health_status(3600), EccHealthStatus::Critical);
    }

    #[test]
    fn test_ecc_errors_display() {
        let errors = EccErrors::new(5, 100, 0, 2);
        let display = format!("{}", errors);
        assert!(display.contains("Correctable: 5"));
        assert!(display.contains("lifetime: 100"));
        assert!(display.contains("Uncorrectable: 0"));
        assert!(display.contains("lifetime: 2"));
    }

    #[test]
    fn test_ecc_health_status_display() {
        assert_eq!(format!("{}", EccHealthStatus::Healthy), "Healthy");
        assert_eq!(format!("{}", EccHealthStatus::Fair), "Fair");
        assert_eq!(format!("{}", EccHealthStatus::Warning), "Warning");
        assert_eq!(format!("{}", EccHealthStatus::Critical), "Critical");
    }

    #[test]
    fn test_ecc_mode_display() {
        assert_eq!(format!("{}", EccMode::Enabled), "Enabled");
        assert_eq!(format!("{}", EccMode::Disabled), "Disabled");
    }

    #[test]
    fn test_ecc_errors_default() {
        let errors = EccErrors::default();
        assert_eq!(errors, EccErrors::zero());
    }
}
