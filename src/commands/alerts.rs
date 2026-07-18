//! Alert command implementation
//!
//! Runs alert monitoring and exposes a small filesystem-backed control plane so
//! separate CLI invocations can inspect and control the background process.

use crate::alerts::{
    Alert, AlertConfig, AlertManager, AlertManagerConfig, AlertSeverity, NotificationManager,
};
use crate::cli::args::{AlertCommands, OutputFormat};
use crate::cli::output::{print_output, Message};
use crate::error::{ConfigError, DomainError, Result};
use crate::nvml::{GpuManager, NvmlManager};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AlertRuntimeSnapshot {
    pid: u32,
    updated_at_unix_secs: u64,
    active_alerts: Vec<Alert>,
    history: Vec<Alert>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum AlertRuntimeCommand {
    Acknowledge { alert_id: String },
    Silence { alert_id: String },
    ClearHistory {},
}

struct PidFileGuard {
    path: PathBuf,
    pid: u32,
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        if read_pid_from(&self.path).ok().flatten() == Some(self.pid) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Execute alert commands.
pub fn run_alerts(command: &AlertCommands, format: OutputFormat) -> Result<()> {
    match command {
        AlertCommands::Start {
            interval,
            config,
            foreground,
        } => run_alert_start(*interval, config.clone(), *foreground),
        AlertCommands::Stop => run_alert_stop(format),
        AlertCommands::List { all, severity } => run_alert_list(*all, severity.as_deref(), format),
        AlertCommands::Rules { config } => run_alert_rules(config.clone(), format),
        AlertCommands::Ack { alert_id } => run_alert_ack(alert_id, format),
        AlertCommands::Silence { alert_id } => run_alert_silence(alert_id, format),
        AlertCommands::Clear => run_alert_clear(format),
        AlertCommands::Test { config } => run_alert_test(config.clone()),
    }
}

fn run_alert_start(
    interval_override: Option<u64>,
    config_path: Option<String>,
    foreground: bool,
) -> Result<()> {
    let config_path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(AlertConfig::default_path);
    let alert_config = load_or_create_alert_config(&config_path)?;
    let interval = interval_override.unwrap_or(alert_config.settings.check_interval_secs);
    if interval == 0 {
        return Err(ConfigError::InvalidValue {
            key: "alerts.check_interval_secs".to_string(),
            message: "must be greater than zero".to_string(),
        }
        .into());
    }

    ensure_runtime_dirs()?;
    if let Some(pid) = running_daemon_pid()? {
        return Err(DomainError::InvalidValue(format!(
            "alert daemon is already running with PID {pid}"
        ))
        .into());
    }

    if !foreground {
        return spawn_alert_daemon(&config_path, interval);
    }

    run_alert_monitor(alert_config, interval)
}

fn load_or_create_alert_config(path: &Path) -> Result<AlertConfig> {
    if path.exists() {
        return AlertConfig::load(path);
    }

    let config = AlertConfig::default();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    config.save(path)?;
    Ok(config)
}

fn spawn_alert_daemon(config_path: &Path, interval: u64) -> Result<()> {
    let executable = std::env::current_exe()?;
    let log_path = runtime_dir().join("daemon.log");
    let stdout_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr_log = stdout_log.try_clone()?;

    let mut child = Command::new(executable)
        .arg("alerts")
        .arg("start")
        .arg("--foreground")
        .arg("--interval")
        .arg(interval.to_string())
        .arg("--config")
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log))
        .spawn()?;

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(DomainError::InvalidValue(format!(
                "alert daemon exited during startup with {status}; see {}",
                log_path.display()
            ))
            .into());
        }
        if read_pid_from(&pid_path())? == Some(child.id())
            && read_snapshot_if_exists()?.is_some_and(|snapshot| snapshot.pid == child.id())
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DomainError::InvalidValue(format!(
                "alert daemon did not become ready within 2 seconds; see {}",
                log_path.display()
            ))
            .into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!(
        "Alert daemon started with PID {} (log: {})",
        child.id(),
        log_path.display()
    );
    Ok(())
}

