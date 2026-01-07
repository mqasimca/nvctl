//! Info command implementation
//!
//! Shows detailed GPU information.

use crate::cli::args::{InfoArgs, OutputFormat};
use crate::cli::output::{
    print_output, EccStatus, FanInfo, FanStatus, MemoryTempStatus, PcieStatus, PowerStatus,
    ThermalStatus, VideoStatus,
};
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
    let show_all = args.all
        || (!args.fan
            && !args.power
            && !args.thermal
            && !args.ecc
            && !args.pcie
            && !args.memory_temp
            && !args.video);
    let show_fan = show_all || args.fan;
    let show_power = show_all || args.power;
    let show_thermal = show_all || args.thermal;
    let show_ecc = show_all || args.ecc;
    let show_pcie = show_all || args.pcie;
    let show_memory_temp = show_all || args.memory_temp;
    let show_video = show_all || args.video;

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
                max_threshold: thresholds.and_then(|t| t.gpu_max.map(|v| v.as_celsius())),
            };

            print_output(&thermal_status, format)?;
        }

        if show_ecc {
            let ecc_mode = device.ecc_mode().ok().flatten();
            let ecc_enabled = ecc_mode.is_some();
            let ecc_errors = device.ecc_errors().ok().flatten();

            let ecc_status = EccStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                ecc_enabled,
                correctable_current: ecc_errors.as_ref().map(|e| e.correctable_current),
                correctable_aggregate: ecc_errors.as_ref().map(|e| e.correctable_lifetime),
                uncorrectable_current: ecc_errors.as_ref().map(|e| e.uncorrectable_current),
                uncorrectable_aggregate: ecc_errors.as_ref().map(|e| e.uncorrectable_lifetime),
                health_status: ecc_errors.as_ref().map(|e| {
                    // Use 1 hour uptime as a reasonable estimate for health calculation
                    let uptime_seconds = 3600;
                    format!("{:?}", e.health_status(uptime_seconds))
                }),
            };

            print_output(&ecc_status, format)?;
        }

        if show_pcie {
            let pcie_metrics = device.pcie_metrics().ok();

            if let Some(metrics) = pcie_metrics {
                let pcie_status = PcieStatus {
                    gpu_name: info.name.clone(),
                    gpu_index: idx,
                    current_gen: format!("{}", metrics.link_status.current_generation),
                    max_gen: format!("{}", metrics.link_status.max_generation),
                    current_width: format!("{}", metrics.link_status.current_width),
                    max_width: format!("{}", metrics.link_status.max_width),
                    tx_throughput_mbs: Some(
                        metrics.throughput.tx_bytes_per_sec() as f64 / 1024.0 / 1024.0,
                    ),
                    rx_throughput_mbs: Some(
                        metrics.throughput.rx_bytes_per_sec() as f64 / 1024.0 / 1024.0,
                    ),
                    replay_counter: metrics.replay_counter.count() as u32,
                    bandwidth_efficiency: Some(metrics.link_status.bandwidth_efficiency_percent()),
                };

                print_output(&pcie_status, format)?;
            }
        }

        if show_memory_temp {
            let gpu_temp = device.temperature().map(|t| t.as_celsius()).unwrap_or(0);
            let memory_temp = device
                .memory_temperature()
                .ok()
                .flatten()
                .map(|t| t.as_celsius());

            let mem_temp_status = MemoryTempStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                gpu_temp,
                memory_temp,
            };

            print_output(&mem_temp_status, format)?;
        }

        if show_video {
            let encoder_util = device
                .encoder_utilization()
                .ok()
                .flatten()
                .map(|u| u.percent() as u32);
            let decoder_util = device
                .decoder_utilization()
                .ok()
                .flatten()
                .map(|u| u.percent() as u32);

            let video_status = VideoStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                encoder_util,
                decoder_util,
            };

            print_output(&video_status, format)?;
        }

        println!(); // Separator between GPUs
    }

    Ok(())
}
