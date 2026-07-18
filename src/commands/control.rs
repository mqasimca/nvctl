//! Control command implementation
//!
//! Runs the main control loop for continuous GPU management.

use crate::cli::args::{ControlArgs, OutputFormat};
use crate::cli::output::{print_output, Message};
use crate::commands::selection::{select_gpu_indices, GpuSelector};
use crate::config::{Config, FanConfig};
use crate::domain::{FanCurve, FanCurvePoint, FanPolicy, FanSpeed, PowerLimit, Temperature};
use crate::error::{AppError, ConfigError, DomainError, Result};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct ControlTarget {
    selected_index: u32,
    gpu_uuid: String,
}

struct ControlTickPlan {
    label: String,
    gpu_uuid: String,
    target_speed: FanSpeed,
    fan_speeds: Vec<(u32, FanSpeed)>,
    old_power_limit: Option<PowerLimit>,
    old_acoustic_limit: Option<Temperature>,
}

enum ControlRollback {
    FanSpeed(String, u32, FanSpeed),
    PowerLimit(String, PowerLimit),
    AcousticLimit(String, Temperature),
}

struct FanPolicyGuard<'manager, M: GpuManager> {
    manager: &'manager M,
    original_fans: Vec<OriginalFanState>,
}

#[derive(Debug, Clone)]
struct OriginalFanState {
    gpu_uuid: String,
    fan_index: u32,
    policy: FanPolicy,
    speed: FanSpeed,
}

impl<'manager, M: GpuManager> FanPolicyGuard<'manager, M> {
    fn new(manager: &'manager M) -> Self {
        Self {
            manager,
            original_fans: Vec::new(),
        }
    }

    fn record_original(&mut self, original: OriginalFanState) {
        if !self
            .original_fans
            .iter()
            .any(|fan| fan.gpu_uuid == original.gpu_uuid && fan.fan_index == original.fan_index)
        {
            self.original_fans.push(original);
        }
    }

    fn restore(&mut self) -> Result<()> {
        let mut first_error = None;

        let original_fans = std::mem::take(&mut self.original_fans);
        let mut failed = Vec::new();
        for original in original_fans.into_iter().rev() {
            let result = self
                .manager
                .device_by_uuid(&original.gpu_uuid)
                .and_then(|mut device| {
                    device.set_fan_policy(original.fan_index, original.policy)?;
                    if original.policy == FanPolicy::Manual {
                        device.set_fan_speed(original.fan_index, original.speed)?;
                    }
                    Ok(())
                });

            if let Err(error) = result {
                log::error!(
                    "Failed to restore GPU {} fan {} to its original state: {}",
                    original.gpu_uuid,
                    original.fan_index,
                    error
                );
                first_error.get_or_insert(error);
                failed.push(original);
            }
        }

        self.original_fans = failed;

        first_error.map_or(Ok(()), |error| Err(error.into()))
    }
}

impl<M: GpuManager> Drop for FanPolicyGuard<'_, M> {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            log::error!("Failed to restore all fan policies: {}", error);
        }
    }
}