fn run_alert_monitor(alert_config: AlertConfig, interval_secs: u64) -> Result<()> {
    let pid = std::process::id();

    let manager_config = AlertManagerConfig {
        enabled: alert_config.settings.enabled,
        check_interval: Duration::from_secs(interval_secs),
        max_history: alert_config.settings.max_history,
    };
    let mut manager = AlertManager::new(manager_config);
    manager.add_rules(alert_config.to_alert_rules()?);
    if let Some(snapshot) = read_snapshot_if_exists()? {
        manager.restore_state(snapshot.active_alerts, snapshot.history);
    }

    let notifier = NotificationManager::default();
    let nvml = NvmlManager::new()?;
    let device_count = nvml.device_count()?;
    if device_count == 0 {
        return Err(crate::error::AppError::NoGpusFound);
    }

    let running = Arc::new(AtomicBool::new(true));
    let signal_running = Arc::clone(&running);
    ctrlc::set_handler(move || signal_running.store(false, Ordering::SeqCst)).map_err(|error| {
        ConfigError::InvalidValue {
            key: "alerts.signal_handler".to_string(),
            message: error.to_string(),
        }
    })?;

    let _pid_guard = claim_pid_file(pid)?;
    persist_snapshot(pid, &manager)?;

    let interval = Duration::from_secs(interval_secs);
    while running.load(Ordering::SeqCst) {
        process_runtime_commands(&mut manager)?;

        let current_device_count = match nvml.device_count() {
            Ok(count) => count,
            Err(error) => {
                log::error!("Failed to enumerate GPUs: {}", error);
                persist_snapshot(pid, &manager)?;
                wait_while_running(&running, interval);
                continue;
            }
        };
        let mut present_uuids = HashSet::with_capacity(current_device_count as usize);
        let mut inventory_complete = true;
        for gpu_index in 0..current_device_count {
            let device = match nvml.device_by_index(gpu_index) {
                Ok(device) => device,
                Err(error) => {
                    log::error!("Failed to open GPU {}: {}", gpu_index, error);
                    inventory_complete = false;
                    continue;
                }
            };
            match crate::nvml::GpuDevice::uuid(&device) {
                Ok(uuid) => {
                    present_uuids.insert(uuid);
                }
                Err(error) => {
                    log::error!("Failed to identify GPU {}: {}", gpu_index, error);
                    inventory_complete = false;
                }
            }
            match manager.evaluate(&device, gpu_index) {
                Ok(new_alerts) => {
                    for alert in &new_alerts {
                        if let Err(error) = notifier.notify_all(alert) {
                            log::error!("Failed to send alert {}: {}", alert.id, error);
                            manager.retry_notification(&alert.id);
                        }
                    }
                }
                Err(error) => {
                    log::error!("Failed to evaluate GPU {} alerts: {}", gpu_index, error);
                }
            }
        }
        if inventory_complete {
            manager.reconcile_present_gpus(&present_uuids);
        }

        persist_snapshot(pid, &manager)?;
        wait_while_running(&running, interval);
    }

    // Consume any command that raced with shutdown and leave a final readable snapshot.
    process_runtime_commands(&mut manager)?;
    persist_snapshot(0, &manager)?;
    Ok(())
}

fn wait_while_running(running: &AtomicBool, duration: Duration) {
    let Some(deadline) = std::time::Instant::now().checked_add(duration) else {
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        return;
    };
    while running.load(Ordering::SeqCst) {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(100)));
    }
}

fn run_alert_stop(format: OutputFormat) -> Result<()> {
    let Some(pid) = running_daemon_pid()? else {
        return Err(DomainError::InvalidValue("alert daemon is not running".to_string()).into());
    };

    send_terminate(pid)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while is_alert_daemon_process(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if is_alert_daemon_process(pid) {
        return Err(DomainError::InvalidValue(format!(
            "alert daemon PID {pid} did not stop within 5 seconds"
        ))
        .into());
    }

    let _ = fs::remove_file(pid_path());
    print_output(
        &Message {
            message: format!("Stopped alert daemon PID {pid}"),
            success: true,
        },
        format,
    )?;
    Ok(())
}

fn run_alert_list(
    show_all: bool,
    severity_filter: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let snapshot = read_snapshot()?;
    let severity = severity_filter.map(parse_severity).transpose()?;
    let mut alerts = snapshot.active_alerts;
    if show_all {
        alerts.extend(snapshot.history);
    }
    alerts.retain(|alert| severity.is_none_or(|severity| alert.severity == severity));
    alerts.sort_by(|left, right| left.id.cmp(&right.id));

    match format {
        OutputFormat::Json => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            serde_json::to_writer_pretty(&mut handle, &alerts).map_err(ConfigError::from)?;
            writeln!(handle)?;
        }
        OutputFormat::Table => {
            if alerts.is_empty() {
                println!("No matching alerts");
            } else {
                for alert in alerts {
                    println!(
                        "{} [{}] GPU {} {}: {}",
                        alert.id, alert.state, alert.gpu_index, alert.severity, alert.message
                    );
                }
            }
        }
        OutputFormat::Compact => {
            for alert in alerts {
                println!(
                    "{}:{}:{}:{}",
                    alert.id, alert.state, alert.gpu_index, alert.severity
                );
            }
        }
    }
    Ok(())
}

