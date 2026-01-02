//! Power management service
//!
//! Manages GPU power limits.

use crate::domain::PowerLimit;
use crate::error::ServiceError;
use crate::nvml::GpuDevice;

/// Service for managing power limits
pub struct PowerService {
    target_limit: Option<PowerLimit>,
    dry_run: bool,
}

impl PowerService {
    /// Create a new power service
    pub fn new(target_limit: Option<PowerLimit>, dry_run: bool) -> Self {
        Self {
            target_limit,
            dry_run,
        }
    }

    /// Apply the configured power limit to a device
    pub fn apply_limit<D: GpuDevice>(
        &self,
        device: &mut D,
    ) -> Result<Option<PowerLimit>, ServiceError> {
        let Some(limit) = self.target_limit else {
            return Ok(None);
        };

        // Validate against device constraints
        let constraints = device.power_constraints()?;
        limit.validate(&constraints)?;

        if self.dry_run {
            log::info!("DRY RUN: Would set power limit to {}", limit);
            return Ok(Some(limit));
        }

        device.set_power_limit(limit)?;
        log::debug!("Applied power limit {}", limit);

        Ok(Some(limit))
    }

    /// Get the configured target limit
    pub fn target_limit(&self) -> Option<PowerLimit> {
        self.target_limit
    }

    /// Check if in dry-run mode
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_service_creation() {
        let service = PowerService::new(Some(PowerLimit::from_watts(300)), false);
        assert_eq!(service.target_limit().map(|l| l.as_watts()), Some(300));
        assert!(!service.is_dry_run());
    }

    #[test]
    fn test_power_service_no_limit() {
        let service = PowerService::new(None, false);
        assert!(service.target_limit().is_none());
    }
}
