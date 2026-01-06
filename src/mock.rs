//! Mock implementations for testing
//!
//! Provides mock GPU device and manager for unit testing without real hardware.

use crate::domain::{
    AcousticLimits, CoolerTarget, FanPolicy, FanSpeed, GpuInfo, PowerConstraints, PowerLimit,
    Temperature, ThermalThresholds,
};
use crate::error::NvmlError;
use crate::nvml::{GpuDevice, GpuManager};

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

/// Mock GPU device for testing
#[derive(Debug)]
pub struct MockDevice {
    index: u32,
    name: String,
    uuid: String,
    temperature: RwLock<Temperature>,
    fan_speeds: Mutex<HashMap<u32, FanSpeed>>,
    fan_policies: Mutex<HashMap<u32, FanPolicy>>,
    fan_count: u32,
    power_limit: Mutex<PowerLimit>,
    power_constraints: PowerConstraints,
    power_usage: PowerLimit,
    thermal_thresholds: ThermalThresholds,
    acoustic_limits: RwLock<AcousticLimits>,
}

impl MockDevice {
    /// Create a new mock device with default values
    pub fn new(index: u32) -> Self {
        let default_speed = FanSpeed::new(50).unwrap();
        let mut fan_speeds = HashMap::new();
        let mut fan_policies = HashMap::new();
        fan_speeds.insert(0, default_speed);
        fan_speeds.insert(1, default_speed);
        fan_policies.insert(0, FanPolicy::Auto);
        fan_policies.insert(1, FanPolicy::Auto);

        Self {
            index,
            name: format!("Mock GPU {}", index),
            uuid: format!("GPU-MOCK-{:04}", index),
            temperature: RwLock::new(Temperature::new(45)),
            fan_speeds: Mutex::new(fan_speeds),
            fan_policies: Mutex::new(fan_policies),
            fan_count: 2,
            power_limit: Mutex::new(PowerLimit::from_watts(300)),
            power_constraints: PowerConstraints::new(
                PowerLimit::from_watts(100),
                PowerLimit::from_watts(400),
                PowerLimit::from_watts(300),
            ),
            power_usage: PowerLimit::from_watts(150),
            thermal_thresholds: ThermalThresholds::new(
                Some(Temperature::new(100)),
                Some(Temperature::new(95)),
                Some(Temperature::new(83)),
            ),
            acoustic_limits: RwLock::new(AcousticLimits::new(
                Some(Temperature::new(80)),
                Some(Temperature::new(60)),
                Some(Temperature::new(90)),
            )),
        }
    }

    /// Set the mock temperature
    pub fn set_temperature(&self, temp: Temperature) {
        *self.temperature.write().unwrap() = temp;
    }

    /// Builder: set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Builder: set UUID
    pub fn with_uuid(mut self, uuid: impl Into<String>) -> Self {
        self.uuid = uuid.into();
        self
    }

    /// Builder: set fan count
    pub fn with_fan_count(mut self, count: u32) -> Self {
        self.fan_count = count;
        self
    }

    /// Builder: set power constraints
    pub fn with_power_constraints(mut self, constraints: PowerConstraints) -> Self {
        self.power_constraints = constraints;
        self
    }
}

// SAFETY: MockDevice uses Mutex/RwLock for interior mutability, making it Sync
unsafe impl Sync for MockDevice {}

impl GpuDevice for MockDevice {
    fn info(&self) -> Result<GpuInfo, NvmlError> {
        Ok(
            GpuInfo::new(self.index, self.name.clone(), self.uuid.clone())
                .with_fan_count(self.fan_count),
        )
    }

    fn name(&self) -> Result<String, NvmlError> {
        Ok(self.name.clone())
    }

