//! Fan control service
//!
//! Applies fan curves based on GPU temperature.

use crate::domain::{FanCurve, FanPolicy, FanSpeed};
use crate::error::ServiceError;
use crate::nvml::GpuDevice;

/// Service for managing fan speed based on temperature
pub struct FanService {
    curve: FanCurve,
    dry_run: bool,
}

impl FanService {
    /// Create a new fan service
    pub fn new(curve: FanCurve, dry_run: bool) -> Self {
        Self { curve, dry_run }
    }

    /// Apply the fan curve to a device
    pub fn apply_curve<D: GpuDevice>(&self, device: &mut D) -> Result<FanSpeed, ServiceError> {
        let temp = device.temperature()?;
        let target_speed = self.curve.speed_for_temperature(temp.as_celsius());

        if self.dry_run {
            log::info!(
                "DRY RUN: Would set fan speed to {} at {}",
                target_speed,
                temp
            );
            return Ok(target_speed);
        }

        let fan_count = device.fan_count()?;
        for fan_idx in 0..fan_count {
            device.set_fan_speed(fan_idx, target_speed)?;
        }

        log::debug!("Applied fan speed {} at {}", target_speed, temp);
        Ok(target_speed)
    }

    /// Set fan policy on all fans
    pub fn set_policy<D: GpuDevice>(
        &self,
        device: &mut D,
        policy: FanPolicy,
    ) -> Result<(), ServiceError> {
        if self.dry_run {
            log::info!("DRY RUN: Would set fan policy to {}", policy);
            return Ok(());
        }

        let fan_count = device.fan_count()?;
        for fan_idx in 0..fan_count {
            device.set_fan_policy(fan_idx, policy)?;
        }

        log::debug!("Set fan policy to {}", policy);
        Ok(())
    }

    /// Get the configured fan curve
    pub fn curve(&self) -> &FanCurve {
        &self.curve
    }

    /// Check if in dry-run mode
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FanCurve;

    #[test]
    fn test_fan_service_creation() {
        let curve = FanCurve::default();
        let service = FanService::new(curve, false);
        assert!(!service.is_dry_run());
    }

    #[test]
    fn test_fan_service_dry_run() {
        let curve = FanCurve::default();
        let service = FanService::new(curve, true);
        assert!(service.is_dry_run());
    }
}
