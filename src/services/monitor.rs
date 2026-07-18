//! Control loop monitor
//!
//! Orchestrates the control loop, applying services at regular intervals.

use crate::domain::{FanCurve, PowerLimit};
use crate::error::{AppError, DomainError};
use crate::nvml::{GpuDevice, GpuManager};
use crate::services::{AlertService, FanService, PowerService};

use std::time::Duration;

#[derive(Debug, Clone)]
struct OriginalFanState {
    gpu_uuid: String,
    fan_index: u32,
    policy: crate::domain::FanPolicy,
    speed: crate::domain::FanSpeed,
}

/// Configuration for the monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Interval between control ticks
    pub interval: Duration,
    /// Whether to exit after one tick
    pub single_use: bool,
    /// Whether to retry on errors
    pub retry: bool,
    /// Interval between retries
    pub retry_interval: Duration,
    /// Fan curve configuration
    pub fan_curve: FanCurve,
    /// Optional power limit
    pub power_limit: Option<PowerLimit>,
    /// Dry run mode
    pub dry_run: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            single_use: false,
            retry: true,
            retry_interval: Duration::from_secs(10),
            fan_curve: FanCurve::default(),
            power_limit: None,
            dry_run: false,
        }
    }
}

/// Control loop monitor
pub struct Monitor {
    config: MonitorConfig,
    fan_service: FanService,
    power_service: PowerService,
    alert_service: Option<AlertService>,
}

impl Monitor {
    /// Create a new monitor with the given configuration
    pub fn new(config: MonitorConfig) -> Self {
        let fan_service = FanService::new(config.fan_curve.clone(), config.dry_run);
        let power_service = PowerService::new(config.power_limit, config.dry_run);

        Self {
            config,
            fan_service,
            power_service,
            alert_service: None,
        }
    }

    /// Set the alert service for this monitor
    pub fn with_alert_service(mut self, alert_service: AlertService) -> Self {
        self.alert_service = Some(alert_service);
        self
    }

    /// Execute a single control tick on a device
    pub fn tick<D: GpuDevice>(&mut self, device: &mut D, gpu_index: u32) -> Result<(), AppError> {
        let original_fans: Vec<_> = (0..device.fan_count()?)
            .map(|fan_index| {
                Ok((
                    fan_index,
                    device.fan_policy(fan_index)?,
                    device.fan_speed(fan_index)?,
                ))
            })
            .collect::<Result<Vec<_>, crate::error::NvmlError>>()?;
        let original_power_limit =
            if self.config.dry_run || self.power_service.target_limit().is_none() {
                None
            } else {
                Some(device.power_limit()?)
            };
        self.fan_service
            .set_policy(device, crate::domain::FanPolicy::Manual)?;
        let result = (|| {
            self.fan_service.apply_curve(device)?;
            self.power_service.apply_limit(device)?;
            if let Some(alert_service) = &mut self.alert_service {
                alert_service.evaluate(device, gpu_index)?;
            }
            Ok(())
        })();
        if result.is_err() {
            for (fan_index, policy, speed) in original_fans.into_iter().rev() {
                if let Err(error) = device.set_fan_policy(fan_index, policy) {
                    log::error!("Failed to restore Fan {} policy: {}", fan_index, error);
                }
                if policy == crate::domain::FanPolicy::Manual {
                    if let Err(error) = device.set_fan_speed(fan_index, speed) {
                        log::error!("Failed to restore Fan {} speed: {}", fan_index, error);
                    }
                }
            }
            if let Some(limit) = original_power_limit {
                if let Err(error) = device.set_power_limit(limit) {
                    log::error!("Failed to restore power limit: {}", error);
                }
            }
        }
        result
    }

    /// Run the control loop
    pub fn run<M: GpuManager>(&mut self, manager: &M, gpu_indices: &[u32]) -> Result<(), AppError> {
        if self.config.interval.is_zero() || self.config.retry_interval.is_zero() {
            return Err(DomainError::InvalidValue(
                "monitor and retry intervals must be greater than zero".to_string(),
            )
            .into());
        }
        if gpu_indices.is_empty() {
            return Err(AppError::NoGpusFound);
        }
        let gpu_uuids: Vec<String> = gpu_indices
            .iter()
            .map(|index| {
                manager
                    .device_by_index(*index)
                    .and_then(|device| device.uuid())
            })
            .collect::<Result<_, _>>()?;
        let original_fans = self.capture_fan_states(manager, &gpu_uuids)?;

        loop {
            match self.run_tick(manager, &gpu_uuids) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Control tick failed: {}", e);
                    if self.config.retry {
                        self.restore_fan_states(manager, &original_fans)?;
                        log::info!("Retrying in {:?}...", self.config.retry_interval);
                        std::thread::sleep(self.config.retry_interval);
                        continue;
                    }
                    self.restore_fan_states(manager, &original_fans)?;
                    return Err(e);
                }
            }

            if self.config.single_use {
                log::info!("Single-use mode: exiting after one tick");
                self.restore_fan_states(manager, &original_fans)?;
                break;
            }

