//! PCIe domain types for monitoring GPU interconnect performance
//!
//! This module provides validated types for PCIe-related metrics including
//! throughput, link status, and error counters.

use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// PCIe generation (version)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcieGeneration {
    /// PCIe Gen 1 (2.5 GT/s per lane)
    Gen1,
    /// PCIe Gen 2 (5.0 GT/s per lane)
    Gen2,
    /// PCIe Gen 3 (8.0 GT/s per lane)
    Gen3,
    /// PCIe Gen 4 (16.0 GT/s per lane)
    Gen4,
    /// PCIe Gen 5 (32.0 GT/s per lane)
    Gen5,
    /// PCIe Gen 6 (64.0 GT/s per lane)
    Gen6,
}

impl PcieGeneration {
    /// Get theoretical bandwidth per lane in GB/s
    pub fn bandwidth_per_lane_gbps(&self) -> f64 {
        match self {
            Self::Gen1 => 0.25,  // 2.5 GT/s * 8/10 encoding
            Self::Gen2 => 0.5,   // 5.0 GT/s * 8/10 encoding
            Self::Gen3 => 0.985, // 8.0 GT/s * 128/130 encoding
            Self::Gen4 => 1.969, // 16.0 GT/s * 128/130 encoding
            Self::Gen5 => 3.938, // 32.0 GT/s * 128/130 encoding
            Self::Gen6 => 7.877, // 64.0 GT/s * 128/130 encoding
        }
    }

    /// Get generation number
    pub fn generation_number(&self) -> u8 {
        match self {
            Self::Gen1 => 1,
            Self::Gen2 => 2,
            Self::Gen3 => 3,
            Self::Gen4 => 4,
            Self::Gen5 => 5,
            Self::Gen6 => 6,
        }
    }
}

impl fmt::Display for PcieGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Gen {}", self.generation_number())
    }
}

/// PCIe link width (number of lanes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcieLinkWidth {
    /// 1 lane
    X1,
    /// 2 lanes
    X2,
    /// 4 lanes
    X4,
    /// 8 lanes
    X8,
    /// 16 lanes
    X16,
    /// 32 lanes
    X32,
}

impl PcieLinkWidth {
    /// Get number of lanes
    pub fn lanes(&self) -> u8 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
            Self::X32 => 32,
        }
    }

    /// Create from lane count
    pub fn from_lanes(lanes: u8) -> Result<Self, DomainError> {
        match lanes {
            1 => Ok(Self::X1),
            2 => Ok(Self::X2),
            4 => Ok(Self::X4),
            8 => Ok(Self::X8),
            16 => Ok(Self::X16),
            32 => Ok(Self::X32),
            _ => Err(DomainError::InvalidValue(format!(
                "Invalid PCIe lane count: {}. Must be 1, 2, 4, 8, 16, or 32",
                lanes
            ))),
        }
    }
}

impl fmt::Display for PcieLinkWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x{}", self.lanes())
    }
}

/// PCIe link status and capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieLinkStatus {
    /// Current PCIe generation
    pub current_generation: PcieGeneration,
    /// Maximum supported PCIe generation
    pub max_generation: PcieGeneration,
    /// Current link width
    pub current_width: PcieLinkWidth,
    /// Maximum supported link width
    pub max_width: PcieLinkWidth,
}

impl PcieLinkStatus {
    /// Create new PCIe link status
    pub fn new(
        current_generation: PcieGeneration,
        max_generation: PcieGeneration,
        current_width: PcieLinkWidth,
        max_width: PcieLinkWidth,
    ) -> Self {
        Self {
            current_generation,
            max_generation,
            current_width,
            max_width,
        }
    }

    /// Check if link is operating at maximum capability
    pub fn is_at_max_capability(&self) -> bool {
        self.current_generation == self.max_generation && self.current_width == self.max_width
    }

    /// Get current theoretical maximum bandwidth in GB/s (bidirectional)
    pub fn current_bandwidth_gbps(&self) -> f64 {
        self.current_generation.bandwidth_per_lane_gbps() * self.current_width.lanes() as f64
    }

    /// Get maximum theoretical bandwidth in GB/s (bidirectional)
    pub fn max_bandwidth_gbps(&self) -> f64 {
        self.max_generation.bandwidth_per_lane_gbps() * self.max_width.lanes() as f64
    }

    /// Get bandwidth utilization as percentage of maximum capability
    pub fn bandwidth_efficiency_percent(&self) -> f64 {
        (self.current_bandwidth_gbps() / self.max_bandwidth_gbps()) * 100.0
    }
}