    fn uuid(&self) -> Result<String, NvmlError> {
        Ok(self.uuid.clone())
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn temperature(&self) -> Result<Temperature, NvmlError> {
        Ok(*self.temperature.read().unwrap())
    }

    fn thermal_thresholds(&self) -> Result<ThermalThresholds, NvmlError> {
        Ok(self.thermal_thresholds)
    }

    fn acoustic_limits(&self) -> Result<AcousticLimits, NvmlError> {
        Ok(*self.acoustic_limits.read().unwrap())
    }

    fn set_acoustic_limit(&mut self, temp: Temperature) -> Result<(), NvmlError> {
        let mut limits = self.acoustic_limits.write().unwrap();
        if !limits.is_valid(temp) {
            return Err(NvmlError::InvalidArgument(format!(
                "Temperature {} outside valid range",
                temp
            )));
        }
        limits.current = Some(temp);
        Ok(())
    }

    fn fan_count(&self) -> Result<u32, NvmlError> {
        Ok(self.fan_count)
    }

    fn fan_speed(&self, fan_idx: u32) -> Result<FanSpeed, NvmlError> {
        self.fan_speeds
            .lock()
            .unwrap()
            .get(&fan_idx)
            .copied()
            .ok_or_else(|| NvmlError::InvalidArgument(format!("Fan {} not found", fan_idx)))
    }

    fn set_fan_speed(&mut self, fan_idx: u32, speed: FanSpeed) -> Result<(), NvmlError> {
        if fan_idx >= self.fan_count {
            return Err(NvmlError::InvalidArgument(format!(
                "Fan {} not found (count: {})",
                fan_idx, self.fan_count
            )));
        }
        self.fan_speeds.lock().unwrap().insert(fan_idx, speed);
        Ok(())
    }

    fn fan_policy(&self, fan_idx: u32) -> Result<FanPolicy, NvmlError> {
        self.fan_policies
            .lock()
            .unwrap()
            .get(&fan_idx)
            .copied()
            .ok_or_else(|| NvmlError::InvalidArgument(format!("Fan {} not found", fan_idx)))
    }

    fn set_fan_policy(&mut self, fan_idx: u32, policy: FanPolicy) -> Result<(), NvmlError> {
        if fan_idx >= self.fan_count {
            return Err(NvmlError::InvalidArgument(format!(
                "Fan {} not found (count: {})",
                fan_idx, self.fan_count
            )));
        }
        self.fan_policies.lock().unwrap().insert(fan_idx, policy);
        Ok(())
    }

    fn cooler_target(&self, fan_idx: u32) -> Result<CoolerTarget, NvmlError> {
        // Mock implementation: assign targets based on fan index
        // This simulates a typical 4-fan GPU layout
        match fan_idx {
            0 => Ok(CoolerTarget::Gpu),         // Center fan, cools GPU
            1 => Ok(CoolerTarget::Memory),      // Side fan, cools memory
            2 => Ok(CoolerTarget::Memory),      // Side fan, cools memory
            3 => Ok(CoolerTarget::PowerSupply), // Back fan, cools VRM
            _ => Ok(CoolerTarget::All),         // Default for unknown
        }
    }

    fn power_limit(&self) -> Result<PowerLimit, NvmlError> {
        Ok(*self.power_limit.lock().unwrap())
    }

    fn power_constraints(&self) -> Result<PowerConstraints, NvmlError> {
        Ok(self.power_constraints)
    }

    fn set_power_limit(&mut self, limit: PowerLimit) -> Result<(), NvmlError> {
        if !self.power_constraints.contains(&limit) {
            return Err(NvmlError::InvalidArgument(format!(
                "Power limit {} out of range",
                limit
            )));
        }
        *self.power_limit.lock().unwrap() = limit;
        Ok(())
    }

    fn power_usage(&self) -> Result<PowerLimit, NvmlError> {
        Ok(self.power_usage)
    }
}

/// Mock GPU manager for testing
pub struct MockManager {
    devices: Vec<MockDevice>,
    driver_version: String,
    nvml_version: String,
}

impl MockManager {
    /// Create a new mock manager with the specified number of devices
    pub fn new(device_count: u32) -> Self {
        let devices = (0..device_count).map(MockDevice::new).collect();

        Self {
            devices,
            driver_version: "535.154.05".to_string(),
            nvml_version: "12.535.154.05".to_string(),
        }
    }

    /// Create a mock manager with custom devices
    pub fn with_devices(devices: Vec<MockDevice>) -> Self {
        Self {
            devices,
            driver_version: "535.154.05".to_string(),
            nvml_version: "12.535.154.05".to_string(),
        }
    }
}

// SAFETY: MockManager only contains MockDevice which is Sync
unsafe impl Sync for MockManager {}

impl GpuManager for MockManager {
    type Device = MockDevice;

