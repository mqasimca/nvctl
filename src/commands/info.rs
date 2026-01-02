//! Info command implementation
//!
//! Shows detailed GPU information.

use crate::cli::args::{InfoArgs, OutputFormat};
use crate::cli::output::{print_output, FanInfo, FanStatus, PowerStatus, ThermalStatus};
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the info command
pub fn run_info(args: &InfoArgs, format: OutputFormat, gpu_index: Option<u32>) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Determine which GPUs to show info for
    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    // Determine what info to show
    let show_all = args.all || (!args.fan && !args.power && !args.thermal);
    let show_fan = show_all || args.fan;
    let show_power = show_all || args.power;
    let show_thermal = show_all || args.thermal;

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        if show_fan {
            let mut fans = Vec::new();
            let fan_count = device.fan_count().unwrap_or(0);

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

            let fan_status = FanStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                fans,
            };

            print_output(&fan_status, format)?;
        }

        if show_power {
            let usage = device.power_usage().map(|p| p.as_watts()).unwrap_or(0);
            let limit = device.power_limit().map(|p| p.as_watts()).unwrap_or(0);
            let constraints = device.power_constraints().ok();

            let power_status = PowerStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                current_usage_watts: usage,
                limit_watts: limit,
                min_limit_watts: constraints.as_ref().map(|c| c.min.as_watts()).unwrap_or(0),
                max_limit_watts: constraints.as_ref().map(|c| c.max.as_watts()).unwrap_or(0),
                default_limit_watts: constraints.map(|c| c.default.as_watts()).unwrap_or(0),
            };

            print_output(&power_status, format)?;
        }

        if show_thermal {
            let temp = device.temperature().map(|t| t.as_celsius()).unwrap_or(0);
            let thresholds = device.thermal_thresholds().ok();

            let thermal_status = ThermalStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                current_temp: temp,
                shutdown_threshold: thresholds
                    .as_ref()
                    .and_then(|t| t.shutdown.map(|v| v.as_celsius())),
                slowdown_threshold: thresholds
                    .as_ref()
                    .and_then(|t| t.slowdown.map(|v| v.as_celsius())),
                max_threshold: thresholds
                    .and_then(|t| t.gpu_max.map(|v| v.as_celsius())),
            };

            print_output(&thermal_status, format)?;
        }

        println!(); // Separator between GPUs
    }

    Ok(())
}
