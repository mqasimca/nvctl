//! Power command implementation
//!
//! Handles power status and limit commands.

use crate::cli::args::{OutputFormat, PowerArgs, PowerCommands};
use crate::cli::output::{print_output, Message, PowerStatus};
use crate::domain::PowerLimit;
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute power commands
pub fn run_power(
    args: &PowerArgs,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        PowerCommands::Status => run_power_status(&manager, format, gpu_index),
        PowerCommands::Limit { watts } => {
            run_power_limit(&manager, *watts, format, gpu_index, dry_run)
        }
    }
}

fn run_power_status(
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

        let usage = device.power_usage().map(|p| p.as_watts()).unwrap_or(0);
        let limit = device.power_limit().map(|p| p.as_watts()).unwrap_or(0);
        let constraints = device.power_constraints().ok();

        let status = PowerStatus {
            gpu_name: info.name,
            gpu_index: idx,
            current_usage_watts: usage,
            limit_watts: limit,
            min_limit_watts: constraints.as_ref().map(|c| c.min.as_watts()).unwrap_or(0),
            max_limit_watts: constraints.as_ref().map(|c| c.max.as_watts()).unwrap_or(0),
            default_limit_watts: constraints.map(|c| c.default.as_watts()).unwrap_or(0),
        };

        print_output(&status, format)?;
    }

    Ok(())
}

fn run_power_limit(
    manager: &NvmlManager,
    watts: u32,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let limit = PowerLimit::from_watts(watts);

    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for idx in indices {
        let mut device = manager.device_by_index(idx)?;
        let info = device.info()?;

        // Validate against constraints
        if let Ok(constraints) = device.power_constraints() {
            if let Err(e) = limit.validate(&constraints) {
                let message = Message {
                    message: format!(
                        "Cannot set power limit on GPU {}: {}. Valid range: {}-{}W",
                        info.name,
                        e,
                        constraints.min.as_watts(),
                        constraints.max.as_watts()
                    ),
                    success: false,
                };
                print_output(&message, format)?;
                continue;
            }
        }

        let message = if dry_run {
            format!(
                "[DRY RUN] Would set power limit to {}W on GPU {}",
                watts, info.name
            )
        } else {
            device.set_power_limit(limit)?;
            format!("Set power limit to {}W on GPU {}", watts, info.name)
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
