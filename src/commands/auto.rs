//! Workload-aware profile automation commands and runtime status.

use crate::automation::{
    AutomationEngine, DesiredProfile, ProcessObservation, ProfileSwitchGate, RuleMatch,
};
use crate::cli::args::{AutoArgs, AutoCommands, OutputFormat};
use crate::cli::output::{print_output, TableDisplay};
use crate::commands::profile::{find_profile, load_catalog, wait_while_running};
use crate::commands::runtime::install_shutdown_handler;
use crate::commands::selection::{resolve_gpu_uuids, select_gpu_indices, GpuSelector};
use crate::config::{AutomationConfig, Config};
use crate::error::{ConfigError, DomainError, Result};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};
use crate::profiles::ProfileCatalog;
use crate::services::{GpuMutationGuard, ProfileController};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Execute a workload automation command.
pub fn run_auto(
    args: &AutoArgs,
    format: OutputFormat,
    selector: &GpuSelector,
    config: &Config,
) -> Result<()> {
    match &args.command {
        AutoCommands::Run {
            interval,
            single_use,
            force,
        } => run_automation(
            interval.unwrap_or(config.automation.interval_seconds),
            *single_use,
            *force,
            format,
            selector,
            config,
        ),
        AutoCommands::Explain => explain_automation(format, selector, config),
        AutoCommands::Status => show_status(format, config),
    }
}

