//! Domain models for nvctl
//!
//! This module contains all domain types with validation.
//! Types are validated on construction (fail-fast pattern).

pub mod fan;
pub mod gpu;
pub mod memory;
pub mod pcie;
pub mod performance;
pub mod power;
pub mod process;
pub mod thermal;

pub use fan::{CoolerTarget, FanCurve, FanCurvePoint, FanInfo, FanPolicy, FanSpeed};
pub use gpu::GpuInfo;
pub use memory::{EccErrors, EccHealthStatus, EccMode};
pub use pcie::{
    PcieGeneration, PcieLinkStatus, PcieLinkWidth, PcieMetrics, PcieReplayCounter, PcieThroughput,
};
pub use performance::{
    ClockSpeed, ClockType, DecoderUtilization, EncoderUtilization, MemoryInfo, PerformanceState,
    ThrottleReasons, Utilization,
};
pub use power::{PowerConstraints, PowerLimit};
pub use process::{GpuProcess, ProcessList, ProcessType};
pub use thermal::{
    AcousticLimits, Temperature, TemperatureReading, TemperatureSensor, ThermalThresholds,
};
