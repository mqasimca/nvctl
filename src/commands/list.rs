//! List command implementation
//!
//! Lists all detected NVIDIA GPUs.

use crate::cli::args::OutputFormat;
use crate::cli::output::{print_output, GpuList, GpuListEntry};
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the list command
pub fn run_list(format: OutputFormat) -> Result<()> {
    let manager = NvmlManager::new()?;
    let driver_version = manager.driver_version()?;
    let count = manager.device_count()?;

    let mut gpus = Vec::with_capacity(count as usize);

    for i in 0..count {
        let device = manager.device_by_index(i)?;
        let info = device.info()?;
        gpus.push(GpuListEntry::from(&info));
    }

    let gpu_list = GpuList {
        gpus,
        driver_version,
    };

    print_output(&gpu_list, format)?;

    Ok(())
}