fn run_alert_ack(alert_id: &str, format: OutputFormat) -> Result<()> {
    ensure_active_alert_exists(alert_id)?;
    enqueue_runtime_command(&AlertRuntimeCommand::Acknowledge {
        alert_id: alert_id.to_string(),
    })?;
    print_output(
        &Message {
            message: format!("Queued acknowledgement for alert {alert_id}"),
            success: true,
        },
        format,
    )?;
    Ok(())
}

fn run_alert_silence(alert_id: &str, format: OutputFormat) -> Result<()> {
    ensure_active_alert_exists(alert_id)?;
    enqueue_runtime_command(&AlertRuntimeCommand::Silence {
        alert_id: alert_id.to_string(),
    })?;
    print_output(
        &Message {
            message: format!("Queued silence for alert {alert_id}"),
            success: true,
        },
        format,
    )?;
    Ok(())
}

fn run_alert_clear(format: OutputFormat) -> Result<()> {
    require_running_daemon()?;
    enqueue_runtime_command(&AlertRuntimeCommand::ClearHistory {})?;
    print_output(
        &Message {
            message: "Queued clearing of resolved alert history".to_string(),
            success: true,
        },
        format,
    )?;
    Ok(())
}

fn process_runtime_commands(manager: &mut AlertManager) -> Result<()> {
    let directory = command_dir();
    if !directory.exists() {
        return Ok(());
    }

    let mut paths: Vec<_> = fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();

    for path in paths {
        let result = fs::read_to_string(&path)
            .map_err(crate::error::AppError::from)
            .and_then(|contents| {
                serde_json::from_str::<AlertRuntimeCommand>(&contents)
                    .map_err(ConfigError::from)
                    .map_err(crate::error::AppError::from)
            })
            .and_then(|command| match command {
                AlertRuntimeCommand::Acknowledge { alert_id } => {
                    manager.acknowledge_alert(&alert_id)
                }
                AlertRuntimeCommand::Silence { alert_id } => manager.silence_alert(&alert_id),
                AlertRuntimeCommand::ClearHistory {} => {
                    manager.clear_history();
                    Ok(())
                }
            });

        if let Err(error) = result {
            log::error!(
                "Failed to process alert command {}: {}",
                path.display(),
                error
            );
        }
        fs::remove_file(path)?;
    }
    Ok(())
}

fn ensure_active_alert_exists(alert_id: &str) -> Result<()> {
    let pid = require_running_daemon()?;
    let snapshot = read_snapshot()?;
    if snapshot.pid != pid {
        return Err(DomainError::InvalidValue(
            "alert daemon state is still initializing; try again shortly".to_string(),
        )
        .into());
    }
    if snapshot
        .active_alerts
        .iter()
        .any(|alert| alert.id == alert_id)
    {
        Ok(())
    } else {
        Err(DomainError::InvalidValue(format!("active alert '{}' was not found", alert_id)).into())
    }
}

