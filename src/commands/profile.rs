//! Saved profile discovery and foreground profile sessions.

use crate::cli::args::{OutputFormat, ProfileArgs, ProfileCommands};
use crate::cli::output::{print_output, TableDisplay};
use crate::commands::runtime::install_shutdown_handler;
use crate::commands::selection::{resolve_gpu_uuids, select_gpu_indices, GpuSelector};
use crate::config::Config;
use crate::error::{ConfigError, Result};
use crate::nvml::NvmlManager;
use crate::profiles::{GpuSettings, Profile, ProfileCatalog};
use crate::services::{GpuMutationGuard, ProfileController};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Execute a saved profile command.
pub fn run_profile(
    args: &ProfileArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    config: &Config,
) -> Result<()> {
    let catalog = load_catalog(config)?;
    match &args.command {
        ProfileCommands::List => {
            print_output(&profile_list_output(&catalog), format).map_err(Into::into)
        }
        ProfileCommands::Show { name } => {
            let profile = find_profile(&catalog, name)?;
            print_output(
                &ProfileShowOutput {
                    directory: catalog.directory().display().to_string(),
                    profile: profile.clone(),
                },
                format,
            )?;
            Ok(())
        }
        ProfileCommands::Run {
            name,
            interval,
            single_use,
        } => run_profile_session(
            &catalog,
            name,
            interval.unwrap_or(config.automation.interval_seconds),
            *single_use,
            format,
            selector,
            config.general.dry_run,
        ),
    }
}

pub(crate) fn load_catalog(config: &Config) -> Result<ProfileCatalog> {
    config
        .automation
        .profiles_dir
        .as_ref()
        .map_or_else(ProfileCatalog::load_default, ProfileCatalog::load_from)
        .map_err(Into::into)
}

pub(crate) fn find_profile<'catalog>(
    catalog: &'catalog ProfileCatalog,
    name: &str,
) -> Result<&'catalog Profile> {
    catalog.get(name).ok_or_else(|| {
        ConfigError::InvalidValue {
            key: "profile".to_string(),
            message: format!(
                "profile '{name}' was not found in {}",
                catalog.directory().display()
            ),
        }
        .into()
    })
}

