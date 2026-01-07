//! Performance and utilization domain types
//!
//! Types for GPU clocks, utilization rates, and VRAM usage.

use serde::{Deserialize, Serialize};

/// GPU clock speed in MHz
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClockSpeed(u32);

impl ClockSpeed {
    /// Create a new clock speed value
    pub fn new(mhz: u32) -> Self {
        Self(mhz)
    }

    /// Get clock speed in MHz
    pub fn as_mhz(&self) -> u32 {
        self.0
    }

    /// Get clock speed in GHz
    pub fn as_ghz(&self) -> f32 {
        self.0 as f32 / 1000.0
    }
}

impl std::fmt::Display for ClockSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} MHz", self.0)
    }
}

/// Clock type for querying specific clocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockType {
    /// Graphics/SM clock
    Graphics,
    /// Streaming Multiprocessor clock (same as graphics on most GPUs)
    SM,
    /// Memory clock
    Memory,
    /// Video encoder/decoder clock
    Video,
}

/// GPU and memory utilization rates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Utilization {
    /// GPU compute utilization (0-100%)
    pub gpu: u8,
    /// Memory bandwidth utilization (0-100%)
    pub memory: u8,
}

impl Utilization {
    /// Create a new utilization value
    pub fn new(gpu: u8, memory: u8) -> Self {
        Self {
            gpu: gpu.min(100),
            memory: memory.min(100),
        }
    }

    /// Get GPU utilization as percentage
    pub fn gpu_percent(&self) -> u8 {
        self.gpu
    }

    /// Get memory bandwidth utilization as percentage
    pub fn memory_percent(&self) -> u8 {
        self.memory
    }
}

/// Video encoder utilization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EncoderUtilization {
    /// Encoder utilization (0-100%)
    pub utilization: u8,
    /// Sampling period in microseconds
    pub sampling_period_us: u32,
}

impl EncoderUtilization {
    /// Create new encoder utilization
    pub fn new(utilization: u8, sampling_period_us: u32) -> Self {
        Self {
            utilization: utilization.min(100),
            sampling_period_us,
        }
    }

    /// Get utilization as percentage
    pub fn percent(&self) -> u8 {
        self.utilization
    }
}

/// Video decoder utilization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DecoderUtilization {
    /// Decoder utilization (0-100%)
    pub utilization: u8,
    /// Sampling period in microseconds
    pub sampling_period_us: u32,
}

impl DecoderUtilization {
    /// Create new decoder utilization
    pub fn new(utilization: u8, sampling_period_us: u32) -> Self {
        Self {
            utilization: utilization.min(100),
            sampling_period_us,
        }
    }

    /// Get utilization as percentage
    pub fn percent(&self) -> u8 {
        self.utilization
    }
}

/// VRAM/Memory information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total memory in bytes
    pub total: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Free memory in bytes
    pub free: u64,
}

impl MemoryInfo {
    /// Create a new memory info value
    pub fn new(total: u64, used: u64, free: u64) -> Self {
        Self { total, used, free }
    }

    /// Get total memory in MB
    pub fn total_mb(&self) -> u64 {
        self.total / (1024 * 1024)
    }

    /// Get used memory in MB
    pub fn used_mb(&self) -> u64 {
        self.used / (1024 * 1024)
    }

    /// Get free memory in MB
    pub fn free_mb(&self) -> u64 {
        self.free / (1024 * 1024)
    }

    /// Get total memory in GB
    pub fn total_gb(&self) -> f32 {
        self.total as f32 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Get used memory in GB
    pub fn used_gb(&self) -> f32 {
        self.used as f32 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Get usage percentage (0.0 - 1.0)
    pub fn usage_ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f32 / self.total as f32
        }
    }

    /// Get usage percentage (0 - 100)
    pub fn usage_percent(&self) -> u8 {
        (self.usage_ratio() * 100.0) as u8
    }
}

/// GPU performance state (P-state)
///
/// Lower numbers = higher performance, higher power
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PerformanceState {
    /// Maximum performance (P0)
    P0,
    /// High performance (P1)
    P1,
    /// Balanced (P2)
    P2,
    /// Adaptive (P3-P7)
    P3,
    P4,
    P5,
    P6,
    P7,
    /// Power saving (P8-P11)
    P8,
    P9,
    P10,
    P11,
    /// Minimum performance (P12)
    P12,
    /// Very low power (P13-P15)
    P13,
    P14,
    P15,
    /// Unknown state
    #[default]
    Unknown,
}

