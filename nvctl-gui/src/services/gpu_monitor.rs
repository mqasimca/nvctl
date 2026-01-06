//! GPU monitoring service
//!
//! Handles GPU detection, state polling, and GPU control operations.

#![allow(dead_code)]

use crate::message::GpuStateSnapshot;
use crate::state::GpuState;

use nvctl::domain::{CoolerTarget, FanPolicy, FanSpeed, PowerLimit, Temperature};
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

        Some(GpuStateSnapshot {
            index,
            name,
            temperature,
            fan_speeds,
            fan_policies,
            power_usage,
            power_limit,
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
