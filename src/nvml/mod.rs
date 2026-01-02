//! NVML abstraction layer
//!
//! Provides trait-based abstractions over NVML for testability.

pub mod device;
pub mod traits;
pub mod wrapper;

pub use device::NvmlDevice;
pub use traits::{GpuDevice, GpuManager};
pub use wrapper::NvmlManager;
