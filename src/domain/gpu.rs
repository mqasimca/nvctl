//! GPU information domain type
//!
//! Provides the GpuInfo struct for GPU identification and metadata.

use serde::{Deserialize, Serialize};
use std::fmt;

/// GPU information and identification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU index (0-based)
    pub index: u32,
    /// GPU name (e.g., "NVIDIA GeForce RTX 4090")
    pub name: String,
    /// Unique GPU UUID
    pub uuid: String,
    /// PCI bus ID
    pub pci_bus_id: Option<String>,
    /// Driver version
    pub driver_version: Option<String>,
    /// VBIOS version
    pub vbios_version: Option<String>,
    /// Number of fan controllers
    pub fan_count: u32,
}

impl GpuInfo {
    /// Create new GPU info
    pub fn new(index: u32, name: String, uuid: String) -> Self {
        Self {
            index,
            name,
            uuid,
            pci_bus_id: None,
            driver_version: None,
            vbios_version: None,
            fan_count: 0,
        }
    }

    /// Set the PCI bus ID
    pub fn with_pci_bus_id(mut self, bus_id: String) -> Self {
        self.pci_bus_id = Some(bus_id);
        self
    }

    /// Set the driver version
    pub fn with_driver_version(mut self, version: String) -> Self {
        self.driver_version = Some(version);
        self
    }

    /// Set the VBIOS version
    pub fn with_vbios_version(mut self, version: String) -> Self {
        self.vbios_version = Some(version);
        self
    }

    /// Set the fan count
    pub fn with_fan_count(mut self, count: u32) -> Self {
        self.fan_count = count;
        self
    }

    /// Get a short display name
    pub fn short_name(&self) -> &str {
        // Remove "NVIDIA " prefix if present
        self.name.strip_prefix("NVIDIA ").unwrap_or(&self.name)
    }
}

impl fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.index, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_info_display() {
        let info = GpuInfo::new(
            0,
            "NVIDIA GeForce RTX 4090".to_string(),
            "GPU-xxx".to_string(),
        );
        assert_eq!(info.to_string(), "[0] NVIDIA GeForce RTX 4090");
    }

    #[test]
    fn test_gpu_info_short_name() {
        let info = GpuInfo::new(
            0,
            "NVIDIA GeForce RTX 4090".to_string(),
            "GPU-xxx".to_string(),
        );
        assert_eq!(info.short_name(), "GeForce RTX 4090");
    }

    #[test]
    fn test_gpu_info_builder() {
        let info = GpuInfo::new(0, "Test GPU".to_string(), "GPU-123".to_string())
            .with_fan_count(2)
            .with_driver_version("535.154.05".to_string());

        assert_eq!(info.fan_count, 2);
        assert_eq!(info.driver_version.as_deref(), Some("535.154.05"));
    }
}
