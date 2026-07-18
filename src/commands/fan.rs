//! Fan command implementation
//!
//! Handles fan status, policy, and speed commands.

use crate::cli::args::{FanArgs, FanCommands, FanPolicyArg, OutputFormat};
use crate::cli::output::{FanInfo, FanStatus, Message, OutputWriter};
use crate::commands::selection::{select_gpu_indices, GpuSelector};
use crate::domain::{FanPolicy, FanSpeed};
use crate::error::{NvmlError, Result};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute fan commands
pub fn run_fan(
    args: &FanArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        FanCommands::Status => run_fan_status(&manager, format, selector),
        FanCommands::Policy { policy } => {
            run_fan_policy(&manager, *policy, format, selector, dry_run)
        }
        FanCommands::Speed { speed, fan_index } => {
            run_fan_speed(&manager, *speed, *fan_index, format, selector, dry_run)
        }
    }
}

fn run_fan_status(
    manager: &NvmlManager,
    format: OutputFormat,
    selector: &GpuSelector,
) -> Result<()> {
    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let fan_count = device.fan_count()?;

        let mut fans = Vec::new();
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

        let status = FanStatus {
            gpu_name: info.name,
            gpu_index: idx,
            fans,
        };

        output.write(&status)?;
    }

    output.finish()?;
    Ok(())
}

fn run_fan_policy<M: GpuManager>(
    manager: &M,
    policy_arg: FanPolicyArg,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let policy = match policy_arg {
        FanPolicyArg::Auto => FanPolicy::Auto,
        FanPolicyArg::Manual => FanPolicy::Manual,
    };

    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);
    let mut targets = Vec::new();

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let fan_count = device.fan_count()?;
        if fan_count == 0 {
            return Err(NvmlError::FanControlNotAvailable.into());
        }

        for fan_idx in 0..fan_count {
            targets.push((
                idx,
                info.uuid.clone(),
                fan_idx,
                info.name.clone(),
                device.fan_policy(fan_idx)?,
                device.fan_speed(fan_idx)?,
            ));
        }
    }

    if !dry_run {
        let mut applied = Vec::new();
        for (_, gpu_uuid, fan_index, _, old_policy, old_speed) in &targets {
            let result = manager
                .device_by_uuid(gpu_uuid)
                .and_then(|mut device| device.set_fan_policy(*fan_index, policy));
            if let Err(error) = result {
                rollback_fan_policies(manager, &applied);
                return Err(error.into());
            }
            applied.push((gpu_uuid.clone(), *fan_index, *old_policy, *old_speed));
        }
    }

    for (_, _, fan_index, gpu_name, _, _) in targets {
        let message = if dry_run {
            format!("[DRY RUN] Would set fan {fan_index} policy to {policy} on GPU {gpu_name}")
        } else {
            format!("Set fan {fan_index} policy to {policy} on GPU {gpu_name}")
        };
        output.write(&Message {
            message,
            success: true,
        })?;
    }

    output.finish()?;
    Ok(())
}

fn run_fan_speed<M: GpuManager>(
    manager: &M,
    speed: u8,
    fan_index: Option<u32>,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let fan_speed = FanSpeed::new(speed)?;

    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);
    let mut targets = Vec::new();

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let fan_count = device.fan_count()?;
        if fan_count == 0 {
            return Err(NvmlError::FanControlNotAvailable.into());
        }

        if let Some(fi) = fan_index {
            if fi >= fan_count {
                return Err(NvmlError::InvalidArgument(format!(
                    "fan index {fi} is out of range for GPU {} ({fan_count} fans)",
                    info.name
                ))
                .into());
            }
        }

        let fan_indices: Vec<u32> = if let Some(fi) = fan_index {
            vec![fi]
        } else {
            (0..fan_count).collect()
        };

        for fan_idx in fan_indices {
            targets.push((
                idx,
                info.uuid.clone(),
                fan_idx,
                info.name.clone(),
                device.fan_policy(fan_idx)?,
                device.fan_speed(fan_idx)?,
            ));
        }
    }

    if !dry_run {
        let mut applied = Vec::new();
        for (_, gpu_uuid, fan_index, _, old_policy, old_speed) in &targets {
            let result = manager
                .device_by_uuid(gpu_uuid)
                .and_then(|mut device| device.set_fan_speed(*fan_index, fan_speed));
            if let Err(error) = result {
                rollback_fan_policies(manager, &applied);
                return Err(error.into());
            }
            applied.push((gpu_uuid.clone(), *fan_index, *old_policy, *old_speed));
        }
    }

    for (_, _, fan_index, gpu_name, _, _) in targets {
        let message = if dry_run {
            format!(
                "[DRY RUN] Would set fan {fan_index} speed to {}% on GPU {gpu_name}",
                fan_speed.as_percentage()
            )
        } else {
            format!(
                "Set fan {fan_index} speed to {}% on GPU {gpu_name}",
                fan_speed.as_percentage()
            )
        };
        output.write(&Message {
            message,
            success: true,
        })?;
    }

    output.finish()?;
    Ok(())
}

