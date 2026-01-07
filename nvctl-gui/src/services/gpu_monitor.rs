//! GPU monitoring service
//!
//! Handles GPU detection, state polling, and GPU control operations.

#![allow(dead_code)]

use crate::message::GpuStateSnapshot;
use crate::state::GpuState;

use nvctl::domain::{
    ClockSpeed, ClockType, CoolerTarget, FanPolicy, FanSpeed, MemoryInfo, PerformanceState,
    PowerLimit, Temperature, Utilization,
};
use nvctl::error::NvmlError;
use nvctl::nvml::traits::{GpuDevice, GpuManager};
use nvctl::nvml::wrapper::NvmlManager;
use std::time::Instant;

/// GPU monitoring service
pub struct GpuMonitor {
    manager: Option<NvmlManager>,
}

impl GpuMonitor {
    /// Create a new GPU monitor
    pub fn new() -> Self {
        let manager = NvmlManager::new().ok();
        Self { manager }
    }

    /// Check if NVML is available
    pub fn is_available(&self) -> bool {
        self.manager.is_some()
    }

    /// Get all detected GPUs
    pub fn detect_gpus(&self) -> Vec<GpuState> {
        let Some(ref manager) = self.manager else {
            return Vec::new();
        };

        let devices = match manager.all_devices() {
            Ok(devices) => devices,
            Err(_) => return Vec::new(),
        };

        // Get driver version once for all GPUs
        let driver_version = manager.driver_version().ok();

        devices
            .into_iter()
            .filter_map(|device| {
                let mut info = device.info().ok()?;

                // Set driver version on GPU info
                if let Some(ref version) = driver_version {
                    info = info.with_driver_version(version.clone());
                }

                let mut state = GpuState::new(info);

                // Populate initial state
                if let Ok(temp) = device.temperature() {
                    state.temperature = temp;
                }
                if let Ok(thresholds) = device.thermal_thresholds() {
                    state.thresholds = thresholds;
                }
                if let Ok(power_limit) = device.power_limit() {
                    state.power_limit = power_limit;
                }
                if let Ok(power_usage) = device.power_usage() {
                    state.power_usage = power_usage;
                }
                if let Ok(constraints) = device.power_constraints() {
                    state.power_constraints = Some(constraints);
                }

                // Get fan info
                if let Ok(fan_count) = device.fan_count() {
                    for i in 0..fan_count {
                        if let Ok(speed) = device.fan_speed(i) {
                            state.fan_speeds.push(speed);
                        }
                        if let Ok(policy) = device.fan_policy(i) {
                            state.fan_policies.push(policy);
                        }
                    }
                }

                Some(state)
            })
            .collect()
    }

    /// Poll current state for a GPU
    pub fn poll_gpu(&self, index: u32) -> Option<GpuStateSnapshot> {
        let manager = self.manager.as_ref()?;
        let device = manager.device_by_index(index).ok()?;

        let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());
        let temperature = device.temperature().unwrap_or(Temperature::new(0));
        let power_usage = device.power_usage().unwrap_or(PowerLimit::from_watts(0));
        let power_limit = device.power_limit().unwrap_or(PowerLimit::from_watts(0));

        // Get clock speeds
        let gpu_clock = device
            .clock_speed(ClockType::Graphics)
            .unwrap_or(ClockSpeed::default());
        let mem_clock = device
            .clock_speed(ClockType::Memory)
            .unwrap_or(ClockSpeed::default());

        // Get utilization
        let utilization = device.utilization().unwrap_or(Utilization::default());

        // Get memory info
        let memory_info = device.memory_info().unwrap_or(MemoryInfo::default());

        // Get performance state
        let perf_state = device
            .performance_state()
            .unwrap_or(PerformanceState::default());

        let mut fan_speeds = Vec::new();
        let mut fan_policies = Vec::new();

        if let Ok(fan_count) = device.fan_count() {
            for i in 0..fan_count {
                if let Ok(speed) = device.fan_speed(i) {
                    fan_speeds.push(speed);
                }
                if let Ok(policy) = device.fan_policy(i) {
                    fan_policies.push(policy);
                }
            }
        }

        // Get Phase 1 metrics
        let memory_temperature = device.memory_temperature().ok().flatten();
        let ecc_errors = device.ecc_errors().ok().flatten();
        let pcie_metrics = device.pcie_metrics().ok();
        let encoder_util = device.encoder_utilization().ok().flatten();
        let decoder_util = device.decoder_utilization().ok().flatten();