fn run_automation(
    interval_seconds: u64,
    single_use: bool,
    force: bool,
    format: OutputFormat,
    selector: &GpuSelector,
    config: &Config,
) -> Result<()> {
    if !config.automation.enabled && !force {
        return Err(ConfigError::InvalidValue {
            key: "automation.enabled".to_string(),
            message: "set it to true, or pass --force for an intentional one-off run".to_string(),
        }
        .into());
    }
    if interval_seconds == 0 {
        return Err(ConfigError::InvalidValue {
            key: "automation.interval_seconds".to_string(),
            message: "must be greater than zero".to_string(),
        }
        .into());
    }

    let catalog = load_catalog(config)?;
    validate_profile_references(&config.automation, &catalog)?;
    let engine = AutomationEngine::new(&config.automation)?;
    let manager = NvmlManager::new()?;
    let indices = select_gpu_indices(&manager, selector)?;
    let target_uuids = resolve_gpu_uuids(&manager, &indices)?;
    let _mutation_guard = if config.general.dry_run {
        None
    } else {
        Some(GpuMutationGuard::acquire(&target_uuids, "automation")?)
    };
    let pid = std::process::id();
    let _pid_guard = claim_pid_file(pid)?;
    let mut controller =
        ProfileController::new_by_uuids(&manager, &target_uuids, config.general.dry_run)?;
    let running = install_shutdown_handler()?;

    let started_at = chrono::Utc::now().to_rfc3339();
    let start = Instant::now();
    let interval = Duration::from_secs(interval_seconds);
    let cooldown = Duration::from_secs(config.automation.cooldown_seconds);
    let mut gate = ProfileSwitchGate::default();
    let mut transitions = 0usize;
    let mut fan_adjustments = 0usize;
    let mut last_match = None;
    let mut last_desired = DesiredProfile::Baseline;
    let mut last_processes = Vec::new();

    let loop_result = loop {
        if !running.load(Ordering::SeqCst) {
            break Ok(());
        }
        let observations = match collect_observations(&manager, &target_uuids) {
            Ok(observations) => observations,
            Err(error) => break Err(error),
        };
        let matched = engine.evaluate(&observations);
        let desired = desired_profile(&matched, &config.automation);
        let stable_for = if single_use {
            Duration::ZERO
        } else {
            Duration::from_secs(
                matched
                    .as_ref()
                    .map_or(config.automation.release_delay_seconds, |matched| {
                        matched.min_duration_seconds
                    }),
            )
        };

        if let Some(transition) = gate.observe(
            desired.clone(),
            start.elapsed(),
            stable_for,
            if single_use { Duration::ZERO } else { cooldown },
        ) {
            let result: Result<()> = match &transition {
                DesiredProfile::Baseline => controller.restore().map(|_| ()).map_err(Into::into),
                DesiredProfile::Named(name) => match find_profile(&catalog, name) {
                    Ok(profile) => controller
                        .apply_profile(profile)
                        .map(|_| ())
                        .map_err(Into::into),
                    Err(error) => Err(error),
                },
            };
            if let Err(error) = result {
                break Err(error);
            }
            transitions += 1;
        }

        if let DesiredProfile::Named(name) = gate.active() {
            let profile = match find_profile(&catalog, name) {
                Ok(profile) => profile,
                Err(error) => break Err(error),
            };
            match controller.tick_fan_curves(profile) {
                Ok(report) => fan_adjustments += report.actions.len(),
                Err(error) => break Err(error.into()),
            }
        }

        last_processes = observed_processes(&observations);
        last_match = matched;
        last_desired = desired;
        let snapshot = AutoRuntimeState {
            pid,
            started_at: started_at.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            dry_run: config.general.dry_run,
            interval_seconds,
            active_profile: controller.active_profile().map(ToOwned::to_owned),
            desired_profile: desired_label(&last_desired),
            matched_rule: last_match.clone(),
            observed_processes: last_processes.clone(),
            restoration_error: None,
        };
        if let Err(error) = persist_state(&snapshot) {
            break Err(error);
        }

        if single_use || !running.load(Ordering::SeqCst) {
            break Ok(());
        }
        wait_while_running(&running, interval);
        if !running.load(Ordering::SeqCst) {
            break Ok(());
        }
    };

    let restored = controller.restore();
    let (active_profile, restoration_error) = match &restored {
        Ok(_) => (None, None),
        Err(error) => (
            controller.active_profile().map(ToOwned::to_owned),
            Some(error.to_string()),
        ),
    };
    let final_state = AutoRuntimeState {
        pid: 0,
        started_at,
        updated_at: chrono::Utc::now().to_rfc3339(),
        dry_run: config.general.dry_run,
        interval_seconds,
        active_profile,
        desired_profile: desired_label(&last_desired),
        matched_rule: last_match.clone(),
        observed_processes: last_processes.clone(),
        restoration_error,
    };
    let state_result = persist_state(&final_state);

    match (loop_result, restored) {
        (Err(error), Err(restore_error)) => {
            if let Err(state_error) = state_result {
                log::error!("Failed to persist automation failure state: {state_error}");
            }
            Err(
                crate::services::profile_controller::ProfileApplyError::Rollback {
                    original: error.to_string(),
                    rollback: restore_error.to_string(),
                }
                .into(),
            )
        }
        (Err(error), Ok(_)) => {
            if let Err(state_error) = state_result {
                log::error!("Failed to persist automation failure state: {state_error}");
            }
            Err(error)
        }
        (Ok(()), Err(error)) => {
            if let Err(state_error) = state_result {
                log::error!("Failed to persist automation restoration failure: {state_error}");
            }
            Err(error.into())
        }
        (Ok(()), Ok(restored)) => {
            state_result?;
            print_output(
                &AutomationRunOutput {
                    dry_run: config.general.dry_run,
                    single_use,
                    transitions,
                    fan_adjustments,
                    final_decision: desired_label(&last_desired),
                    matched_rule: last_match,
                    observed_processes: last_processes,
                    restored_actions: restored.actions,
                },
                format,
            )?;
            Ok(())
        }
    }
}

fn explain_automation(format: OutputFormat, selector: &GpuSelector, config: &Config) -> Result<()> {
    let catalog = load_catalog(config)?;
    validate_profile_references(&config.automation, &catalog)?;
    let engine = AutomationEngine::new(&config.automation)?;
    let manager = NvmlManager::new()?;
    let indices = select_gpu_indices(&manager, selector)?;
    let target_uuids = resolve_gpu_uuids(&manager, &indices)?;
    let observations = collect_observations(&manager, &target_uuids)?;
    let matched = engine.evaluate(&observations);
    let desired = desired_profile(&matched, &config.automation);
    print_output(
        &AutomationExplanation {
            enabled: config.automation.enabled,
            profiles_directory: catalog.directory().display().to_string(),
            rule_count: config.automation.rules.len(),
            desired_profile: desired_label(&desired),
            matched_rule: matched,
            observed_processes: observed_processes(&observations),
        },
        format,
    )?;
    Ok(())
}

