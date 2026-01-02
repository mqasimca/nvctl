//! Trait definitions for GPU operations
//!
//! These traits abstract over NVML to enable testing with mocks.

use crate::domain::{
    AcousticLimits, FanPolicy, FanSpeed, GpuInfo, PowerConstraints, PowerLimit, Temperature,
    ThermalThresholds,
};
use crate::error::NvmlError;

/// Trait for GPU device operations
///
/// This trait abstracts all GPU operations, allowing for mock implementations
/// in tests while using real NVML in production.
pub trait GpuDevice: Send + Sync {
    /// Get GPU information
    fn info(&self) -> Result<GpuInfo, NvmlError>;

    /// Get the GPU name
    fn name(&self) -> Result<String, NvmlError>;

    /// Get the GPU UUID
    fn uuid(&self) -> Result<String, NvmlError>;

    /// Get the GPU index
    fn index(&self) -> u32;

    // Temperature operations
    /// Get current GPU temperature
    fn temperature(&self) -> Result<Temperature, NvmlError>;

    /// Get thermal thresholds
    fn thermal_thresholds(&self) -> Result<ThermalThresholds, NvmlError>;

    /// Get acoustic temperature limits (min/max/current)
    fn acoustic_limits(&self) -> Result<AcousticLimits, NvmlError>;

    /// Set acoustic temperature limit
    ///
    /// This tells the GPU to throttle to maintain a target temperature.
    /// Same setting as GeForce Experience temperature target.
    fn set_acoustic_limit(&mut self, temp: Temperature) -> Result<(), NvmlError>;

    // Fan operations
    /// Get the number of fans
    fn fan_count(&self) -> Result<u32, NvmlError>;

    /// Get current fan speed for a specific fan
    fn fan_speed(&self, fan_idx: u32) -> Result<FanSpeed, NvmlError>;

    /// Set fan speed for a specific fan
    fn set_fan_speed(&mut self, fan_idx: u32, speed: FanSpeed) -> Result<(), NvmlError>;

    /// Get current fan control policy
    fn fan_policy(&self, fan_idx: u32) -> Result<FanPolicy, NvmlError>;

    /// Set fan control policy
    fn set_fan_policy(&mut self, fan_idx: u32, policy: FanPolicy) -> Result<(), NvmlError>;

    // Power operations
    /// Get current power limit
    fn power_limit(&self) -> Result<PowerLimit, NvmlError>;

    /// Get power constraints (min/max/default)
    fn power_constraints(&self) -> Result<PowerConstraints, NvmlError>;

    /// Set power limit
    fn set_power_limit(&mut self, limit: PowerLimit) -> Result<(), NvmlError>;

    /// Get current power usage
    fn power_usage(&self) -> Result<PowerLimit, NvmlError>;
}

/// Trait for managing multiple GPUs
///
/// This trait provides methods for discovering and accessing GPU devices.
pub trait GpuManager: Send + Sync {
    /// The device type returned by this manager
    type Device: GpuDevice;

    /// Get the number of GPU devices
    fn device_count(&self) -> Result<u32, NvmlError>;

    /// Get a device by index
    fn device_by_index(&self, index: u32) -> Result<Self::Device, NvmlError>;

    /// Get a device by UUID
    fn device_by_uuid(&self, uuid: &str) -> Result<Self::Device, NvmlError>;

    /// Get a device by name (partial match)
    fn device_by_name(&self, name: &str) -> Result<Self::Device, NvmlError>;

    /// Get all devices
    fn all_devices(&self) -> Result<Vec<Self::Device>, NvmlError> {
        let count = self.device_count()?;
        let mut devices = Vec::with_capacity(count as usize);
        for i in 0..count {
            devices.push(self.device_by_index(i)?);
        }
        Ok(devices)
    }

    /// Get driver version
    fn driver_version(&self) -> Result<String, NvmlError>;

    /// Get NVML version
    fn nvml_version(&self) -> Result<String, NvmlError>;
}
