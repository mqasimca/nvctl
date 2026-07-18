//! GPU monitoring service
//!
//! Handles GPU detection, state polling, and GPU control operations.

#![allow(dead_code)]

use crate::message::GpuStateSnapshot;
use crate::state::GpuState;

use nvctl::domain::{
    AcousticLimits, ClockType, CoolerTarget, FanPolicy, FanSpeed, PowerLimit, Temperature,
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

        let device_count = match manager.device_count() {
            Ok(count) => count,
            Err(error) => {
                log::warn!("Failed to enumerate GPUs: {}", error);
                return Vec::new();
            }
        };

        // Get driver version once for all GPUs
        let driver_version = manager.driver_version().ok();

        (0..device_count)
            .filter_map(|index| {
                let device = match manager.device_by_index(index) {
                    Ok(device) => device,
                    Err(error) => {
                        log::warn!("Skipping GPU {index}: failed to open device: {error}");
                        return None;
                    }
                };
                let state = (|| -> Result<GpuState, String> {
                    let mut info = device.info().map_err(|error| error.to_string())?;
                    if let Some(ref version) = driver_version {
                        info = info.with_driver_version(version.clone());
                    }

                    let mut state = GpuState::new(info);
                    state.thresholds = device
                        .thermal_thresholds()
                        .map_err(|error| error.to_string())?;
                    state.power_constraints = match device.power_constraints() {
                        Ok(constraints) => Some(constraints),
                        Err(NvmlError::NotSupported(_)) => None,
                        Err(error) => return Err(error.to_string()),
                    };
                    let snapshot = self.poll_device(&device)?;
                    state.update_from_snapshot(snapshot);

                    Ok(state)
                })();

                match state {
                    Ok(state) => Some(state),
                    Err(error) => {
                        log::warn!("Skipping GPU with incomplete initial telemetry: {}", error);
                        None
                    }
                }
            })
            .collect()
    }

    /// Poll current state for a GPU
    pub fn poll_gpu(&self, index: u32) -> Result<GpuStateSnapshot, String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_index(index)
            .map_err(|error| format!("Failed to open GPU {index}: {error}"))?;

        self.poll_device(&device)
    }

    /// Poll a physical GPU by its stable UUID.
    pub fn poll_gpu_by_uuid(&self, gpu_uuid: &str) -> Result<GpuStateSnapshot, String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to open GPU {gpu_uuid}: {error}"))?;

        self.poll_device(&device)
    }

    fn poll_device<D: GpuDevice>(&self, device: &D) -> Result<GpuStateSnapshot, String> {
        let index = device.index();

        let metric_error = |metric: &str, error: NvmlError| {
            format!("Failed to read {metric} for GPU {index}: {error}")
        };
        let name = device.name().map_err(|error| metric_error("name", error))?;
        let temperature = device
            .temperature()
            .map_err(|error| metric_error("temperature", error))?;
        let power_usage = device
            .power_usage()
            .map_err(|error| metric_error("power usage", error))?;
        let power_limit = device
            .power_limit()
            .map_err(|error| metric_error("power limit", error))?;
        let gpu_clock = device
            .clock_speed(ClockType::Graphics)
            .map_err(|error| metric_error("graphics clock", error))?;
        let mem_clock = device
            .clock_speed(ClockType::Memory)
            .map_err(|error| metric_error("memory clock", error))?;
        let utilization = device
            .utilization()
            .map_err(|error| metric_error("utilization", error))?;
        let memory_info = device
            .memory_info()
            .map_err(|error| metric_error("memory usage", error))?;
        let perf_state = device
            .performance_state()
            .map_err(|error| metric_error("performance state", error))?;

        let mut fan_speeds = Vec::new();
        let mut fan_policies = Vec::new();

        let fan_count = device
            .fan_count()
            .map_err(|error| metric_error("fan count", error))?;
        for i in 0..fan_count {
            fan_speeds.push(
                device
                    .fan_speed(i)
                    .map_err(|error| metric_error(&format!("fan {i} speed"), error))?,
            );
            fan_policies.push(match device.fan_policy(i) {
                Ok(policy) => Some(policy),
                Err(NvmlError::NotSupported(_)) => None,
                Err(error) => return Err(metric_error(&format!("fan {i} policy"), error)),
            });
        }

        // Unsupported optional metrics remain absent; operational failures abort the snapshot.
        let memory_temperature =
            Self::optional_metric(device.memory_temperature(), "memory temperature", index)?
                .flatten();
        let ecc_errors = Self::optional_metric(device.ecc_errors(), "ECC errors", index)?.flatten();
        let pcie_metrics = Self::optional_metric(device.pcie_metrics(), "PCIe metrics", index)?;
        let encoder_util =
            Self::optional_metric(device.encoder_utilization(), "encoder utilization", index)?
                .flatten();
        let decoder_util =
            Self::optional_metric(device.decoder_utilization(), "decoder utilization", index)?
                .flatten();
        let throttle_reasons =
            Self::optional_metric(device.throttle_reasons(), "throttle reasons", index)?;

        // Calculate health score
        let health_score = if let Some(throttle_reasons) = throttle_reasons {
            let thresholds = device
                .thermal_thresholds()
                .map_err(|error| metric_error("thermal thresholds", error))?;

            let is_thermal_throttling = throttle_reasons.sw_thermal || throttle_reasons.hw_thermal;
            let is_power_throttling =
                throttle_reasons.sw_power_cap || throttle_reasons.hw_power_brake;

            // Calculate VRAM usage ratio
            let vram_usage_ratio = if memory_info.total > 0 {
                Some((memory_info.used as f64 / memory_info.total as f64).clamp(0.0, 1.0))
            } else {
                None
            };

            let uptime_seconds = nvctl::health::system_uptime_seconds().unwrap_or(0);

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

        Ok(GpuStateSnapshot {
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

    /// Poll only the safety-critical values needed by fan curve control.
    pub fn poll_fan_control(
        &self,
        gpu_index: u32,
        fan_index: u32,
    ) -> Result<(nvctl::domain::Temperature, FanSpeed, FanPolicy), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_index(gpu_index)
            .map_err(|error| format!("Failed to open GPU {gpu_index}: {error}"))?;
        let fan_count = device
            .fan_count()
            .map_err(|error| format!("Failed to read GPU {gpu_index} fan count: {error}"))?;
        if fan_index >= fan_count {
            return Err(format!(
                "Fan {fan_index} is out of range for GPU {gpu_index} ({fan_count} fans)"
            ));
        }

        let temperature = device
            .temperature()
            .map_err(|error| format!("Failed to read GPU {gpu_index} temperature: {error}"))?;
        let speed = device.fan_speed(fan_index).map_err(|error| {
            format!("Failed to read GPU {gpu_index} Fan {fan_index} speed: {error}")
        })?;
        let policy = device.fan_policy(fan_index).map_err(|error| {
            format!("Failed to read GPU {gpu_index} Fan {fan_index} policy: {error}")
        })?;
        Ok((temperature, speed, policy))
    }

    /// Poll the safety-critical fan values using a stable GPU UUID.
    pub fn poll_fan_control_by_uuid(
        &self,
        gpu_uuid: &str,
        fan_index: u32,
    ) -> Result<(nvctl::domain::Temperature, FanSpeed, FanPolicy), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to open GPU {gpu_uuid}: {error}"))?;
        let fan_count = device
            .fan_count()
            .map_err(|error| format!("Failed to read GPU {gpu_uuid} fan count: {error}"))?;
        if fan_index >= fan_count {
            return Err(format!(
                "Fan {fan_index} is out of range for GPU {gpu_uuid} ({fan_count} fans)"
            ));
        }

        let temperature = device
            .temperature()
            .map_err(|error| format!("Failed to read GPU {gpu_uuid} temperature: {error}"))?;
        let speed = device.fan_speed(fan_index).map_err(|error| {
            format!("Failed to read GPU {gpu_uuid} Fan {fan_index} speed: {error}")
        })?;
        let policy = device.fan_policy(fan_index).map_err(|error| {
            format!("Failed to read GPU {gpu_uuid} Fan {fan_index} policy: {error}")
        })?;
        Ok((temperature, speed, policy))
    }

    fn optional_metric<T>(
        result: Result<T, NvmlError>,
        metric: &str,
        gpu_index: u32,
    ) -> Result<Option<T>, String> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(error) => Err(format!(
                "Failed to read {metric} for GPU {gpu_index}: {error}"
            )),
        }
    }

    /// Get driver version
    pub fn driver_version(&self) -> Option<String> {
        self.manager.as_ref()?.driver_version().ok()
    }

    /// Get cooler targets for all fans on a GPU
    pub fn get_cooler_targets(&self, index: u32) -> Result<Vec<CoolerTarget>, String> {
        let Some(ref manager) = self.manager else {
            return Err("NVML not available".to_string());
        };

        let device = manager
            .device_by_index(index)
            .map_err(|error| format!("Failed to open GPU {index}: {error}"))?;

        let fan_count = device
            .fan_count()
            .map_err(|error| format!("Failed to read GPU {index} fan count: {error}"))?;
        let mut targets = Vec::with_capacity(fan_count as usize);

        for i in 0..fan_count {
            targets.push(device.cooler_target(i).map_err(|error| {
                format!("Failed to read GPU {index} Fan {i} cooler target: {error}")
            })?);
        }

        Ok(targets)
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

    /// Set fan policy using a stable GPU UUID.
    pub fn set_fan_policy_by_uuid(
        &self,
        gpu_uuid: &str,
        fan_index: u32,
        policy: FanPolicy,
    ) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let mut device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to get GPU {gpu_uuid}: {error}"))?;

        device
            .set_fan_policy(fan_index, policy)
            .map_err(|error| Self::format_nvml_error(error, "set fan policy"))
    }

    /// Set fan speed for a specific fan on a GPU
    ///
    /// Note: NVML implicitly switches the fan to Manual mode on success.
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

    /// Set fan speed using a stable GPU UUID.
    pub fn set_fan_speed_by_uuid(
        &self,
        gpu_uuid: &str,
        fan_index: u32,
        speed: FanSpeed,
    ) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let mut device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to get GPU {gpu_uuid}: {error}"))?;

        device
            .set_fan_speed(fan_index, speed)
            .map_err(|error| Self::format_nvml_error(error, "set fan speed"))
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

    /// Set a power limit using a stable GPU UUID.
    pub fn set_power_limit_by_uuid(&self, gpu_uuid: &str, limit: PowerLimit) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let mut device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to get GPU {gpu_uuid}: {error}"))?;

        device
            .set_power_limit(limit)
            .map_err(|error| Self::format_nvml_error(error, "set power limit"))
    }

    /// Read the current acoustic target and its supported range.
    pub fn acoustic_limits(&self, gpu_index: u32) -> Result<Option<AcousticLimits>, String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_index(gpu_index)
            .map_err(|e| format!("Failed to get GPU {}: {}", gpu_index, e))?;

        Self::optional_metric(device.acoustic_limits(), "acoustic limits", gpu_index)
    }

    /// Read acoustic limits using a stable GPU UUID.
    pub fn acoustic_limits_by_uuid(
        &self,
        gpu_uuid: &str,
    ) -> Result<Option<AcousticLimits>, String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to get GPU {gpu_uuid}: {error}"))?;

        match device.acoustic_limits() {
            Ok(limits) => Ok(Some(limits)),
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(error) => Err(format!(
                "Failed to read acoustic limits for GPU {gpu_uuid}: {error}"
            )),
        }
    }

    /// Set the acoustic temperature target for a GPU.
    pub fn set_acoustic_limit(&self, gpu_index: u32, limit: Temperature) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let mut device = manager
            .device_by_index(gpu_index)
            .map_err(|e| format!("Failed to get GPU {}: {}", gpu_index, e))?;

        device
            .set_acoustic_limit(limit)
            .map_err(|e| Self::format_nvml_error(e, "set acoustic temperature limit"))
    }

    /// Set the acoustic temperature target using a stable GPU UUID.
    pub fn set_acoustic_limit_by_uuid(
        &self,
        gpu_uuid: &str,
        limit: Temperature,
    ) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "NVML not available".to_string())?;
        let mut device = manager
            .device_by_uuid(gpu_uuid)
            .map_err(|error| format!("Failed to get GPU {gpu_uuid}: {error}"))?;

        device
            .set_acoustic_limit(limit)
            .map_err(|error| Self::format_nvml_error(error, "set acoustic temperature limit"))
    }

    /// Format NVML error into user-friendly message
    fn format_nvml_error(error: NvmlError, operation: &str) -> String {
        match error {
            NvmlError::InsufficientPermissions(_) => {
                format!(
                    "Permission denied to {}. Retry with appropriate privileges or ask an administrator to configure a least-privilege policy.",
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