fn show_status(format: OutputFormat, config: &Config) -> Result<()> {
    let state = read_state_if_exists()?;
    let pid = running_automation_pid()?;
    let output = AutomationStatusOutput {
        running: pid.is_some(),
        pid,
        config_enabled: config.automation.enabled,
        rule_count: config.automation.rules.len(),
        profiles_directory: config
            .automation
            .profiles_dir
            .clone()
            .unwrap_or_else(ProfileCatalog::default_directory)
            .display()
            .to_string(),
        last_state: state,
    };
    print_output(&output, format)?;
    Ok(())
}

fn validate_profile_references(
    automation: &AutomationConfig,
    catalog: &ProfileCatalog,
) -> Result<()> {
    if let Some(profile) = &automation.fallback_profile {
        find_profile(catalog, profile)?;
    }
    for rule in &automation.rules {
        find_profile(catalog, &rule.profile).map_err(|_| ConfigError::InvalidValue {
            key: format!("automation.rules.{}.profile", rule.name),
            message: format!(
                "profile '{}' was not found in {}",
                rule.profile,
                catalog.directory().display()
            ),
        })?;
    }
    Ok(())
}

fn desired_profile(matched: &Option<RuleMatch>, automation: &AutomationConfig) -> DesiredProfile {
    matched
        .as_ref()
        .map(|matched| DesiredProfile::Named(matched.profile.clone()))
        .or_else(|| {
            automation
                .fallback_profile
                .as_ref()
                .map(|profile| DesiredProfile::Named(profile.clone()))
        })
        .unwrap_or(DesiredProfile::Baseline)
}

fn desired_label(desired: &DesiredProfile) -> String {
    match desired {
        DesiredProfile::Baseline => "baseline".to_string(),
        DesiredProfile::Named(profile) => profile.clone(),
    }
}

fn collect_observations<M: GpuManager>(
    manager: &M,
    target_uuids: &[String],
) -> Result<Vec<ProcessObservation>> {
    let mut observations = Vec::new();
    for uuid in target_uuids {
        let device = manager.device_by_uuid(uuid)?;
        let index = device.index();
        for process in device.running_processes()?.processes {
            observations.push(ProcessObservation {
                gpu_uuid: uuid.clone(),
                gpu_index: index,
                process,
            });
        }
    }
    observations.sort_by(|left, right| {
        left.gpu_index
            .cmp(&right.gpu_index)
            .then_with(|| left.process.pid.cmp(&right.process.pid))
            .then_with(|| {
                left.process
                    .display_name()
                    .cmp(&right.process.display_name())
            })
    });
    Ok(observations)
}

