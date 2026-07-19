//! Power command implementation
//!
//! Handles power status and limit commands.

use crate::cli::args::{OutputFormat, PowerArgs, PowerCommands};
use crate::cli::output::{Message, OutputWriter, PowerStatus};
use crate::commands::selection::{resolve_gpu_uuids, select_gpu_indices, GpuSelector};
use crate::domain::PowerLimit;
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};
use crate::services::GpuMutationGuard;

/// Execute power commands
pub fn run_power(
    args: &PowerArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        PowerCommands::Status => run_power_status(&manager, format, selector),
        PowerCommands::Limit { watts } => {
            let indices = select_gpu_indices(&manager, selector)?;
            let target_uuids = resolve_gpu_uuids(&manager, &indices)?;
            let _mutation_guard = (!dry_run)
                .then(|| GpuMutationGuard::acquire(&target_uuids, "power limit"))
                .transpose()?;
            run_power_limit(&manager, *watts, format, &target_uuids, dry_run)
        }
    }
}

fn run_power_status(
    manager: &NvmlManager,
    format: OutputFormat,
    selector: &GpuSelector,
) -> Result<()> {
    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        let usage = device.power_usage()?.as_watts();
        let limit = device.power_limit()?.as_watts();
        let constraints = device.power_constraints()?;

        let status = PowerStatus {
            gpu_name: info.name,
            gpu_index: idx,
            current_usage_watts: usage,
            limit_watts: limit,
            min_limit_watts: constraints.min.as_watts(),
            max_limit_watts: constraints.max.as_watts(),
            default_limit_watts: constraints.default.as_watts(),
        };

        output.write(&status)?;
    }

    output.finish()?;
    Ok(())
}

fn run_power_limit<M: GpuManager>(
    manager: &M,
    watts: u32,
    format: OutputFormat,
    target_uuids: &[String],
    dry_run: bool,
) -> Result<()> {
    let limit = PowerLimit::try_from_watts(watts)?;

    let mut output = OutputWriter::new(format);
    let mut targets = Vec::with_capacity(target_uuids.len());

    // Validate every selected GPU before mutating any of them.
    for uuid in target_uuids {
        let device = manager.device_by_uuid(uuid)?;
        let idx = device.index();
        limit.validate(&device.power_constraints()?)?;
        let info = device.info()?;
        targets.push((idx, info.uuid, info.name, device.power_limit()?));
    }

    if !dry_run {
        let mut applied: Vec<(String, PowerLimit)> = Vec::new();
        for (_, uuid, _, old_limit) in &targets {
            let result = manager
                .device_by_uuid(uuid)
                .and_then(|mut device| device.set_power_limit(limit));
            if let Err(error) = result {
                for (rollback_uuid, rollback_limit) in applied.into_iter().rev() {
                    match manager.device_by_uuid(&rollback_uuid) {
                        Ok(mut rollback_device) => {
                            if let Err(rollback_error) =
                                rollback_device.set_power_limit(rollback_limit)
                            {
                                log::error!(
                                    "Failed to roll back GPU {} power limit: {}",
                                    rollback_uuid,
                                    rollback_error
                                );
                            }
                        }
                        Err(rollback_error) => log::error!(
                            "Failed to open GPU {} for power rollback: {}",
                            rollback_uuid,
                            rollback_error
                        ),
                    }
                }
                return Err(error.into());
            }
            applied.push((uuid.clone(), *old_limit));
        }
    }

    for (_, _, name, _) in targets {
        let message = if dry_run {
            format!("[DRY RUN] Would set power limit to {watts}W on GPU {name}")
        } else {
            format!("Set power limit to {watts}W on GPU {name}")
        };

        output.write(&Message {
            message,
            success: true,
        })?;
    }

    output.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NvmlError;
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
    fn device_reopen_failure_rolls_back_earlier_power_writes() {
        let manager = FailingUuidLookupManager {
            inner: MockManager::new(2),
            uuid_lookups: AtomicUsize::new(0),
            fail_on_lookup: 3,
        };

        assert!(run_power_limit(
            &manager,
            250,
            OutputFormat::Compact,
            &["GPU-MOCK-0000".to_string(), "GPU-MOCK-0001".to_string()],
            false,
        )
        .is_err());

        for index in 0..2 {
            assert_eq!(
                manager
                    .device_by_index(index)
                    .unwrap()
                    .power_limit()
                    .unwrap(),
                PowerLimit::from_watts(300)
            );
        }
    }
}