        // Calculate health score
        let health_score = if let Ok(thresholds) = device.thermal_thresholds() {
            let _power_constraints = device.power_constraints().ok();

            // Determine throttling status
            let is_thermal_throttling = if let Some(slowdown) = thresholds.slowdown {
                temperature.as_celsius() >= slowdown.as_celsius()
            } else {
                false
            };

            let is_power_throttling =
                power_usage.as_watts() as f64 >= power_limit.as_watts() as f64 * 0.99;

            // Calculate VRAM usage ratio
            let vram_usage_ratio = if memory_info.total > 0 {
                Some(memory_info.used as f64 / memory_info.total as f64)
            } else {
                None
            };

            // Use uptime estimate of 1 hour for ECC error rate
            let uptime_seconds = 3600;

            // Build health params
            let params = nvctl::health::HealthParams {
                temperature,
                thresholds: &thresholds,
                power_usage,
                power_limit,
                is_thermal_throttling,
                is_power_throttling,
                ecc_errors: ecc_errors.as_ref(),
                vram_usage_ratio,
                utilization: Some(&utilization),
                pcie_metrics: pcie_metrics.as_ref(),
                uptime_seconds,
            };

            // Calculate health using default weights
            let calculator = nvctl::health::HealthCalculator::default();
            let health = calculator.calculate(&params);
            Some(health.overall)
        } else {
            None
        };

        Some(GpuStateSnapshot {
            index,
            name,
            temperature,
            fan_speeds,
            fan_policies,
            power_usage,
            power_limit,
            gpu_clock,
            mem_clock,
            utilization,
            memory_info,
            perf_state,
            memory_temperature,
            ecc_errors,
            pcie_metrics,
            encoder_util,
            decoder_util,
            health_score,
            timestamp: Instant::now(),
        })
    }

    /// Get driver version
    pub fn driver_version(&self) -> Option<String> {
        self.manager.as_ref()?.driver_version().ok()
    }

    /// Get cooler targets for all fans on a GPU
    pub fn get_cooler_targets(&self, index: u32) -> Vec<CoolerTarget> {
        let Some(ref manager) = self.manager else {
            return Vec::new();
        };

        let device = match manager.device_by_index(index) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let fan_count = device.fan_count().unwrap_or(0);
        let mut targets = Vec::with_capacity(fan_count as usize);

        for i in 0..fan_count {
            let target = device.cooler_target(i).unwrap_or(CoolerTarget::All);
            targets.push(target);
        }

        targets
    }

    // === GPU Control Operations ===

    /// Set fan policy for a specific fan on a GPU
    ///
    /// # Arguments
    /// * `gpu_index` - GPU index
    /// * `fan_index` - Fan index on the GPU
    /// * `policy` - Fan policy (Auto or Manual)
    ///
    /// # Returns
    /// Ok(()) on success, Err with description on failure
    pub fn set_fan_policy(
        &self,
        gpu_index: u32,
        fan_index: u32,
        policy: FanPolicy,
    ) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;

        let mut device = manager
            .device_by_index(gpu_index)
            .map_err(|e| format!("Failed to get GPU {}: {}", gpu_index, e))?;

        device
            .set_fan_policy(fan_index, policy)
            .map_err(|e| Self::format_nvml_error(e, "set fan policy"))
    }

    /// Set fan speed for a specific fan on a GPU
    ///
    /// Note: Fan must be in Manual mode for this to work.
    ///
    /// # Arguments
    /// * `gpu_index` - GPU index
    /// * `fan_index` - Fan index on the GPU
    /// * `speed` - Target fan speed (0-100%)
    pub fn set_fan_speed(
        &self,
        gpu_index: u32,
        fan_index: u32,
        speed: FanSpeed,
    ) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;

        let mut device = manager
            .device_by_index(gpu_index)
            .map_err(|e| format!("Failed to get GPU {}: {}", gpu_index, e))?;

        device
            .set_fan_speed(fan_index, speed)
            .map_err(|e| Self::format_nvml_error(e, "set fan speed"))
    }

    /// Set power limit for a GPU
    ///
    /// # Arguments
    /// * `gpu_index` - GPU index
    /// * `limit` - Power limit in watts
    pub fn set_power_limit(&self, gpu_index: u32, limit: PowerLimit) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;

        let mut device = manager
            .device_by_index(gpu_index)
            .map_err(|e| format!("Failed to get GPU {}: {}", gpu_index, e))?;

        device
            .set_power_limit(limit)
            .map_err(|e| Self::format_nvml_error(e, "set power limit"))
    }

    /// Format NVML error into user-friendly message
    fn format_nvml_error(error: NvmlError, operation: &str) -> String {
        match error {
            NvmlError::InsufficientPermissions(_) => {
                format!(
                    "Permission denied to {}. Try running with sudo or add your user to the 'video' group.",
                    operation
                )
            }
            NvmlError::NotSupported(msg) => {
                format!("Operation not supported: {}", msg)
            }
            NvmlError::InvalidArgument(msg) => {
                format!("Invalid argument: {}", msg)
            }
            NvmlError::GpuLost => {
                "GPU connection lost. The GPU may have been reset or removed.".to_string()
            }
            _ => format!("Failed to {}: {}", operation, error),
        }
    }
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}