/// Execute the control command
pub fn run_control(
    args: &ControlArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    config: &Config,
) -> Result<()> {
    let manager = NvmlManager::new()?;
    let dry_run = config.general.dry_run;
    let verbose = config.general.verbose;
    let retry = args.retry || config.general.retry;

    // Parse fan curve from speed pairs
    let curve = parse_fan_curve(args, &config.fan)?;

    // Parse power limit if provided
    let power_limit = args
        .power_limit
        .or(config.power.limit_watts)
        .map(PowerLimit::try_from_watts)
        .transpose()?;
    let acoustic_limit = config.thermal.acoustic_limit_celsius.map(Temperature::new);

    let interval = Duration::from_secs(args.interval.unwrap_or(config.general.interval_seconds));
    let retry_interval = Duration::from_secs(
        args.retry_interval
            .unwrap_or(config.general.retry_interval_seconds),
    );
    if interval.is_zero() || retry_interval.is_zero() {
        return Err(ConfigError::InvalidValue {
            key: "general.interval_seconds".to_string(),
            message: "control and retry intervals must be greater than zero".to_string(),
        }
        .into());
    }

    // Determine which GPUs to control
    let indices = select_gpu_indices(&manager, selector)?;
    let targets = resolve_control_targets(&manager, &indices)?;
    validate_control_targets(
        &manager,
        &targets,
        power_limit.as_ref(),
        acoustic_limit.as_ref(),
    )?;

    let running = Arc::new(AtomicBool::new(true));
    if !args.single_use {
        let running_for_signal = Arc::clone(&running);
        ctrlc::set_handler(move || running_for_signal.store(false, Ordering::SeqCst)).map_err(
            |error| {
                AppError::Io(std::io::Error::other(format!(
                    "Failed to install shutdown handler: {}",
                    error
                )))
            },
        )?;
    }

    if verbose {
        log::info!("Starting control loop");
        log::info!("  Interval: {:?}", interval);
        log::info!("  Single use: {}", args.single_use);
        log::info!("  Dry run: {}", dry_run);
        log::info!("  GPUs: {:?}", targets);
        log::info!("  Fan curve: {:?}", curve.points());
    }

    // Initial setup: set fan policy to manual. The guard restores every fan
    // on normal completion, Ctrl-C, and all error-return paths.
    let mut policy_guard = FanPolicyGuard::new(&manager);
    if !dry_run {
        acquire_manual_fans(&manager, &targets, &mut policy_guard)?;
    }

    while running.load(Ordering::SeqCst) {
        match control_tick(
            &manager,
            &targets,
            &curve,
            power_limit.as_ref(),
            acoustic_limit.as_ref(),
            dry_run,
            verbose,
        ) {
            Ok(()) => {}
            Err(e) => {
                log::error!("Control tick failed: {}", e);

                if retry {
                    if !dry_run {
                        if let Err(restore_error) = policy_guard.restore() {
                            log::error!(
                                "Failed to restore all fans to their original state while retrying: {}",
                                restore_error
                            );
                            return Err(restore_error);
                        }
                    }
                    log::info!("Retrying in {:?}...", retry_interval);
                    wait_while_running(&running, retry_interval);
                    if running.load(Ordering::SeqCst) && !dry_run {
                        acquire_manual_fans(&manager, &targets, &mut policy_guard)?;
                    }
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

        wait_while_running(&running, interval);
    }

    policy_guard.restore()?;

    if !args.single_use {
        print_output(
            &Message {
                message: "Control loop stopped; original fan control restored".to_string(),
                success: true,
            },
            format,
        )?;
    }

    Ok(())
}

fn resolve_control_targets<M: GpuManager>(
    manager: &M,
    indices: &[u32],
) -> Result<Vec<ControlTarget>> {
    indices
        .iter()
        .map(|selected_index| {
            let device = manager.device_by_index(*selected_index)?;
            Ok(ControlTarget {
                selected_index: *selected_index,
                gpu_uuid: device.uuid()?,
            })
        })
        .collect()
}

fn validate_control_targets<M: GpuManager>(
    manager: &M,
    targets: &[ControlTarget],
    power_limit: Option<&PowerLimit>,
    acoustic_limit: Option<&Temperature>,
) -> Result<()> {
    for target in targets {
        let device = manager.device_by_uuid(&target.gpu_uuid)?;
        if device.fan_count()? == 0 {
            return Err(crate::error::NvmlError::FanControlNotAvailable.into());
        }
        if let Some(limit) = power_limit {
            limit.validate(&device.power_constraints()?)?;
        }
        if let Some(limit) = acoustic_limit {
            let limits = device.acoustic_limits()?;
            if limits.current.is_none() || limits.min.is_none() || limits.max.is_none() {
                return Err(crate::error::NvmlError::NotSupported(format!(
                    "GPU {} does not expose complete acoustic temperature limits",
                    target.gpu_uuid
                ))
                .into());
            }
            if !limits.is_valid(*limit) {
                return Err(DomainError::InvalidTemperature(limit.as_celsius()).into());
            }
        }
    }
    Ok(())
}

fn acquire_manual_fans<'manager, M: GpuManager>(
    manager: &'manager M,
    targets: &[ControlTarget],
    guard: &mut FanPolicyGuard<'manager, M>,
) -> Result<()> {
    for target in targets {
        let mut device = manager.device_by_uuid(&target.gpu_uuid)?;
        for fan_index in 0..device.fan_count()? {
            let original = OriginalFanState {
                gpu_uuid: target.gpu_uuid.clone(),
                fan_index,
                policy: device.fan_policy(fan_index)?,
                speed: device.fan_speed(fan_index)?,
            };
            device.set_fan_policy(fan_index, FanPolicy::Manual)?;
            guard.record_original(original);
        }
    }
    Ok(())
}

fn wait_while_running(running: &AtomicBool, duration: Duration) {
    let Some(deadline) = Instant::now().checked_add(duration) else {
        while running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(100));
        }
        return;
    };
    while running.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(100)));
    }
}

