//! Thermal command implementation
//!
//! Handles thermal status and acoustic limit commands.

use crate::cli::args::{OutputFormat, ThermalArgs, ThermalCommands};
use crate::cli::output::{AcousticStatus, Message, OutputWriter};
use crate::commands::selection::{select_gpu_indices, GpuSelector};
use crate::domain::Temperature;
use crate::error::{DomainError, NvmlError, Result};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute thermal commands
pub fn run_thermal(
    args: &ThermalArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    match &args.command {
        ThermalCommands::Status => run_thermal_status(&manager, format, selector),
        ThermalCommands::Limit { celsius } => {
            run_thermal_limit(&manager, *celsius, format, selector, dry_run)
        }
    }
}

fn run_thermal_status(
    manager: &NvmlManager,
    format: OutputFormat,
    selector: &GpuSelector,
) -> Result<()> {
    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        let current_temp = device.temperature()?.as_celsius();
        let acoustic = device.acoustic_limits()?;

        let status = AcousticStatus {
            gpu_name: info.name,
            gpu_index: idx,
            current_temp,
            acoustic_current: acoustic.current.map(|t| t.as_celsius()),
            acoustic_min: acoustic.min.map(|t| t.as_celsius()),
            acoustic_max: acoustic.max.map(|t| t.as_celsius()),
        };

        output.write(&status)?;
    }

    output.finish()?;
    Ok(())
}

fn run_thermal_limit<M: GpuManager>(
    manager: &M,
    celsius: i32,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    let target = Temperature::new(celsius);

    let indices = select_gpu_indices(manager, selector)?;
    let mut output = OutputWriter::new(format);
    let mut targets = Vec::with_capacity(indices.len());

    // Validate every selected GPU before mutating any of them.
    for &idx in &indices {
        let device = manager.device_by_index(idx)?;
        let limits = device.acoustic_limits()?;
        if limits.current.is_none() || limits.min.is_none() || limits.max.is_none() {
            return Err(NvmlError::NotSupported(format!(
                "GPU {} does not expose complete acoustic temperature limits",
                device.name()?
            ))
            .into());
        }
        if !limits.is_valid(target) {
            return Err(DomainError::InvalidTemperature(celsius).into());
        }
        let info = device.info()?;
        targets.push((idx, info.uuid, info.name, limits.current));
    }

    if !dry_run {
        let mut applied: Vec<(String, Option<Temperature>)> = Vec::new();
        for (_, uuid, _, old_limit) in &targets {
            let result = manager
                .device_by_uuid(uuid)
                .and_then(|mut device| device.set_acoustic_limit(target));
            if let Err(error) = result {
                for (rollback_uuid, rollback_limit) in applied.into_iter().rev() {
                    if let Some(rollback_limit) = rollback_limit {
                        match manager.device_by_uuid(&rollback_uuid) {
                            Ok(mut rollback_device) => {
                                if let Err(rollback_error) =
                                    rollback_device.set_acoustic_limit(rollback_limit)
                                {
                                    log::error!(
                                        "Failed to roll back GPU {} acoustic limit: {}",
                                        rollback_uuid,
                                        rollback_error
                                    );
                                }
                            }
                            Err(rollback_error) => log::error!(
                                "Failed to open GPU {} for acoustic rollback: {}",
                                rollback_uuid,
                                rollback_error
                            ),
                        }
                    }
                }
                return Err(error.into());
            }
            applied.push((uuid.clone(), *old_limit));
        }
    }

    for (_, _, name, _) in targets {
        let message = if dry_run {
            format!("[DRY RUN] Would set acoustic temperature limit to {celsius}°C on GPU {name}")
        } else {
            format!("Set acoustic temperature limit to {celsius}°C on GPU {name}")
        };

        output.write(&Message {
            message,
            success: true,
        })?;
    }

    output.finish()?;
    Ok(())
}