fn observed_processes(observations: &[ProcessObservation]) -> Vec<ObservedProcess> {
    observations
        .iter()
        .map(|observation| ObservedProcess {
            gpu_index: observation.gpu_index,
            gpu_uuid: observation.gpu_uuid.clone(),
            pid: observation.process.pid,
            name: observation.process.display_name(),
            process_type: observation.process.process_type.to_string(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObservedProcess {
    gpu_index: u32,
    gpu_uuid: String,
    pid: u32,
    name: String,
    process_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoRuntimeState {
    pid: u32,
    started_at: String,
    updated_at: String,
    dry_run: bool,
    interval_seconds: u64,
    active_profile: Option<String>,
    desired_profile: String,
    matched_rule: Option<RuleMatch>,
    observed_processes: Vec<ObservedProcess>,
    #[serde(default)]
    restoration_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AutomationExplanation {
    enabled: bool,
    profiles_directory: String,
    rule_count: usize,
    desired_profile: String,
    matched_rule: Option<RuleMatch>,
    observed_processes: Vec<ObservedProcess>,
}

impl TableDisplay for AutomationExplanation {
    fn to_table(&self) -> String {
        let mut output = format!(
            "Automation: {}\nProfiles: {}\nRules: {}\nDecision: {}\n",
            if self.enabled { "enabled" } else { "disabled" },
            self.profiles_directory,
            self.rule_count,
            self.desired_profile
        );
        if let Some(matched) = &self.matched_rule {
            output.push_str(&format!(
                "Matched '{}' (priority {}) via {} (PID {}, GPU {})\n",
                matched.rule, matched.priority, matched.process, matched.pid, matched.gpu_index
            ));
        } else {
            output.push_str("Matched rule: none\n");
        }
        output.push_str(&format!(
            "Observed GPU processes: {}",
            self.observed_processes.len()
        ));
        for process in &self.observed_processes {
            output.push_str(&format!(
                "\n  GPU {} PID {} [{}] {}",
                process.gpu_index, process.pid, process.process_type, process.name
            ));
        }
        output
    }
}

#[derive(Debug, Clone, Serialize)]
struct AutomationRunOutput {
    dry_run: bool,
    single_use: bool,
    transitions: usize,
    fan_adjustments: usize,
    final_decision: String,
    matched_rule: Option<RuleMatch>,
    observed_processes: Vec<ObservedProcess>,
    restored_actions: Vec<String>,
}

impl TableDisplay for AutomationRunOutput {
    fn to_table(&self) -> String {
        format!(
            "Automation {}{}; decision {}, {} transition(s), {} fan adjustment(s); original GPU state restored",
            if self.single_use { "single use complete" } else { "stopped" },
            if self.dry_run { " (dry run)" } else { "" },
            self.final_decision,
            self.transitions,
            self.fan_adjustments
        )
    }
}

#[derive(Debug, Clone, Serialize)]
struct AutomationStatusOutput {
    running: bool,
    pid: Option<u32>,
    config_enabled: bool,
    rule_count: usize,
    profiles_directory: String,
    last_state: Option<AutoRuntimeState>,
}

impl TableDisplay for AutomationStatusOutput {
    fn to_table(&self) -> String {
        let mut output = if let Some(pid) = self.pid {
            format!("Automation is running (PID {pid})")
        } else {
            "Automation is not running".to_string()
        };
        output.push_str(&format!(
            "\nConfiguration: {}, {} rule(s)\nProfiles: {}",
            if self.config_enabled {
                "enabled"
            } else {
                "disabled"
            },
            self.rule_count,
            self.profiles_directory
        ));
        if let Some(state) = &self.last_state {
            output.push_str(&format!(
                "\nLast update: {}\nLast decision: {}\nActive profile: {}",
                state.updated_at,
                state.desired_profile,
                state.active_profile.as_deref().unwrap_or("baseline")
            ));
            if let Some(rule) = &state.matched_rule {
                output.push_str(&format!("\nLast matched rule: {}", rule.rule));
            }
            if let Some(error) = &state.restoration_error {
                output.push_str(&format!("\nRESTORATION ERROR: {error}"));
            }
        }
        output
    }
}

struct PidFileGuard {
    file: File,
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        if let Err(error) = FileExt::unlock(&self.file) {
            log::error!("Failed to release the automation process lock: {error}");
        }
    }
}

fn claim_pid_file(pid: u32) -> Result<PidFileGuard> {
    fs::create_dir_all(runtime_dir())?;
    claim_pid_file_at(&pid_path(), pid)
}

fn claim_pid_file_at(path: &Path, pid: u32) -> Result<PidFileGuard> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            let existing = read_pid_from_file(&mut file)?.map_or_else(
                || "an unknown PID".to_string(),
                |existing| format!("PID {existing}"),
            );
            return Err(DomainError::InvalidValue(format!(
                "profile automation is already running with {existing}"
            ))
            .into());
        }
        Err(error) => return Err(error.into()),
    }
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    Ok(PidFileGuard { file })
}

fn running_automation_pid() -> Result<Option<u32>> {
    running_automation_pid_at(&pid_path())
}

fn running_automation_pid_at(path: &Path) -> Result<Option<u32>> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file)?;
            Ok(None)
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => read_pid_from_file(&mut file),
        Err(error) => Err(error.into()),
    }
}

fn read_pid_from_file(file: &mut File) -> Result<Option<u32>> {
    file.seek(SeekFrom::Start(0))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let value = contents.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<u32>().map(Some).map_err(|error| {
        ConfigError::ParseError(format!("invalid automation PID file: {error}")).into()
    })
}