impl fmt::Display for PcieLinkStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} (max: {} {})",
            self.current_generation, self.current_width, self.max_generation, self.max_width
        )
    }
}

/// PCIe throughput metrics in bytes per second
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieThroughput {
    /// Transmit (TX) throughput in bytes per second
    tx_bytes_per_sec: u64,
    /// Receive (RX) throughput in bytes per second
    rx_bytes_per_sec: u64,
}

impl PcieThroughput {
    /// Create new PCIe throughput metrics
    pub fn new(tx_bytes_per_sec: u64, rx_bytes_per_sec: u64) -> Self {
        Self {
            tx_bytes_per_sec,
            rx_bytes_per_sec,
        }
    }

    /// Create zero throughput
    pub fn zero() -> Self {
        Self {
            tx_bytes_per_sec: 0,
            rx_bytes_per_sec: 0,
        }
    }

    /// Get TX throughput in bytes per second
    pub fn tx_bytes_per_sec(&self) -> u64 {
        self.tx_bytes_per_sec
    }

    /// Get RX throughput in bytes per second
    pub fn rx_bytes_per_sec(&self) -> u64 {
        self.rx_bytes_per_sec
    }

    /// Get TX throughput in MB/s
    pub fn tx_mbps(&self) -> f64 {
        self.tx_bytes_per_sec as f64 / 1_000_000.0
    }

    /// Get RX throughput in MB/s
    pub fn rx_mbps(&self) -> f64 {
        self.rx_bytes_per_sec as f64 / 1_000_000.0
    }

    /// Get TX throughput in GB/s
    pub fn tx_gbps(&self) -> f64 {
        self.tx_bytes_per_sec as f64 / 1_000_000_000.0
    }

    /// Get RX throughput in GB/s
    pub fn rx_gbps(&self) -> f64 {
        self.rx_bytes_per_sec as f64 / 1_000_000_000.0
    }

    /// Get total bidirectional throughput in GB/s
    pub fn total_gbps(&self) -> f64 {
        self.tx_gbps() + self.rx_gbps()
    }

    /// Calculate bandwidth utilization percentage
    pub fn utilization_percent(&self, link_status: &PcieLinkStatus) -> f64 {
        let max_bandwidth_bytes_per_sec = link_status.current_bandwidth_gbps() * 1_000_000_000.0;
        let total_bytes_per_sec = (self.tx_bytes_per_sec + self.rx_bytes_per_sec) as f64;
        (total_bytes_per_sec / max_bandwidth_bytes_per_sec) * 100.0
    }
}

impl fmt::Display for PcieThroughput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TX: {:.2} GB/s, RX: {:.2} GB/s",
            self.tx_gbps(),
            self.rx_gbps()
        )
    }
}

/// PCIe replay counter (link error recovery events)
///
/// Replay counter increments indicate link instability or signal integrity issues.
/// Increasing replay counts warrant investigation of physical connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieReplayCounter {
    count: u64,
}

impl PcieReplayCounter {
    /// Create new replay counter
    pub fn new(count: u64) -> Self {
        Self { count }
    }

    /// Get replay count
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Check if replay count indicates potential issues
    ///
    /// Guideline: Any increasing replay counter warrants investigation
    pub fn is_problematic(&self) -> bool {
        self.count > 0
    }
}

impl fmt::Display for PcieReplayCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} replays", self.count)
    }
}

/// Complete PCIe metrics
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PcieMetrics {
    /// Link status and capabilities
    pub link_status: PcieLinkStatus,
    /// Current throughput
    pub throughput: PcieThroughput,
    /// Replay counter (link errors)
    pub replay_counter: PcieReplayCounter,
}

impl PcieMetrics {
    /// Create new PCIe metrics
    pub fn new(
        link_status: PcieLinkStatus,
        throughput: PcieThroughput,
        replay_counter: PcieReplayCounter,
    ) -> Self {
        Self {
            link_status,
            throughput,
            replay_counter,
        }
    }