fn run_profile_session(
    catalog: &ProfileCatalog,
    name: &str,
    interval_seconds: u64,
    single_use: bool,
    format: OutputFormat,
    selector: &GpuSelector,
    dry_run: bool,
) -> Result<()> {
    if interval_seconds == 0 {
        return Err(ConfigError::InvalidValue {
            key: "automation.interval_seconds".to_string(),
            message: "must be greater than zero".to_string(),
        }
        .into());
    }

    let profile = find_profile(catalog, name)?;
    let manager = NvmlManager::new()?;
    let indices = select_gpu_indices(&manager, selector)?;
    let target_uuids = resolve_gpu_uuids(&manager, &indices)?;
    let _mutation_guard = if dry_run {
        None
    } else {
        Some(GpuMutationGuard::acquire(&target_uuids, "profile run")?)
    };
    let mut controller = ProfileController::new_by_uuids(&manager, &target_uuids, dry_run)?;
    let running = install_shutdown_handler()?;
    if !running.load(Ordering::SeqCst) {
        return Ok(());
    }
    let initial = controller.apply_profile(profile)?;

    let interval = Duration::from_secs(interval_seconds);
    let mut fan_adjustments = 0usize;
    let loop_result = 'session: loop {
        if !running.load(Ordering::SeqCst) {
            break 'session Ok(());
        }
        match controller.tick_fan_curves(profile) {
            Ok(report) => fan_adjustments += report.actions.len(),
            Err(error) => break 'session Err(error),
        }
        if single_use || !running.load(Ordering::SeqCst) {
            break 'session Ok(());
        }
        wait_while_running(&running, interval);
        if !running.load(Ordering::SeqCst) {
            break 'session Ok(());
        }
    };

    let restored = controller.restore();
    match (loop_result, restored) {
        (Err(original), Err(rollback)) => {
            return Err(
                crate::services::profile_controller::ProfileApplyError::Rollback {
                    original: original.to_string(),
                    rollback: rollback.to_string(),
                }
                .into(),
            );
        }
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => return Err(error.into()),
        (Ok(()), Ok(restored)) => {
            print_output(
                &ProfileSessionOutput {
                    profile: profile.name.clone(),
                    dry_run,
                    single_use,
                    initial_actions: initial.actions,
                    fan_adjustments,
                    restored_actions: restored.actions,
                },
                format,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn wait_while_running(running: &AtomicBool, duration: Duration) {
    let Some(deadline) = Instant::now().checked_add(duration) else {
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

#[derive(Debug, Clone, Serialize)]
struct ProfileSummary {
    name: String,
    description: Option<String>,
    is_default: bool,
    gpu_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileListOutput {
    directory: String,
    profiles: Vec<ProfileSummary>,
}

fn profile_list_output(catalog: &ProfileCatalog) -> ProfileListOutput {
    ProfileListOutput {
        directory: catalog.directory().display().to_string(),
        profiles: catalog
            .list()
            .into_iter()
            .map(|profile| ProfileSummary {
                name: profile.name.clone(),
                description: profile.description.clone(),
                is_default: profile.is_default,
                gpu_entries: profile.gpu_settings.len(),
            })
            .collect(),
    }
}

impl TableDisplay for ProfileListOutput {
    fn to_table(&self) -> String {
        let mut output = format!("Profile directory: {}\n", self.directory);
        if self.profiles.is_empty() {
            output.push_str("No saved profiles found");
            return output;
        }
        for profile in &self.profiles {
            let default = if profile.is_default { " [default]" } else { "" };
            let description = profile
                .description
                .as_deref()
                .map(|value| format!(" — {value}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "  {}{} ({} GPU entries){}\n",
                profile.name, default, profile.gpu_entries, description
            ));
        }
        output
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProfileShowOutput {
    directory: String,
    profile: Profile,
}

impl TableDisplay for ProfileShowOutput {
    fn to_table(&self) -> String {
        let mut output = format!(
            "Profile: {}{}\nDirectory: {}\n",
            self.profile.name,
            if self.profile.is_default {
                " [default]"
            } else {
                ""
            },
            self.directory
        );
        if let Some(description) = &self.profile.description {
            output.push_str(&format!("Description: {description}\n"));
        }

        let mut entries: Vec<_> = self.profile.gpu_settings.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        if entries.is_empty() {
            output.push_str("GPU settings: none");
            return output;
        }
        output.push_str("GPU settings:\n");
        for (gpu_id, settings) in entries {
            output.push_str(&format!("  {gpu_id}: {}\n", describe_settings(settings)));
        }
        output
    }
}

fn describe_settings(settings: &GpuSettings) -> String {
    let mut parts = Vec::new();
    if settings.apply_power_limit {
        parts.push(settings.power_limit.map_or_else(
            || "power enabled without value".to_string(),
            |limit| format!("power {limit}"),
        ));
    }
    if let Some(limit) = settings.acoustic_limit {
        parts.push(format!("acoustic {limit}"));
    }
    if settings.apply_fan_curve {
        let count = if settings.fan_curves.is_empty() {
            usize::from(settings.fan_curve.is_some())
        } else {
            settings.fan_curves.len()
        };
        parts.push(format!("{count} fan curve(s)"));
    }
    if parts.is_empty() {
        "no applied controls".to_string()
    } else {
        parts.join(", ")
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProfileSessionOutput {
    profile: String,
    dry_run: bool,
    single_use: bool,
    initial_actions: Vec<String>,
    fan_adjustments: usize,
    restored_actions: Vec<String>,
}

impl TableDisplay for ProfileSessionOutput {
    fn to_table(&self) -> String {
        let mode = if self.dry_run { "dry run" } else { "live" };
        let lifecycle = if self.single_use {
            "single use"
        } else {
            "stopped"
        };
        format!(
            "Profile '{}' {} ({mode}); {} initial action(s), {} fan adjustment(s), {} restoration action(s)",
            self.profile,
            lifecycle,
            self.initial_actions.len(),
            self.fan_adjustments,
            self.restored_actions.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PowerLimit;
    use crate::profiles::GpuSettings;
    use tempfile::tempdir;

    #[test]
    fn list_output_is_sorted_and_descriptive() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("Zulu.toml"),
            toml::to_string(&Profile::new("Zulu")).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("Alpha.toml"),
            toml::to_string(&Profile::new("Alpha").with_gpu_settings(
                "default",
                GpuSettings::new().with_power_limit(PowerLimit::from_watts(300)),
            ))
            .unwrap(),
        )
        .unwrap();
        let catalog = ProfileCatalog::load_from(directory.path()).unwrap();
        let output = profile_list_output(&catalog);
        assert_eq!(output.profiles[0].name, "Alpha");
        assert_eq!(output.profiles[1].name, "Zulu");
        assert!(output.to_table().contains("1 GPU entries"));
    }

    #[test]
    fn setting_description_includes_enabled_controls() {
        let settings = GpuSettings::new().with_power_limit(PowerLimit::from_watts(320));
        assert_eq!(describe_settings(&settings), "power 320W");
    }
}
