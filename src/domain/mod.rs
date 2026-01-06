//! Domain models for nvctl
//!
//! This module contains all domain types with validation.
//! Types are validated on construction (fail-fast pattern).

pub mod fan;
pub mod gpu;
pub mod power;
pub mod thermal;

pub use fan::{CoolerTarget, FanCurve, FanCurvePoint, FanInfo, FanPolicy, FanSpeed};
pub use gpu::GpuInfo;
pub use power::{PowerConstraints, PowerLimit};
pub use thermal::{AcousticLimits, Temperature, ThermalThresholds};