fn enqueue_runtime_command(command: &AlertRuntimeCommand) -> Result<()> {
    static COMMAND_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    ensure_runtime_dirs()?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = command_dir().join(format!(
        "{}-{}-{}-{}.json",
        timestamp,
        std::process::id(),
        COMMAND_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        match command {
            AlertRuntimeCommand::Acknowledge { .. } => "ack",
            AlertRuntimeCommand::Silence { .. } => "silence",
            AlertRuntimeCommand::ClearHistory {} => "clear",
        }
    ));
    let contents = serde_json::to_vec(command).map_err(ConfigError::from)?;
    let temporary = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&contents)?;
    file.sync_all()?;
    drop(file);
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn persist_snapshot(pid: u32, manager: &AlertManager) -> Result<()> {
    ensure_runtime_dirs()?;
    let mut active_alerts: Vec<_> = manager.active_alerts().into_iter().cloned().collect();
    active_alerts.sort_by(|left, right| left.id.cmp(&right.id));
    let snapshot = AlertRuntimeSnapshot {
        pid,
        updated_at_unix_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        active_alerts,
        history: manager.history().to_vec(),
    };
    let contents = serde_json::to_vec_pretty(&snapshot).map_err(ConfigError::from)?;
    let temporary = runtime_dir().join(format!("state-{}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    if let Err(error) = fs::rename(&temporary, state_path()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn read_snapshot() -> Result<AlertRuntimeSnapshot> {
    let path = state_path();
    let contents = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            crate::error::AppError::Domain(DomainError::InvalidValue(
                "no alert runtime state exists; start the alert daemon first".to_string(),
            ))
        } else {
            error.into()
        }
    })?;
    serde_json::from_str(&contents)
        .map_err(ConfigError::from)
        .map_err(Into::into)
}

fn read_snapshot_if_exists() -> Result<Option<AlertRuntimeSnapshot>> {
    let path = state_path();
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(ConfigError::from)
        .map_err(Into::into)
}

fn claim_pid_file(pid: u32) -> Result<PidFileGuard> {
    ensure_runtime_dirs()?;
    let path = pid_path();
    if let Some(existing) = read_pid_from(&path)? {
        if is_alert_daemon_process(existing) {
            return Err(DomainError::InvalidValue(format!(
                "alert daemon is already running with PID {existing}"
            ))
            .into());
        }
        fs::remove_file(&path)?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    Ok(PidFileGuard { path, pid })
}

fn running_daemon_pid() -> Result<Option<u32>> {
    let path = pid_path();
    let Some(pid) = read_pid_from(&path)? else {
        return Ok(None);
    };
    if is_alert_daemon_process(pid) {
        Ok(Some(pid))
    } else {
        let _ = fs::remove_file(path);
        Ok(None)
    }
}

fn require_running_daemon() -> Result<u32> {
    running_daemon_pid()?
        .ok_or_else(|| DomainError::InvalidValue("alert daemon is not running".to_string()).into())
}

fn read_pid_from(path: &Path) -> Result<Option<u32>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let pid = contents.trim().parse::<u32>().map_err(|error| {
        ConfigError::ParseError(format!("invalid alert daemon PID file: {error}"))
    })?;
    Ok(Some(pid))
}

#[cfg(unix)]
fn send_terminate(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(DomainError::InvalidValue(format!("failed to signal alert daemon PID {pid}")).into())
    }
}

#[cfg(not(unix))]
fn send_terminate(_pid: u32) -> Result<()> {
    Err(DomainError::InvalidValue(
        "stopping the background alert daemon is not supported on this platform".to_string(),
    )
    .into())
}

