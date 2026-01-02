//! Control loop monitor
//!
//! Orchestrates the control loop, applying services at regular intervals.

use crate::domain::{FanCurve, PowerLimit};
use crate::error::AppError;
use crate::nvml::{GpuDevice, GpuManager};
use crate::services::{FanService, PowerService};

use std::time::Duration;

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
        }
    }

    /// Execute a single control tick on a device
    pub fn tick<D: GpuDevice>(&self, device: &mut D) -> Result<(), AppError> {
        // Apply fan curve
        self.fan_service.apply_curve(device)?;

        // Apply power limit if configured
        self.power_service.apply_limit(device)?;

        Ok(())
    }

    /// Run the control loop
    pub fn run<M: GpuManager>(&self, manager: &M, gpu_indices: &[u32]) -> Result<(), AppError> {
        loop {
            match self.run_tick(manager, gpu_indices) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Control tick failed: {}", e);
                    if self.config.retry {
                        log::info!("Retrying in {:?}...", self.config.retry_interval);
                        std::thread::sleep(self.config.retry_interval);
                        continue;
                    }
                    return Err(e);
                }
            }

            if self.config.single_use {
                log::info!("Single-use mode: exiting after one tick");
                break;
            }

            std::thread::sleep(self.config.interval);
        }

        Ok(())
    }

    fn run_tick<M: GpuManager>(&self, manager: &M, gpu_indices: &[u32]) -> Result<(), AppError> {
        for &idx in gpu_indices {
            let mut device = manager.device_by_index(idx)?;
            self.tick(&mut device)?;
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
}
