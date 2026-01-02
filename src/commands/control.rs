//! Control command implementation
//!
//! Runs the main control loop for continuous GPU management.

use crate::cli::args::{ControlArgs, OutputFormat};
use crate::cli::output::{print_output, Message};
use crate::domain::{FanCurve, FanCurvePoint, FanPolicy, FanSpeed, PowerLimit};
use crate::error::{AppError, DomainError, Result};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

use std::thread;
use std::time::Duration;

/// Execute the control command
pub fn run_control(
    args: &ControlArgs,
    format: OutputFormat,
    gpu_index: Option<u32>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Parse fan curve from speed pairs
    let curve = parse_fan_curve(args)?;

    // Parse power limit if provided
    let power_limit = args.power_limit.map(PowerLimit::from_watts);

    let interval = Duration::from_secs(args.interval);
    let retry_interval = Duration::from_secs(args.retry_interval);

    // Determine which GPUs to control
    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    if verbose {
        log::info!("Starting control loop");
        log::info!("  Interval: {:?}", interval);
        log::info!("  Single use: {}", args.single_use);
        log::info!("  Dry run: {}", dry_run);
        log::info!("  GPUs: {:?}", indices);
        log::info!("  Fan curve: {:?}", curve.points());
    }

    // Initial setup: set fan policy to manual
    if !dry_run {
        for &idx in &indices {
            let mut device = manager.device_by_index(idx)?;
            let fan_count = device.fan_count().unwrap_or(0);
            for fan_idx in 0..fan_count {
                if let Err(e) = device.set_fan_policy(fan_idx, FanPolicy::Manual) {
                    log::warn!(
                        "Failed to set fan policy on GPU {} fan {}: {}",
                        idx,
                        fan_idx,
                        e
                    );
                }
            }
        }
    }

    loop {
        match control_tick(
            &manager,
            &indices,
            &curve,
            power_limit.as_ref(),
            dry_run,
            verbose,
        ) {
            Ok(()) => {}
            Err(e) => {
                log::error!("Control tick failed: {}", e);

                if args.retry {
                    log::info!("Retrying in {:?}...", retry_interval);
                    thread::sleep(retry_interval);
                    continue;
                } else {
                    return Err(e);
                }
            }
        }

        if args.single_use {
            let msg = Message {
                message: "Control tick completed (single-use mode)".to_string(),
                success: true,
            };
            print_output(&msg, format)?;
            break;
        }

        thread::sleep(interval);
    }

    Ok(())
}

/// Execute a single control tick
fn control_tick(
    manager: &NvmlManager,
    indices: &[u32],
    curve: &FanCurve,
    power_limit: Option<&PowerLimit>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    for &idx in indices {
        let mut device = manager.device_by_index(idx)?;

        // Get current temperature
        let temp = device.temperature()?.as_celsius();

        // Calculate target fan speed from curve
        let target_speed = curve.speed_for_temperature(temp);

        if verbose {
            log::info!(
                "GPU {}: temp={}°C, target_speed={}%",
                idx,
                temp,
                target_speed.as_percentage()
            );
        }

        // Apply fan speed
        let fan_count = device.fan_count().unwrap_or(0);
        for fan_idx in 0..fan_count {
            if dry_run {
                log::info!(
                    "[DRY RUN] Would set GPU {} fan {} to {}%",
                    idx,
                    fan_idx,
                    target_speed.as_percentage()
                );
            } else if let Err(e) = device.set_fan_speed(fan_idx, target_speed) {
                log::warn!(
                    "Failed to set fan speed on GPU {} fan {}: {}",
                    idx,
                    fan_idx,
                    e
                );
            }
        }

        // Apply power limit if specified
        if let Some(limit) = power_limit {
            if dry_run {
                log::info!(
                    "[DRY RUN] Would set GPU {} power limit to {}W",
                    idx,
                    limit.as_watts()
                );
            } else if let Err(e) = device.set_power_limit(*limit) {
                log::warn!("Failed to set power limit on GPU {}: {}", idx, e);
            }
        }
    }

    Ok(())
}

/// Parse fan curve from command line arguments
fn parse_fan_curve(args: &ControlArgs) -> Result<FanCurve> {
    if args.speed_pairs.is_empty() {
        // Use default curve
        return Ok(FanCurve::default());
    }

    let mut points = Vec::with_capacity(args.speed_pairs.len());

    for pair in &args.speed_pairs {
        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(AppError::Domain(DomainError::InvalidFanCurve(format!(
                "Invalid speed pair format: '{}'. Expected TEMP:SPEED (e.g., 60:50)",
                pair
            ))));
        }

        let temp: i32 = parts[0].parse().map_err(|_| {
            AppError::Domain(DomainError::InvalidFanCurve(format!(
                "Invalid temperature in '{}': not a number",
                pair
            )))
        })?;

        let speed: u8 = parts[1].parse().map_err(|_| {
            AppError::Domain(DomainError::InvalidFanCurve(format!(
                "Invalid speed in '{}': not a number",
                pair
            )))
        })?;

        let fan_speed = FanSpeed::new(speed)?;
        points.push(FanCurvePoint::new(temp, fan_speed));
    }

    let default_speed = FanSpeed::new(args.default_speed)?;
    FanCurve::new(points, default_speed).map_err(AppError::Domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fan_curve_empty() {
        let args = ControlArgs {
            interval: 5,
            single_use: false,
            retry: false,
            retry_interval: 10,
            speed_pairs: vec![],
            default_speed: 30,
            power_limit: None,
        };

        let curve = parse_fan_curve(&args).unwrap();
        assert!(!curve.points().is_empty()); // Default curve
    }

    #[test]
    fn test_parse_fan_curve_valid() {
        let args = ControlArgs {
            interval: 5,
            single_use: false,
            retry: false,
            retry_interval: 10,
            speed_pairs: vec![
                "40:30".to_string(),
                "60:50".to_string(),
                "80:100".to_string(),
            ],
            default_speed: 20,
            power_limit: None,
        };

        let curve = parse_fan_curve(&args).unwrap();
        assert_eq!(curve.points().len(), 3);
        assert_eq!(curve.default_speed().as_percentage(), 20);
    }

    #[test]
    fn test_parse_fan_curve_invalid_format() {
        let args = ControlArgs {
            interval: 5,
            single_use: false,
            retry: false,
            retry_interval: 10,
            speed_pairs: vec!["invalid".to_string()],
            default_speed: 30,
            power_limit: None,
        };

        assert!(parse_fan_curve(&args).is_err());
    }

    #[test]
    fn test_parse_fan_curve_invalid_speed() {
        let args = ControlArgs {
            interval: 5,
            single_use: false,
            retry: false,
            retry_interval: 10,
            speed_pairs: vec!["60:150".to_string()], // Speed > 100
            default_speed: 30,
            power_limit: None,
        };

        assert!(parse_fan_curve(&args).is_err());
    }
}
