//! Fan command implementation
//!
//! Handles fan status, policy, and speed commands.

use crate::cli::args::{FanArgs, FanCommands, FanPolicyArg, OutputFormat};
use crate::cli::output::{print_output, FanInfo, FanStatus, Message};
use crate::domain::{FanPolicy, FanSpeed};
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute fan commands
pub fn run_fan(
    args: &FanArgs,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        FanCommands::Status => run_fan_status(&manager, format, gpu_index),
        FanCommands::Policy { policy } => {
            run_fan_policy(&manager, *policy, format, gpu_index, dry_run)
        }
        FanCommands::Speed { speed, fan_index } => {
            run_fan_speed(&manager, *speed, *fan_index, format, gpu_index, dry_run)
        }
    }
}

fn run_fan_status(
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
        let fan_count = device.fan_count().unwrap_or(0);

        let mut fans = Vec::new();
        for fan_idx in 0..fan_count {
            let speed = device
                .fan_speed(fan_idx)
                .map(|s| s.as_percentage())
                .unwrap_or(0);
            let policy = device
                .fan_policy(fan_idx)
                .map(|p| p.to_string())
                .unwrap_or_else(|_| "Unknown".to_string());

            fans.push(FanInfo {
                index: fan_idx,
                speed,
                policy,
            });
        }

        let status = FanStatus {
            gpu_name: info.name,
            gpu_index: idx,
            fans,
        };

        print_output(&status, format)?;
    }

    Ok(())
}

fn run_fan_policy(
    manager: &NvmlManager,
    policy_arg: FanPolicyArg,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let policy = match policy_arg {
        FanPolicyArg::Auto => FanPolicy::Auto,
        FanPolicyArg::Manual => FanPolicy::Manual,
    };

    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for idx in indices {
        let mut device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let fan_count = device.fan_count().unwrap_or(0);

        for fan_idx in 0..fan_count {
            let message = if dry_run {
                format!(
                    "[DRY RUN] Would set fan {} policy to {} on GPU {}",
                    fan_idx, policy, info.name
                )
            } else {
                device.set_fan_policy(fan_idx, policy)?;
                format!(
                    "Set fan {} policy to {} on GPU {}",
                    fan_idx, policy, info.name
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
    }

    Ok(())
}

fn run_fan_speed(
    manager: &NvmlManager,
    speed: u8,
    fan_index: Option<u32>,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let fan_speed = FanSpeed::new(speed)?;

    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for idx in indices {
        let mut device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let fan_count = device.fan_count().unwrap_or(0);

        let fan_indices: Vec<u32> = if let Some(fi) = fan_index {
            vec![fi]
        } else {
            (0..fan_count).collect()
        };

        for fan_idx in fan_indices {
            let message = if dry_run {
                format!(
                    "[DRY RUN] Would set fan {} speed to {}% on GPU {}",
                    fan_idx,
                    fan_speed.as_percentage(),
                    info.name
                )
            } else {
                device.set_fan_speed(fan_idx, fan_speed)?;
                format!(
                    "Set fan {} speed to {}% on GPU {}",
                    fan_idx,
                    fan_speed.as_percentage(),
                    info.name
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
    }

    Ok(())
}
