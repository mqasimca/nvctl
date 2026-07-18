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
        let fan_count = device.fan_count()?;
        if fan_count == 0 {
            return Err(crate::error::NvmlError::FanControlNotAvailable.into());
        }

        if self.dry_run {
            log::info!(
                "DRY RUN: Would set fan speed to {} at {}",
                target_speed,
                temp
            );
            return Ok(target_speed);
        }

        let old_speeds: Vec<_> = (0..fan_count)
            .map(|fan_index| device.fan_speed(fan_index).map(|speed| (fan_index, speed)))
            .collect::<Result<_, _>>()?;
        let mut applied = Vec::new();
        for fan_idx in 0..fan_count {
            if let Err(error) = device.set_fan_speed(fan_idx, target_speed) {
                for (rollback_index, rollback_speed) in applied.into_iter().rev() {
                    if let Err(rollback_error) =
                        device.set_fan_speed(rollback_index, rollback_speed)
                    {
                        log::error!(
                            "Failed to roll back Fan {} speed: {}",
                            rollback_index,
                            rollback_error
                        );
                    }
                }
                return Err(error.into());
            }
            applied.push(old_speeds[fan_idx as usize]);
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
        let fan_count = device.fan_count()?;
        if fan_count == 0 {
            return Err(crate::error::NvmlError::FanControlNotAvailable.into());
        }

        if self.dry_run {
            log::info!("DRY RUN: Would set fan policy to {}", policy);
            return Ok(());
        }

        let old_states: Vec<_> = (0..fan_count)
            .map(|fan_index| {
                Ok((
                    fan_index,
                    device.fan_policy(fan_index)?,
                    device.fan_speed(fan_index)?,
                ))
            })
            .collect::<Result<Vec<_>, crate::error::NvmlError>>()?;
        let mut applied = Vec::new();
        for fan_idx in 0..fan_count {
            if let Err(error) = device.set_fan_policy(fan_idx, policy) {
                for (rollback_index, rollback_policy, rollback_speed) in applied.into_iter().rev() {
                    if let Err(rollback_error) =
                        device.set_fan_policy(rollback_index, rollback_policy)
                    {
                        log::error!(
                            "Failed to roll back Fan {} policy: {}",
                            rollback_index,
                            rollback_error
                        );
                    }
                    if rollback_policy == FanPolicy::Manual {
                        if let Err(rollback_error) =
                            device.set_fan_speed(rollback_index, rollback_speed)
                        {
                            log::error!(
                                "Failed to roll back Fan {} speed: {}",
                                rollback_index,
                                rollback_error
                            );
                        }
                    }
                }
                return Err(error.into());
            }
            applied.push(old_states[fan_idx as usize]);
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
    use crate::mock::MockDevice;

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

    #[test]
    fn applies_curve_and_policy_to_every_fan() {
        let service = FanService::new(FanCurve::default(), false);
        let mut device = MockDevice::new(0);

        let target = service.apply_curve(&mut device).unwrap();
        service.set_policy(&mut device, FanPolicy::Manual).unwrap();

        for fan_index in 0..device.fan_count().unwrap() {
            assert_eq!(device.fan_speed(fan_index).unwrap(), target);
            assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Manual);
        }
    }

    #[test]
    fn fanless_devices_are_rejected_even_in_dry_run() {
        let service = FanService::new(FanCurve::default(), true);
        let mut device = MockDevice::new(0).with_fan_count(0);

        assert!(matches!(
            service.apply_curve(&mut device),
            Err(ServiceError::Nvml(
                crate::error::NvmlError::FanControlNotAvailable
            ))
        ));
        assert!(matches!(
            service.set_policy(&mut device, FanPolicy::Manual),
            Err(ServiceError::Nvml(
                crate::error::NvmlError::FanControlNotAvailable
            ))
        ));
    }
}