fn rollback_fan_policies<M: GpuManager>(
    manager: &M,
    applied: &[(String, u32, FanPolicy, FanSpeed)],
) {
    for (gpu_uuid, fan_index, old_policy, old_speed) in applied.iter().rev() {
        match manager.device_by_uuid(gpu_uuid) {
            Ok(mut device) => {
                if *old_policy == FanPolicy::Auto {
                    // Setting a speed implicitly selects Manual. Restore the
                    // captured speed first, then make Auto the final policy.
                    if let Err(error) = device.set_fan_speed(*fan_index, *old_speed) {
                        log::error!(
                            "Failed to roll back GPU {} Fan {} speed: {}",
                            gpu_uuid,
                            fan_index,
                            error
                        );
                    }
                }
                if let Err(error) = device.set_fan_policy(*fan_index, *old_policy) {
                    log::error!(
                        "Failed to roll back GPU {} Fan {} policy: {}",
                        gpu_uuid,
                        fan_index,
                        error
                    );
                }
                if *old_policy == FanPolicy::Manual {
                    if let Err(error) = device.set_fan_speed(*fan_index, *old_speed) {
                        log::error!(
                            "Failed to roll back GPU {} Fan {} speed: {}",
                            gpu_uuid,
                            fan_index,
                            error
                        );
                    }
                }
            }
            Err(error) => log::error!(
                "Failed to open GPU {} for fan policy rollback: {}",
                gpu_uuid,
                error
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockDevice, MockManager};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FailingUuidLookupManager {
        inner: MockManager,
        uuid_lookups: AtomicUsize,
        fail_on_lookup: usize,
    }

    impl GpuManager for FailingUuidLookupManager {
        type Device<'manager>
            = MockDevice
        where
            Self: 'manager;

        fn device_count(&self) -> std::result::Result<u32, NvmlError> {
            self.inner.device_count()
        }

        fn device_by_index(&self, index: u32) -> std::result::Result<Self::Device<'_>, NvmlError> {
            self.inner.device_by_index(index)
        }

        fn device_by_uuid(&self, uuid: &str) -> std::result::Result<Self::Device<'_>, NvmlError> {
            let lookup = self.uuid_lookups.fetch_add(1, Ordering::SeqCst);
            if lookup == self.fail_on_lookup {
                Err(NvmlError::GpuLost)
            } else {
                self.inner.device_by_uuid(uuid)
            }
        }

        fn device_by_name(&self, name: &str) -> std::result::Result<Self::Device<'_>, NvmlError> {
            self.inner.device_by_name(name)
        }

        fn driver_version(&self) -> std::result::Result<String, NvmlError> {
            self.inner.driver_version()
        }

        fn nvml_version(&self) -> std::result::Result<String, NvmlError> {
            self.inner.nvml_version()
        }
    }

    #[test]
    fn speed_failure_rolls_back_implicit_manual_policy_and_speed() {
        let manager = FailingUuidLookupManager {
            inner: MockManager::new(2),
            uuid_lookups: AtomicUsize::new(0),
            fail_on_lookup: 2,
        };

        assert!(run_fan_speed(
            &manager,
            80,
            None,
            OutputFormat::Compact,
            &GpuSelector::All,
            false,
        )
        .is_err());

        for gpu_index in 0..2 {
            let device = manager.device_by_index(gpu_index).unwrap();
            for fan_index in 0..device.fan_count().unwrap() {
                assert_eq!(device.fan_policy(fan_index).unwrap(), FanPolicy::Auto);
                assert_eq!(device.fan_speed(fan_index).unwrap().as_percentage(), 50);
            }
        }
    }
}
