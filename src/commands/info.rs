//! Info command implementation
//!
//! Shows detailed GPU information.

use crate::cli::args::{InfoArgs, OutputFormat};
use crate::cli::output::{
    EccStatus, FanInfo, FanStatus, MemoryTempStatus, OutputWriter, PcieStatus, PowerStatus,
    ThermalStatus, VideoStatus,
};
use crate::commands::selection::{select_gpu_indices, GpuSelector};
use crate::domain::EccMode;
use crate::error::{NvmlError, Result};
use crate::health::system_uptime_seconds;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the info command
pub fn run_info(args: &InfoArgs, format: OutputFormat, selector: &GpuSelector) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Determine which GPUs to show info for
    let indices = select_gpu_indices(&manager, selector)?;

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
    let mut output = OutputWriter::new(format);

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        if show_fan {
            let mut fans = Vec::new();
            let fan_count = device.fan_count()?;

            for fan_idx in 0..fan_count {
                let speed = device.fan_speed(fan_idx)?.as_percentage();
                let policy = match device.fan_policy(fan_idx) {
                    Ok(policy) => policy.to_string(),
                    Err(NvmlError::NotSupported(_)) => "Unavailable".to_string(),
                    Err(error) => return Err(error.into()),
                };

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

            output.write(&fan_status)?;
        }

        if show_power {
            let usage = device.power_usage()?.as_watts();
            let limit = device.power_limit()?.as_watts();
            let constraints = device.power_constraints()?;

            let power_status = PowerStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                current_usage_watts: usage,
                limit_watts: limit,
                min_limit_watts: constraints.min.as_watts(),
                max_limit_watts: constraints.max.as_watts(),
                default_limit_watts: constraints.default.as_watts(),
            };

            output.write(&power_status)?;
        }

        if show_thermal {
            let temp = device.temperature()?.as_celsius();
            let thresholds = device.thermal_thresholds()?;

            let thermal_status = ThermalStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                current_temp: temp,
                shutdown_threshold: thresholds.shutdown.map(|v| v.as_celsius()),
                slowdown_threshold: thresholds.slowdown.map(|v| v.as_celsius()),
                max_threshold: thresholds.gpu_max.map(|v| v.as_celsius()),
            };

            output.write(&thermal_status)?;
        }

        if show_ecc {
            let ecc_mode = device.ecc_mode()?;
            let ecc_enabled = matches!(ecc_mode, Some(EccMode::Enabled));
            let ecc_errors = if ecc_enabled {
                device.ecc_errors()?
            } else {
                None
            };

            let ecc_status = EccStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                ecc_enabled,
                correctable_current: ecc_errors.as_ref().map(|e| e.correctable_current),
                correctable_aggregate: ecc_errors.as_ref().map(|e| e.correctable_lifetime),
                uncorrectable_current: ecc_errors.as_ref().map(|e| e.uncorrectable_current),
                uncorrectable_aggregate: ecc_errors.as_ref().map(|e| e.uncorrectable_lifetime),
                health_status: ecc_errors.as_ref().map(|e| {
                    let uptime_seconds = system_uptime_seconds().unwrap_or(0);
                    format!("{:?}", e.health_status(uptime_seconds))
                }),
            };

            output.write(&ecc_status)?;
        }

        if show_pcie {
            let pcie_metrics = match device.pcie_metrics() {
                Ok(metrics) => Some(metrics),
                Err(NvmlError::NotSupported(_)) => None,
                Err(error) => return Err(error.into()),
            };

            let pcie_status = if let Some(metrics) = pcie_metrics {
                PcieStatus {
                    gpu_name: info.name.clone(),
                    gpu_index: idx,
                    current_gen: Some(format!("{}", metrics.link_status.current_generation)),
                    max_gen: Some(format!("{}", metrics.link_status.max_generation)),
                    current_width: Some(format!("{}", metrics.link_status.current_width)),
                    max_width: Some(format!("{}", metrics.link_status.max_width)),
                    tx_throughput_mbs: Some(metrics.throughput.tx_mbps()),
                    rx_throughput_mbs: Some(metrics.throughput.rx_mbps()),
                    replay_counter: Some(metrics.replay_counter.count()),
                    bandwidth_efficiency: Some(metrics.link_status.bandwidth_efficiency_percent()),
                }
            } else {
                PcieStatus {
                    gpu_name: info.name.clone(),
                    gpu_index: idx,
                    current_gen: None,
                    max_gen: None,
                    current_width: None,
                    max_width: None,
                    tx_throughput_mbs: None,
                    rx_throughput_mbs: None,
                    replay_counter: None,
                    bandwidth_efficiency: None,
                }
            };
            output.write(&pcie_status)?;
        }

        if show_memory_temp {
            let gpu_temp = device.temperature()?.as_celsius();
            let memory_temp = device.memory_temperature()?.map(|t| t.as_celsius());

            let mem_temp_status = MemoryTempStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                gpu_temp,
                memory_temp,
            };

            output.write(&mem_temp_status)?;
        }

        if show_video {
            let encoder_util = device.encoder_utilization()?.map(|u| u.percent() as u32);
            let decoder_util = device.decoder_utilization()?.map(|u| u.percent() as u32);

            let video_status = VideoStatus {
                gpu_name: info.name.clone(),
                gpu_index: idx,
                encoder_util,
                decoder_util,
            };

            output.write(&video_status)?;
        }
    }

    output.finish()?;
    Ok(())
}