#[cfg(target_os = "linux")]
fn is_alert_daemon_process(pid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;

    // A PID file can outlive its process and the PID can then be reused. Match
    // the executable inode before ever treating the process as our daemon.
    let Ok(process_executable) = fs::metadata(format!("/proc/{pid}/exe")) else {
        return false;
    };
    let Ok(current_executable_path) = std::env::current_exe() else {
        return false;
    };
    let Ok(current_executable) = fs::metadata(current_executable_path) else {
        return false;
    };
    if process_executable.dev() != current_executable.dev()
        || process_executable.ino() != current_executable.ino()
    {
        return false;
    }

    let Ok(command_line) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let arguments: Vec<_> = command_line
        .split(|byte| *byte == 0)
        .filter_map(|argument| std::str::from_utf8(argument).ok())
        .collect();
    arguments.contains(&"alerts")
        && arguments.contains(&"start")
        && arguments.contains(&"--foreground")
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn is_alert_daemon_process(pid: u32) -> bool {
    if !process_is_running(pid) {
        return false;
    }
    let pid_string = pid.to_string();
    Command::new("ps")
        .args(["-p", pid_string.as_str(), "-o", "command="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|command| {
            command.contains("alerts")
                && command.contains("start")
                && command.contains("--foreground")
        })
}

#[cfg(not(unix))]
fn is_alert_daemon_process(_pid: u32) -> bool {
    false
}

fn ensure_runtime_dirs() -> Result<()> {
    fs::create_dir_all(command_dir())?;
    Ok(())
}

fn runtime_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nvctl")
        .join("alerts")
}

fn command_dir() -> PathBuf {
    runtime_dir().join("commands")
}

fn pid_path() -> PathBuf {
    runtime_dir().join("daemon.pid")
}

fn state_path() -> PathBuf {
    runtime_dir().join("state.json")
}

fn parse_severity(value: &str) -> Result<AlertSeverity> {
    match value.to_ascii_lowercase().as_str() {
        "info" => Ok(AlertSeverity::Info),
        "warning" => Ok(AlertSeverity::Warning),
        "critical" => Ok(AlertSeverity::Critical),
        "emergency" => Ok(AlertSeverity::Emergency),
        _ => Err(DomainError::InvalidValue(format!(
            "unknown alert severity '{}'; expected info, warning, critical, or emergency",
            value
        ))
        .into()),
    }
}

fn load_alert_config_for_inspection(config_path: Option<String>) -> Result<(PathBuf, AlertConfig)> {
    if let Some(config_path) = config_path {
        let path = PathBuf::from(config_path);
        let config = AlertConfig::load(&path)?;
        return Ok((path, config));
    }

    let path = AlertConfig::default_path();
    let config = if path.exists() {
        AlertConfig::load(&path)?
    } else {
        AlertConfig::default()
    };
    Ok((path, config))
}

fn run_alert_rules(config_path: Option<String>, format: OutputFormat) -> Result<()> {
    let (config_path, alert_config) = load_alert_config_for_inspection(config_path)?;

    match format {
        OutputFormat::Json => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            serde_json::to_writer_pretty(&mut handle, &alert_config.rules)
                .map_err(ConfigError::from)?;
            writeln!(handle)?;
        }
        OutputFormat::Table | OutputFormat::Compact => {
            println!("Alert Rules ({})", config_path.display());
            println!(
                "enabled={}, interval={}s, max_history={}",
                alert_config.settings.enabled,
                alert_config.settings.check_interval_secs,
                alert_config.settings.max_history
            );
            for rule in &alert_config.rules {
                let status = if rule.enabled { "enabled" } else { "disabled" };
                println!(
                    "{} [{}] {} - {} {:?}; GPU {}",
                    status, rule.severity, rule.name, rule.metric, rule.condition, rule.gpu_filter
                );
            }
        }
    }
    Ok(())
}

fn run_alert_test(config_path: Option<String>) -> Result<()> {
    let (_, alert_config) = load_alert_config_for_inspection(config_path)?;
    let rules = alert_config.to_alert_rules()?;
    println!("Configuration is valid: {} enabled rules", rules.len());
    for (index, rule) in rules.iter().enumerate() {
        println!(
            "{}. {} [{}] - metric: {}, condition: {}",
            index + 1,
            rule.name,
            rule.severity,
            rule.metric,
            rule.condition
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_parser_is_case_insensitive() {
        assert_eq!(parse_severity("WARNING").unwrap(), AlertSeverity::Warning);
        assert!(parse_severity("fatal").is_err());
    }

    #[test]
    fn runtime_command_round_trips() {
        let command = AlertRuntimeCommand::Acknowledge {
            alert_id: "high-temp-0".to_string(),
        };
        let json = serde_json::to_string(&command).unwrap();
        let parsed: AlertRuntimeCommand = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            parsed,
            AlertRuntimeCommand::Acknowledge { alert_id } if alert_id == "high-temp-0"
        ));
    }

    #[test]
    fn explicit_missing_alert_config_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.toml");
        assert!(load_alert_config_for_inspection(Some(missing.display().to_string())).is_err());
    }

    #[test]
    fn runtime_commands_reject_unknown_fields() {
        assert!(serde_json::from_str::<AlertRuntimeCommand>(
            r#"{"command":"clear_history","unexpected":true}"#
        )
        .is_err());
    }
}