    /// Check if PCIe link is healthy
    pub fn is_healthy(&self) -> bool {
        // Healthy if:
        // - No replay errors
        // - Operating at reasonable bandwidth (>50% of max)
        !self.replay_counter.is_problematic()
            && self.link_status.bandwidth_efficiency_percent() > 50.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcie_generation_bandwidth() {
        assert_eq!(PcieGeneration::Gen3.bandwidth_per_lane_gbps(), 0.985);
        assert_eq!(PcieGeneration::Gen4.bandwidth_per_lane_gbps(), 1.969);
    }

    #[test]
    fn test_pcie_generation_number() {
        assert_eq!(PcieGeneration::Gen3.generation_number(), 3);
        assert_eq!(PcieGeneration::Gen4.generation_number(), 4);
    }

    #[test]
    fn test_pcie_generation_display() {
        assert_eq!(format!("{}", PcieGeneration::Gen4), "Gen 4");
    }

    #[test]
    fn test_pcie_link_width_lanes() {
        assert_eq!(PcieLinkWidth::X16.lanes(), 16);
        assert_eq!(PcieLinkWidth::X8.lanes(), 8);
    }

    #[test]
    fn test_pcie_link_width_from_lanes() {
        assert_eq!(PcieLinkWidth::from_lanes(16).unwrap(), PcieLinkWidth::X16);
        assert_eq!(PcieLinkWidth::from_lanes(8).unwrap(), PcieLinkWidth::X8);
        assert!(PcieLinkWidth::from_lanes(3).is_err());
    }

    #[test]
    fn test_pcie_link_width_display() {
        assert_eq!(format!("{}", PcieLinkWidth::X16), "x16");
    }

    #[test]
    fn test_pcie_link_status_bandwidth() {
        let status = PcieLinkStatus::new(
            PcieGeneration::Gen4,
            PcieGeneration::Gen4,
            PcieLinkWidth::X16,
            PcieLinkWidth::X16,
        );

        // Gen4 x16 = 1.969 GB/s/lane * 16 lanes = 31.504 GB/s
        assert!((status.current_bandwidth_gbps() - 31.504).abs() < 0.01);
        assert!(status.is_at_max_capability());
    }

    #[test]
    fn test_pcie_link_status_not_at_max() {
        let status = PcieLinkStatus::new(
            PcieGeneration::Gen3,
            PcieGeneration::Gen4,
            PcieLinkWidth::X8,
            PcieLinkWidth::X16,
        );

        assert!(!status.is_at_max_capability());
        assert!(status.bandwidth_efficiency_percent() < 50.0);
    }

    #[test]
    fn test_pcie_throughput_conversions() {
        let throughput = PcieThroughput::new(1_000_000_000, 2_000_000_000);

        assert_eq!(throughput.tx_gbps(), 1.0);
        assert_eq!(throughput.rx_gbps(), 2.0);
        assert_eq!(throughput.total_gbps(), 3.0);
        assert_eq!(throughput.tx_mbps(), 1000.0);
    }

    #[test]
    fn test_pcie_throughput_utilization() {
        let link_status = PcieLinkStatus::new(
            PcieGeneration::Gen4,
            PcieGeneration::Gen4,
            PcieLinkWidth::X16,
            PcieLinkWidth::X16,
        );

        // Max bandwidth ~31.5 GB/s
        // Using 15.75 GB/s = 50% utilization
        let throughput = PcieThroughput::new(7_875_000_000, 7_875_000_000);
        let util = throughput.utilization_percent(&link_status);

        assert!((util - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_pcie_replay_counter() {
        let no_errors = PcieReplayCounter::new(0);
        assert!(!no_errors.is_problematic());

        let has_errors = PcieReplayCounter::new(5);
        assert!(has_errors.is_problematic());
        assert_eq!(has_errors.count(), 5);
    }

    #[test]
    fn test_pcie_metrics_healthy() {
        let link_status = PcieLinkStatus::new(
            PcieGeneration::Gen4,
            PcieGeneration::Gen4,
            PcieLinkWidth::X16,
            PcieLinkWidth::X16,
        );
        let throughput = PcieThroughput::zero();
        let replay_counter = PcieReplayCounter::new(0);

        let metrics = PcieMetrics::new(link_status, throughput, replay_counter);
        assert!(metrics.is_healthy());
    }

    #[test]
    fn test_pcie_metrics_unhealthy_replays() {
        let link_status = PcieLinkStatus::new(
            PcieGeneration::Gen4,
            PcieGeneration::Gen4,
            PcieLinkWidth::X16,
            PcieLinkWidth::X16,
        );
        let throughput = PcieThroughput::zero();
        let replay_counter = PcieReplayCounter::new(10);

        let metrics = PcieMetrics::new(link_status, throughput, replay_counter);
        assert!(!metrics.is_healthy());
    }

    #[test]
    fn test_pcie_throughput_zero() {
        let throughput = PcieThroughput::zero();
        assert_eq!(throughput.tx_bytes_per_sec(), 0);
        assert_eq!(throughput.rx_bytes_per_sec(), 0);
        assert_eq!(throughput.total_gbps(), 0.0);
    }
}