impl PerformanceState {
    /// Create from raw NVML value
    pub fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::P0,
            1 => Self::P1,
            2 => Self::P2,
            3 => Self::P3,
            4 => Self::P4,
            5 => Self::P5,
            6 => Self::P6,
            7 => Self::P7,
            8 => Self::P8,
            9 => Self::P9,
            10 => Self::P10,
            11 => Self::P11,
            12 => Self::P12,
            13 => Self::P13,
            14 => Self::P14,
            15 => Self::P15,
            _ => Self::Unknown,
        }
    }

    /// Get the raw value
    pub fn as_raw(&self) -> u32 {
        match self {
            Self::P0 => 0,
            Self::P1 => 1,
            Self::P2 => 2,
            Self::P3 => 3,
            Self::P4 => 4,
            Self::P5 => 5,
            Self::P6 => 6,
            Self::P7 => 7,
            Self::P8 => 8,
            Self::P9 => 9,
            Self::P10 => 10,
            Self::P11 => 11,
            Self::P12 => 12,
            Self::P13 => 13,
            Self::P14 => 14,
            Self::P15 => 15,
            Self::Unknown => 32,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::P0 => "Maximum Performance",
            Self::P1 => "High Performance",
            Self::P2 => "Balanced",
            Self::P3 | Self::P4 | Self::P5 | Self::P6 | Self::P7 => "Adaptive",
            Self::P8 | Self::P9 | Self::P10 | Self::P11 => "Power Saving",
            Self::P12 => "Minimum Performance",
            Self::P13 | Self::P14 | Self::P15 => "Very Low Power",
            Self::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for PerformanceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            _ => write!(f, "P{}", self.as_raw()),
        }
    }
}

/// Reasons why the GPU clocks are being throttled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ThrottleReasons {
    /// GPU is idle
    pub idle: bool,
    /// Software power cap
    pub sw_power_cap: bool,
    /// Hardware slowdown (temperature/power)
    pub hw_slowdown: bool,
    /// Sync boost
    pub sync_boost: bool,
    /// Software thermal slowdown
    pub sw_thermal: bool,
    /// Hardware thermal slowdown
    pub hw_thermal: bool,
    /// Hardware power brake
    pub hw_power_brake: bool,
    /// Display clock setting
    pub display_clocks: bool,
}

impl ThrottleReasons {
    /// Check if any throttling is active
    pub fn is_throttling(&self) -> bool {
        self.sw_power_cap
            || self.hw_slowdown
            || self.sw_thermal
            || self.hw_thermal
            || self.hw_power_brake
    }

    /// Get a list of active throttle reasons
    pub fn active_reasons(&self) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        if self.idle {
            reasons.push("Idle");
        }
        if self.sw_power_cap {
            reasons.push("Power Cap");
        }
        if self.hw_slowdown {
            reasons.push("HW Slowdown");
        }
        if self.sw_thermal {
            reasons.push("SW Thermal");
        }
        if self.hw_thermal {
            reasons.push("HW Thermal");
        }
        if self.hw_power_brake {
            reasons.push("Power Brake");
        }
        if self.sync_boost {
            reasons.push("Sync Boost");
        }
        if self.display_clocks {
            reasons.push("Display Clocks");
        }
        reasons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_speed() {
        let clock = ClockSpeed::new(2100);
        assert_eq!(clock.as_mhz(), 2100);
        assert!((clock.as_ghz() - 2.1).abs() < 0.001);
    }

    #[test]
    fn test_utilization() {
        let util = Utilization::new(75, 50);
        assert_eq!(util.gpu_percent(), 75);
        assert_eq!(util.memory_percent(), 50);
    }

    #[test]
    fn test_utilization_clamp() {
        let util = Utilization::new(150, 200);
        assert_eq!(util.gpu_percent(), 100);
        assert_eq!(util.memory_percent(), 100);
    }

    #[test]
    fn test_memory_info() {
        // 8 GB total, 2 GB used
        let mem = MemoryInfo::new(
            8 * 1024 * 1024 * 1024,
            2 * 1024 * 1024 * 1024,
            6 * 1024 * 1024 * 1024,
        );
        assert_eq!(mem.total_mb(), 8192);
        assert_eq!(mem.used_mb(), 2048);
        assert!((mem.total_gb() - 8.0).abs() < 0.001);
        assert!((mem.usage_ratio() - 0.25).abs() < 0.001);
        assert_eq!(mem.usage_percent(), 25);
    }

    #[test]
    fn test_performance_state() {
        assert_eq!(PerformanceState::from_raw(0), PerformanceState::P0);
        assert_eq!(PerformanceState::from_raw(8), PerformanceState::P8);
        assert_eq!(PerformanceState::P0.description(), "Maximum Performance");
    }

    #[test]
    fn test_throttle_reasons() {
        let mut reasons = ThrottleReasons::default();
        assert!(!reasons.is_throttling());

        reasons.hw_thermal = true;
        assert!(reasons.is_throttling());
        assert_eq!(reasons.active_reasons(), vec!["HW Thermal"]);
    }

    #[test]
    fn test_encoder_utilization() {
        let encoder = EncoderUtilization::new(75, 1000);
        assert_eq!(encoder.percent(), 75);
        assert_eq!(encoder.sampling_period_us, 1000);

        // Test clamping
        let clamped = EncoderUtilization::new(150, 1000);
        assert_eq!(clamped.percent(), 100);
    }

    #[test]
    fn test_decoder_utilization() {
        let decoder = DecoderUtilization::new(50, 2000);
        assert_eq!(decoder.percent(), 50);
        assert_eq!(decoder.sampling_period_us, 2000);

        // Test clamping
        let clamped = DecoderUtilization::new(200, 2000);
        assert_eq!(clamped.percent(), 100);
    }
}