fn persist_state(state: &AutoRuntimeState) -> Result<()> {
    let directory = runtime_dir();
    fs::create_dir_all(&directory)?;
    let contents = serde_json::to_vec_pretty(state).map_err(ConfigError::from)?;
    let temporary = directory.join(format!("state-{}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    if let Err(error) = fs::rename(&temporary, state_path()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn read_state_if_exists() -> Result<Option<AutoRuntimeState>> {
    let contents = match fs::read_to_string(state_path()) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(ConfigError::from)
        .map_err(Into::into)
}

fn runtime_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nvctl")
        .join("automation")
}

fn pid_path() -> PathBuf {
    runtime_dir().join("automation.pid")
}

fn state_path() -> PathBuf {
    runtime_dir().join("state.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationRule;
    use crate::mock::MockManager;
    use crate::profiles::Profile;
    use tempfile::tempdir;

    #[test]
    fn desired_profile_uses_match_then_fallback_then_baseline() {
        let matched = Some(RuleMatch {
            rule: "game".to_string(),
            profile: "Performance".to_string(),
            priority: 10,
            gpu_uuid: "GPU-1".to_string(),
            gpu_index: 0,
            pid: 42,
            process: "game".to_string(),
            process_type: "graphics".to_string(),
            min_duration_seconds: 0,
        });
        assert_eq!(
            desired_profile(&matched, &AutomationConfig::default()),
            DesiredProfile::Named("Performance".to_string())
        );
        let fallback = AutomationConfig {
            fallback_profile: Some("Quiet".to_string()),
            ..AutomationConfig::default()
        };
        assert_eq!(
            desired_profile(&None, &fallback),
            DesiredProfile::Named("Quiet".to_string())
        );
        assert_eq!(
            desired_profile(&None, &AutomationConfig::default()),
            DesiredProfile::Baseline
        );
    }

    #[test]
    fn missing_referenced_profile_is_rejected() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("Quiet.toml"),
            toml::to_string(&Profile::new("Quiet")).unwrap(),
        )
        .unwrap();
        let catalog = ProfileCatalog::load_from(directory.path()).unwrap();
        let automation = AutomationConfig {
            rules: vec![AutomationRule {
                name: "game".to_string(),
                profile: "Missing".to_string(),
                processes: vec!["game".to_string()],
                ..AutomationRule::default()
            }],
            ..AutomationConfig::default()
        };
        assert!(validate_profile_references(&automation, &catalog).is_err());
    }

    #[test]
    fn explanation_table_reports_decision_and_process_count() {
        let explanation = AutomationExplanation {
            enabled: true,
            profiles_directory: "/profiles".to_string(),
            rule_count: 2,
            desired_profile: "Performance".to_string(),
            matched_rule: None,
            observed_processes: Vec::new(),
        };
        let table = explanation.to_table();
        assert!(table.contains("Decision: Performance"));
        assert!(table.contains("Observed GPU processes: 0"));
    }

    #[test]
    fn observations_reopen_targets_by_stable_uuid() {
        let manager = MockManager::new(1);
        assert!(
            collect_observations(&manager, &["GPU-MOCK-0000".to_string()])
                .unwrap()
                .is_empty()
        );
        assert!(collect_observations(&manager, &["GPU-MOCK-9999".to_string()]).is_err());
    }

    #[test]
    fn pid_file_lock_has_atomic_ownership_and_live_status() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("automation.pid");
        let guard = claim_pid_file_at(&path, 1234).unwrap();

        assert_eq!(running_automation_pid_at(&path).unwrap(), Some(1234));
        assert!(claim_pid_file_at(&path, 5678).is_err());

        drop(guard);
        assert_eq!(running_automation_pid_at(&path).unwrap(), None);
        claim_pid_file_at(&path, 5678).unwrap();
    }

    #[test]
    fn status_surfaces_restoration_failures() {
        let status = AutomationStatusOutput {
            running: false,
            pid: None,
            config_enabled: true,
            rule_count: 1,
            profiles_directory: "/profiles".to_string(),
            last_state: Some(AutoRuntimeState {
                pid: 0,
                started_at: "start".to_string(),
                updated_at: "update".to_string(),
                dry_run: false,
                interval_seconds: 5,
                active_profile: Some("Performance".to_string()),
                desired_profile: "baseline".to_string(),
                matched_rule: None,
                observed_processes: Vec::new(),
                restoration_error: Some("GPU was lost".to_string()),
            }),
        };

        let table = status.to_table();
        assert!(table.contains("Active profile: Performance"));
        assert!(table.contains("RESTORATION ERROR: GPU was lost"));
    }

    #[test]
    fn older_runtime_state_defaults_restoration_error() {
        let state: AutoRuntimeState = serde_json::from_str(
            r#"{
                "pid": 0,
                "started_at": "start",
                "updated_at": "update",
                "dry_run": false,
                "interval_seconds": 5,
                "active_profile": null,
                "desired_profile": "baseline",
                "matched_rule": null,
                "observed_processes": []
            }"#,
        )
        .unwrap();
        assert!(state.restoration_error.is_none());
    }
}