/// Execute a single control tick
fn control_tick<M: GpuManager>(
    manager: &M,
    targets: &[ControlTarget],
    curve: &FanCurve,
    power_limit: Option<&PowerLimit>,
    acoustic_limit: Option<&Temperature>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // Read and validate every target before mutating any of them. This keeps a
    // transient failure on one GPU from leaving earlier GPUs partially changed.
    let mut plans = Vec::with_capacity(targets.len());
    for target in targets {
        let device = manager.device_by_uuid(&target.gpu_uuid)?;
        let label = format!("{} ({})", target.selected_index, target.gpu_uuid);
        let temp = device.temperature()?.as_celsius();
        let target_speed = curve.speed_for_temperature(temp);
        if verbose {
            log::info!(
                "GPU {}: temp={}°C, target_speed={}%",
                label,
                temp,
                target_speed.as_percentage()
            );
        }

        let fan_count = device.fan_count()?;
        if fan_count == 0 {
            return Err(crate::error::NvmlError::FanControlNotAvailable.into());
        }
        let fan_speeds = (0..fan_count)
            .map(|fan_index| device.fan_speed(fan_index).map(|speed| (fan_index, speed)))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let old_power_limit = if let Some(limit) = power_limit {
            limit.validate(&device.power_constraints()?)?;
            Some(device.power_limit()?)
        } else {
            None
        };

        let old_acoustic_limit = if let Some(limit) = acoustic_limit {
            let limits = device.acoustic_limits()?;
            if !limits.is_valid(*limit) {
                return Err(DomainError::InvalidTemperature(limit.as_celsius()).into());
            }
            Some(limits.current.ok_or_else(|| {
                crate::error::NvmlError::NotSupported(format!(
                    "GPU {} does not expose its current acoustic temperature limit",
                    target.gpu_uuid
                ))
            })?)
        } else {
            None
        };

        plans.push(ControlTickPlan {
            label,
            gpu_uuid: target.gpu_uuid.clone(),
            target_speed,
            fan_speeds,
            old_power_limit,
            old_acoustic_limit,
        });
    }

    if dry_run {
        for plan in plans {
            for (fan_index, _) in plan.fan_speeds {
                log::info!(
                    "[DRY RUN] Would set GPU {} fan {} to {}%",
                    plan.label,
                    fan_index,
                    plan.target_speed.as_percentage()
                );
            }
            if let Some(limit) = power_limit {
                log::info!(
                    "[DRY RUN] Would set GPU {} power limit to {}W",
                    plan.label,
                    limit.as_watts()
                );
            }
            if let Some(limit) = acoustic_limit {
                log::info!(
                    "[DRY RUN] Would set GPU {} acoustic limit to {}°C",
                    plan.label,
                    limit.as_celsius()
                );
            }
        }
        return Ok(());
    }

    let mut rollback = Vec::new();
    let result = (|| {
        for plan in plans {
            let mut device = manager.device_by_uuid(&plan.gpu_uuid)?;
            for (fan_index, old_speed) in plan.fan_speeds {
                device.set_fan_speed(fan_index, plan.target_speed)?;
                rollback.push(ControlRollback::FanSpeed(
                    plan.gpu_uuid.clone(),
                    fan_index,
                    old_speed,
                ));
            }
            if let (Some(limit), Some(old_limit)) = (power_limit, plan.old_power_limit) {
                device.set_power_limit(*limit)?;
                rollback.push(ControlRollback::PowerLimit(
                    plan.gpu_uuid.clone(),
                    old_limit,
                ));
            }
            if let (Some(limit), Some(old_limit)) = (acoustic_limit, plan.old_acoustic_limit) {
                device.set_acoustic_limit(*limit)?;
                rollback.push(ControlRollback::AcousticLimit(plan.gpu_uuid, old_limit));
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        rollback_control_tick(manager, rollback);
        return Err(error);
    }
    Ok(())
}

fn rollback_control_tick<M: GpuManager>(manager: &M, rollback: Vec<ControlRollback>) {
    for operation in rollback.into_iter().rev() {
        let (gpu_uuid, result) = match operation {
            ControlRollback::FanSpeed(gpu_uuid, fan_index, speed) => {
                let result = manager
                    .device_by_uuid(&gpu_uuid)
                    .and_then(|mut device| device.set_fan_speed(fan_index, speed));
                (gpu_uuid, result)
            }
            ControlRollback::PowerLimit(gpu_uuid, limit) => {
                let result = manager
                    .device_by_uuid(&gpu_uuid)
                    .and_then(|mut device| device.set_power_limit(limit));
                (gpu_uuid, result)
            }
            ControlRollback::AcousticLimit(gpu_uuid, limit) => {
                let result = manager
                    .device_by_uuid(&gpu_uuid)
                    .and_then(|mut device| device.set_acoustic_limit(limit));
                (gpu_uuid, result)
            }
        };
        if let Err(error) = result {
            log::error!(
                "Failed to roll back GPU {} control setting: {}",
                gpu_uuid,
                error
            );
        }
    }
}

/// Parse fan curve from command line arguments
fn parse_fan_curve(args: &ControlArgs, config: &FanConfig) -> Result<FanCurve> {
    if args.speed_pairs.is_empty() {
        return config.to_fan_curve().map_err(AppError::Domain);
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

    let default_speed = FanSpeed::new(args.default_speed.unwrap_or(config.default_speed))?;
    FanCurve::new(points, default_speed).map_err(AppError::Domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockManager;

    #[test]
    fn test_parse_fan_curve_empty() {
        let args = ControlArgs {
            interval: Some(5),
            single_use: false,
            retry: false,
            retry_interval: Some(10),
            speed_pairs: vec![],
            default_speed: Some(30),
            power_limit: None,
        };

        let curve = parse_fan_curve(&args, &FanConfig::default()).unwrap();
        assert!(!curve.points().is_empty()); // Default curve
    }

    #[test]
    fn test_parse_fan_curve_valid() {
        let args = ControlArgs {
            interval: Some(5),
            single_use: false,
            retry: false,
            retry_interval: Some(10),
            speed_pairs: vec![
                "40:30".to_string(),
                "60:50".to_string(),
                "80:100".to_string(),
            ],
            default_speed: Some(20),
            power_limit: None,
        };

        let curve = parse_fan_curve(&args, &FanConfig::default()).unwrap();
        assert_eq!(curve.points().len(), 3);
        assert_eq!(curve.default_speed().as_percentage(), 20);
    }

    #[test]
    fn test_parse_fan_curve_invalid_format() {
        let args = ControlArgs {
            interval: Some(5),
            single_use: false,
            retry: false,
            retry_interval: Some(10),
            speed_pairs: vec!["invalid".to_string()],
            default_speed: Some(30),
            power_limit: None,
        };

        assert!(parse_fan_curve(&args, &FanConfig::default()).is_err());
    }

    #[test]
    fn test_parse_fan_curve_invalid_speed() {
        let args = ControlArgs {
            interval: Some(5),
            single_use: false,
            retry: false,
            retry_interval: Some(10),
            speed_pairs: vec!["60:150".to_string()], // Speed > 100
            default_speed: Some(30),
            power_limit: None,
        };

        assert!(parse_fan_curve(&args, &FanConfig::default()).is_err());
    }

    #[test]
    fn control_targets_and_policy_restoration_use_stable_uuids() {
        let manager = MockManager::new(1);
        let targets = resolve_control_targets(&manager, &[0]).unwrap();
        assert_eq!(targets[0].gpu_uuid, "GPU-MOCK-0000");

        let mut guard = FanPolicyGuard::new(&manager);
        acquire_manual_fans(&manager, &targets, &mut guard).unwrap();
        let device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        for fan_index in 0..device.fan_count().unwrap() {
            assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Manual);
        }

        guard.restore().unwrap();
        let device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        for fan_index in 0..device.fan_count().unwrap() {
            assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Auto);
        }
    }

    #[test]
    fn control_restores_preexisting_manual_policy_and_speed() {
        let manager = MockManager::new(1);
        let mut device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        let original_speed = FanSpeed::new(42).unwrap();
        device.set_fan_policy(0, FanPolicy::Manual).unwrap();
        device.set_fan_speed(0, original_speed).unwrap();
        drop(device);

        let targets = resolve_control_targets(&manager, &[0]).unwrap();
        let mut guard = FanPolicyGuard::new(&manager);
        acquire_manual_fans(&manager, &targets, &mut guard).unwrap();
        manager
            .device_by_uuid("GPU-MOCK-0000")
            .unwrap()
            .set_fan_speed(0, FanSpeed::new(90).unwrap())
            .unwrap();

        guard.restore().unwrap();
        let device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Manual);
        assert_eq!(device.fan_speed(0).unwrap(), original_speed);
    }
}