            std::thread::sleep(self.config.interval);
        }

        Ok(())
    }

    fn capture_fan_states<M: GpuManager>(
        &self,
        manager: &M,
        gpu_uuids: &[String],
    ) -> Result<Vec<OriginalFanState>, AppError> {
        let mut originals = Vec::new();
        for gpu_uuid in gpu_uuids {
            let device = manager.device_by_uuid(gpu_uuid)?;
            for fan_index in 0..device.fan_count()? {
                originals.push(OriginalFanState {
                    gpu_uuid: gpu_uuid.clone(),
                    fan_index,
                    policy: device.fan_policy(fan_index)?,
                    speed: device.fan_speed(fan_index)?,
                });
            }
        }
        Ok(originals)
    }

    fn restore_fan_states<M: GpuManager>(
        &self,
        manager: &M,
        originals: &[OriginalFanState],
    ) -> Result<(), AppError> {
        let mut first_error = None;
        for original in originals.iter().rev() {
            match manager.device_by_uuid(&original.gpu_uuid) {
                Ok(mut device) => {
                    let result: Result<(), crate::error::NvmlError> = (|| {
                        device.set_fan_policy(original.fan_index, original.policy)?;
                        if original.policy == crate::domain::FanPolicy::Manual {
                            device.set_fan_speed(original.fan_index, original.speed)?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        log::error!(
                            "Failed to restore GPU {} Fan {}: {}",
                            original.gpu_uuid,
                            original.fan_index,
                            error
                        );
                        first_error.get_or_insert(AppError::from(error));
                    }
                }
                Err(error) => {
                    log::error!(
                        "Failed to open GPU {} for fan restore: {}",
                        original.gpu_uuid,
                        error
                    );
                    first_error.get_or_insert(AppError::from(error));
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn run_tick<M: GpuManager>(
        &mut self,
        manager: &M,
        gpu_uuids: &[String],
    ) -> Result<(), AppError> {
        let original_power_limits =
            if self.config.dry_run || self.power_service.target_limit().is_none() {
                Vec::new()
            } else {
                gpu_uuids
                    .iter()
                    .map(|gpu_uuid| {
                        manager
                            .device_by_uuid(gpu_uuid)
                            .and_then(|device| device.power_limit())
                            .map(|limit| (gpu_uuid.clone(), limit))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };

        for gpu_uuid in gpu_uuids {
            let result = manager
                .device_by_uuid(gpu_uuid)
                .map_err(AppError::from)
                .and_then(|mut device| {
                    let gpu_index = device.index();
                    self.tick(&mut device, gpu_index)
                });
            if let Err(error) = result {
                for (rollback_uuid, rollback_limit) in original_power_limits.iter().rev() {
                    let rollback = manager
                        .device_by_uuid(rollback_uuid)
                        .and_then(|mut device| device.set_power_limit(*rollback_limit));
                    if let Err(rollback_error) = rollback {
                        log::error!(
                            "Failed to roll back GPU {} power limit: {}",
                            rollback_uuid,
                            rollback_error
                        );
                    }
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Get the monitor configuration
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::{AlertManagerConfig, AlertRule, AlertSeverity, Condition, MetricType};
    use crate::domain::FanPolicy;
    use crate::mock::{MockDevice, MockManager};
    use crate::nvml::{GpuDevice, GpuManager};

    #[test]
    fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.interval, Duration::from_secs(5));
        assert!(!config.single_use);
        assert!(config.retry);
    }

    #[test]
    fn test_monitor_creation() {
        let config = MonitorConfig::default();
        let monitor = Monitor::new(config);
        assert!(!monitor.config().dry_run);
    }

    #[test]
    fn single_use_restores_original_fan_state() {
        let manager = MockManager::new(1);
        let mut monitor = Monitor::new(MonitorConfig {
            single_use: true,
            power_limit: Some(PowerLimit::from_watts(250)),
            ..MonitorConfig::default()
        });

        monitor.run(&manager, &[0]).unwrap();

        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.power_limit().unwrap(), PowerLimit::from_watts(250));
        for fan_index in 0..device.fan_count().unwrap() {
            assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Auto);
        }
    }

    #[test]
    fn single_use_preserves_preexisting_manual_fan_state() {
        let manager = MockManager::new(1);
        let mut device = manager.device_by_index(0).unwrap();
        let original_speed = crate::domain::FanSpeed::new(42).unwrap();
        device.set_fan_policy(0, FanPolicy::Manual).unwrap();
        device.set_fan_speed(0, original_speed).unwrap();
        drop(device);

        let mut monitor = Monitor::new(MonitorConfig {
            single_use: true,
            ..MonitorConfig::default()
        });
        monitor.run(&manager, &[0]).unwrap();

        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Manual);
        assert_eq!(device.fan_speed(0).unwrap(), original_speed);
    }

    #[test]
    fn invalid_run_configuration_is_rejected_before_polling() {
        let manager = MockManager::new(1);
        let mut monitor = Monitor::new(MonitorConfig {
            interval: Duration::ZERO,
            ..MonitorConfig::default()
        });
        assert!(monitor.run(&manager, &[0]).is_err());

        let mut monitor = Monitor::new(MonitorConfig {
            retry_interval: Duration::ZERO,
            ..MonitorConfig::default()
        });
        assert!(monitor.run(&manager, &[0]).is_err());
    }

    #[test]
    fn tick_rolls_back_power_and_fans_when_alert_evaluation_fails() {
        let alert_service = AlertService::new(
            AlertManagerConfig::default(),
            vec![AlertRule::new(
                "temperature".to_string(),
                "Temperature".to_string(),
                MetricType::Temperature,
                Condition::GreaterThan(80.0),
                AlertSeverity::Warning,
            )],
        );
        let mut monitor = Monitor::new(MonitorConfig {
            power_limit: Some(PowerLimit::from_watts(250)),
            ..MonitorConfig::default()
        })
        .with_alert_service(alert_service);
        let mut device = MockDevice::new(0);
        device.fail_uuid_reads(true);

        assert!(monitor.tick(&mut device, 0).is_err());
        assert_eq!(device.power_limit().unwrap(), PowerLimit::from_watts(300));
        for fan_index in 0..device.fan_count().unwrap() {
            assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Auto);
        }
    }
}
