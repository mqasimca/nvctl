//! Thermal command implementation
//!
//! Handles thermal status and acoustic limit commands.

use crate::cli::args::{OutputFormat, ThermalArgs, ThermalCommands};
use crate::cli::output::{print_output, AcousticStatus, Message};
use crate::domain::Temperature;
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute thermal commands
pub fn run_thermal(
    args: &ThermalArgs,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        ThermalCommands::Status => run_thermal_status(&manager, format, gpu_index),
        ThermalCommands::Limit { celsius } => {
            run_thermal_limit(&manager, *celsius, format, gpu_index, dry_run)
        }
    }
}

fn run_thermal_status(
    manager: &NvmlManager,
    format: OutputFormat,
    gpu_index: Option<u32>,
) -> Result<()> {
    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        let current_temp = device.temperature().map(|t| t.as_celsius()).unwrap_or(0);
        let acoustic = device.acoustic_limits().ok();

        let status = AcousticStatus {
            gpu_name: info.name,
            gpu_index: idx,
            current_temp,
            acoustic_current: acoustic
                .as_ref()
                .and_then(|a| a.current.map(|t| t.as_celsius())),
            acoustic_min: acoustic
                .as_ref()
                .and_then(|a| a.min.map(|t| t.as_celsius())),
            acoustic_max: acoustic.and_then(|a| a.max.map(|t| t.as_celsius())),
        };

        print_output(&status, format)?;
    }

    Ok(())
}

fn run_thermal_limit(
    manager: &NvmlManager,
    celsius: i32,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let target = Temperature::new(celsius);

    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for idx in indices {
        let mut device = manager.device_by_index(idx)?;
        let info = device.info()?;

        // Validate against acoustic limits
        if let Ok(limits) = device.acoustic_limits() {
            if !limits.is_valid(target) {
                let min = limits.min.map(|t| t.as_celsius()).unwrap_or(0);
                let max = limits.max.map(|t| t.as_celsius()).unwrap_or(100);
                let message = Message {
                    message: format!(
                        "Cannot set acoustic limit on GPU {}: {}°C is outside valid range {}°C - {}°C",
                        info.name, celsius, min, max
                    ),
                    success: false,
                };
                print_output(&message, format)?;
                continue;
            }
        }

        let message = if dry_run {
            format!(
                "[DRY RUN] Would set acoustic temperature limit to {}°C on GPU {}",
                celsius, info.name
            )
        } else {
            device.set_acoustic_limit(target)?;
            format!(
                "Set acoustic temperature limit to {}°C on GPU {}",
                celsius, info.name
            )
        };

        print_output(
            &Message {
                message,
                success: true,
            },
            format,
        )?;
    }

    Ok(())
}
