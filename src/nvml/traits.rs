//! Trait definitions for GPU operations
//!
//! These traits abstract over NVML to enable testing with mocks.

use crate::domain::{
    AcousticLimits, ClockSpeed, ClockType, CoolerTarget, DecoderUtilization, EccErrors, EccMode,
    EncoderUtilization, FanPolicy, FanSpeed, GpuInfo, MemoryInfo, PcieMetrics, PerformanceState,
    PowerConstraints, PowerLimit, ProcessList, Temperature, TemperatureReading, ThermalThresholds,
    ThrottleReasons, Utilization,
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

    /// Get memory temperature (if available)
    ///
    /// Returns `Ok(None)` if the GPU doesn't have a separate memory temperature sensor.
    /// GDDR6X GPUs typically have memory temperature sensors.
    fn memory_temperature(&self) -> Result<Option<Temperature>, NvmlError>;

    /// Get all temperature readings (GPU and memory)
    fn temperature_readings(&self) -> Result<Vec<TemperatureReading>, NvmlError> {
        let mut readings = vec![];

        if let Ok(temp) = self.temperature() {
            readings.push(TemperatureReading::gpu(temp));
        }

        if let Ok(Some(mem_temp)) = self.memory_temperature() {
            readings.push(TemperatureReading::memory(mem_temp));
        }

        Ok(readings)
    }

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

    /// Get what component a fan/cooler is designed to cool
    ///
    /// Returns the target (GPU, Memory, Power Supply, etc.) that this fan cools.
    /// This can help identify fan positions (e.g., exhaust fans often cool power supply).
    fn cooler_target(&self, fan_idx: u32) -> Result<CoolerTarget, NvmlError>;

    // Power operations
    /// Get current power limit
    fn power_limit(&self) -> Result<PowerLimit, NvmlError>;

    /// Get power constraints (min/max/default)
    fn power_constraints(&self) -> Result<PowerConstraints, NvmlError>;

    /// Set power limit
    fn set_power_limit(&mut self, limit: PowerLimit) -> Result<(), NvmlError>;

    /// Get current power usage
    fn power_usage(&self) -> Result<PowerLimit, NvmlError>;

    // Performance monitoring operations
    /// Get current clock speed for a specific clock type
    fn clock_speed(&self, clock_type: ClockType) -> Result<ClockSpeed, NvmlError>;

    /// Get GPU and memory utilization rates
    fn utilization(&self) -> Result<Utilization, NvmlError>;

    /// Get encoder utilization
    ///
    /// Returns `Ok(None)` if encoder utilization is not available or not supported.
    fn encoder_utilization(&self) -> Result<Option<EncoderUtilization>, NvmlError>;

    /// Get decoder utilization
    ///
    /// Returns `Ok(None)` if decoder utilization is not available or not supported.
    fn decoder_utilization(&self) -> Result<Option<DecoderUtilization>, NvmlError>;

    /// Get memory (VRAM) information
    fn memory_info(&self) -> Result<MemoryInfo, NvmlError>;

    /// Get current performance state (P-state)
    fn performance_state(&self) -> Result<PerformanceState, NvmlError>;

    /// Get current clock throttle reasons
    fn throttle_reasons(&self) -> Result<ThrottleReasons, NvmlError>;

    // ECC memory error tracking
    /// Get ECC mode configuration
    ///
    /// Returns `Ok(None)` if the GPU doesn't support ECC memory.
    fn ecc_mode(&self) -> Result<Option<EccMode>, NvmlError>;

    /// Get ECC memory error counts
    ///
    /// Returns `Ok(None)` if the GPU doesn't support ECC memory.
    /// Otherwise returns error counts for correctable and uncorrectable errors,
    /// both for current boot and GPU lifetime.
    fn ecc_errors(&self) -> Result<Option<EccErrors>, NvmlError>;

    // PCIe monitoring
    /// Get PCIe link and throughput metrics
    ///
    /// Returns PCIe generation, link width, throughput, and error counters.
    fn pcie_metrics(&self) -> Result<PcieMetrics, NvmlError>;

    // Process monitoring operations
    /// Get list of processes running on this GPU
    fn running_processes(&self) -> Result<ProcessList, NvmlError>;
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