    fn device_count(&self) -> Result<u32, NvmlError> {
        Ok(self.devices.len() as u32)
    }

    fn device_by_index(&self, index: u32) -> Result<Self::Device, NvmlError> {
        self.devices
            .get(index as usize)
            .map(|d| MockDevice {
                index: d.index,
                name: d.name.clone(),
                uuid: d.uuid.clone(),
                temperature: RwLock::new(*d.temperature.read().unwrap()),
                fan_speeds: Mutex::new(d.fan_speeds.lock().unwrap().clone()),
                fan_policies: Mutex::new(d.fan_policies.lock().unwrap().clone()),
                fan_count: d.fan_count,
                power_limit: Mutex::new(*d.power_limit.lock().unwrap()),
                power_constraints: d.power_constraints,
                power_usage: d.power_usage,
                thermal_thresholds: d.thermal_thresholds,
                acoustic_limits: RwLock::new(*d.acoustic_limits.read().unwrap()),
            })
            .ok_or(NvmlError::DeviceNotFound(index))
    }

    fn device_by_uuid(&self, uuid: &str) -> Result<Self::Device, NvmlError> {
        for d in &self.devices {
            if d.uuid == uuid {
                return self.device_by_index(d.index);
            }
        }
        Err(NvmlError::DeviceNotFoundByUuid(uuid.to_string()))
    }

    fn device_by_name(&self, name: &str) -> Result<Self::Device, NvmlError> {
        let name_lower = name.to_lowercase();
        for d in &self.devices {
            if d.name.to_lowercase().contains(&name_lower) {
                return self.device_by_index(d.index);
            }
        }
        Err(NvmlError::Unknown(format!(
            "No GPU found matching: {}",
            name
        )))
    }

    fn driver_version(&self) -> Result<String, NvmlError> {
        Ok(self.driver_version.clone())
    }

    fn nvml_version(&self) -> Result<String, NvmlError> {
        Ok(self.nvml_version.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_device_creation() {
        let device = MockDevice::new(0);
        assert_eq!(device.index(), 0);
        assert_eq!(device.fan_count().unwrap(), 2);
    }

    #[test]
    fn test_mock_device_temperature() {
        let device = MockDevice::new(0);
        assert_eq!(device.temperature().unwrap().as_celsius(), 45);

        device.set_temperature(Temperature::new(75));
        assert_eq!(device.temperature().unwrap().as_celsius(), 75);
    }

    #[test]
    fn test_mock_device_fan_speed() {
        let mut device = MockDevice::new(0);

        let initial = device.fan_speed(0).unwrap();
        assert_eq!(initial.as_percentage(), 50);

        let new_speed = FanSpeed::new(80).unwrap();
        device.set_fan_speed(0, new_speed).unwrap();
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 80);
    }

    #[test]
    fn test_mock_device_power_limit() {
        let mut device = MockDevice::new(0);

        let initial = device.power_limit().unwrap();
        assert_eq!(initial.as_watts(), 300);

        let new_limit = PowerLimit::from_watts(350);
        device.set_power_limit(new_limit).unwrap();
        assert_eq!(device.power_limit().unwrap().as_watts(), 350);
    }

    #[test]
    fn test_mock_device_power_limit_out_of_range() {
        let mut device = MockDevice::new(0);

        let invalid = PowerLimit::from_watts(500);
        assert!(device.set_power_limit(invalid).is_err());
    }

    #[test]
    fn test_mock_manager_device_count() {
        let manager = MockManager::new(2);
        assert_eq!(manager.device_count().unwrap(), 2);
    }

    #[test]
    fn test_mock_manager_device_by_index() {
        let manager = MockManager::new(2);
        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.index(), 0);

        let device = manager.device_by_index(1).unwrap();
        assert_eq!(device.index(), 1);

        assert!(manager.device_by_index(5).is_err());
    }

    #[test]
    fn test_mock_manager_device_by_uuid() {
        let manager = MockManager::new(2);
        let device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        assert_eq!(device.index(), 0);

        assert!(manager.device_by_uuid("GPU-INVALID").is_err());
    }
}
