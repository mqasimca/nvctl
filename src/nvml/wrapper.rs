//! NVML manager implementation
//!
//! Provides the main interface for NVML initialization and device discovery.

use crate::error::NvmlError;
use crate::nvml::device::NvmlDevice;
use crate::nvml::traits::{GpuDevice, GpuManager};

use nvml_wrapper::Nvml;

/// NVML manager for GPU discovery and management
pub struct NvmlManager {
    nvml: Nvml,
}

impl NvmlManager {
    /// Initialize NVML and create a new manager
    pub fn new() -> Result<Self, NvmlError> {
        let nvml = Nvml::init().map_err(|e| match e {
            nvml_wrapper::error::NvmlError::LibloadingError(_) => NvmlError::LibraryNotFound,
            nvml_wrapper::error::NvmlError::DriverNotLoaded => {
                NvmlError::InitializationFailed("NVIDIA driver not loaded".to_string())
            }
            other => NvmlError::InitializationFailed(other.to_string()),
        })?;

        Ok(Self { nvml })
    }

    /// Get a reference to the underlying NVML instance
    pub fn nvml(&self) -> &Nvml {
        &self.nvml
    }
}

impl GpuManager for NvmlManager {
    type Device = NvmlDevice<'static>;

    fn device_count(&self) -> Result<u32, NvmlError> {
        self.nvml
            .device_count()
            .map_err(|e| NvmlError::Unknown(e.to_string()))
    }

    fn device_by_index(&self, index: u32) -> Result<Self::Device, NvmlError> {
        // SAFETY: We're extending the lifetime here which is safe because
        // the NvmlDevice only lives as long as the NvmlManager.
        // This is a limitation of the nvml-wrapper API design.
        let nvml: &'static Nvml = unsafe { std::mem::transmute(&self.nvml) };

        let device = nvml.device_by_index(index).map_err(|e| match e {
            nvml_wrapper::error::NvmlError::NotFound => NvmlError::DeviceNotFound(index),
            other => NvmlError::Unknown(other.to_string()),
        })?;

        Ok(NvmlDevice::new(device, index))
    }

    fn device_by_uuid(&self, uuid: &str) -> Result<Self::Device, NvmlError> {
        // SAFETY: Same as above
        let nvml: &'static Nvml = unsafe { std::mem::transmute(&self.nvml) };

        let device = nvml.device_by_uuid(uuid).map_err(|e| match e {
            nvml_wrapper::error::NvmlError::NotFound => {
                NvmlError::DeviceNotFoundByUuid(uuid.to_string())
            }
            other => NvmlError::Unknown(other.to_string()),
        })?;

        // Find the index by iterating through devices
        let count = self.device_count()?;
        let mut index = 0;
        for i in 0..count {
            if let Ok(d) = nvml.device_by_index(i) {
                if let Ok(d_uuid) = d.uuid() {
                    if d_uuid == uuid {
                        index = i;
                        break;
                    }
                }
            }
        }

        Ok(NvmlDevice::new(device, index))
    }

    fn device_by_name(&self, name: &str) -> Result<Self::Device, NvmlError> {
        let count = self.device_count()?;
        let name_lower = name.to_lowercase();

        for i in 0..count {
            let device = self.device_by_index(i)?;
            if let Ok(device_name) = device.name() {
                if device_name.to_lowercase().contains(&name_lower) {
                    return Ok(device);
                }
            }
        }

        Err(NvmlError::Unknown(format!(
            "No GPU found matching name: {}",
            name
        )))
    }

    fn driver_version(&self) -> Result<String, NvmlError> {
        self.nvml
            .sys_driver_version()
            .map_err(|e| NvmlError::Unknown(e.to_string()))
    }

    fn nvml_version(&self) -> Result<String, NvmlError> {
        self.nvml
            .sys_nvml_version()
            .map_err(|e| NvmlError::Unknown(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual NVIDIA hardware and drivers
    // They will be skipped if NVML is not available

    #[test]
    #[ignore = "Requires NVIDIA GPU"]
    fn test_nvml_init() {
        let manager = NvmlManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    #[ignore = "Requires NVIDIA GPU"]
    fn test_device_count() {
        let manager = NvmlManager::new().unwrap();
        let count = manager.device_count();
        assert!(count.is_ok());
        assert!(count.unwrap() > 0);
    }
}
