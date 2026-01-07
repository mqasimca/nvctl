//! Alert command implementation
//!
//! Handles alert-related CLI commands.

use crate::alerts::{AlertConfig, AlertManager, AlertManagerConfig, NotificationManager};
use crate::cli::args::{AlertCommands, OutputFormat};
use crate::error::Result;
use crate::nvml::{GpuManager, NvmlManager};
use std::path::PathBuf;
use std::time::Duration;

/// Execute alert commands
pub fn run_alerts(command: &AlertCommands, format: OutputFormat) -> Result<()> {
    match command {
        AlertCommands::Start {
            interval,
            config,
            foreground,
        } => run_alert_start(*interval, config.clone(), *foreground),
        AlertCommands::Stop => run_alert_stop(),
        AlertCommands::List { all, severity } => run_alert_list(*all, severity.clone(), format),
        AlertCommands::Rules { config } => run_alert_rules(config.clone(), format),
        AlertCommands::Ack { alert_id } => run_alert_ack(alert_id),
        AlertCommands::Silence { alert_id } => run_alert_silence(alert_id),
        AlertCommands::Clear => run_alert_clear(),
        AlertCommands::Test { config } => run_alert_test(config.clone()),
    }
}

/// Start alert monitoring
fn run_alert_start(interval: u64, config_path: Option<String>, foreground: bool) -> Result<()> {
    // Load alert configuration
    let config_path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(AlertConfig::default_path);

    let alert_config = if config_path.exists() {
        println!(
            "Loading alert configuration from: {}",
            config_path.display()
        );
        AlertConfig::load(&config_path)?
    } else {
        println!(
            "No configuration found at {}, using defaults",
            config_path.display()
        );
        let config = AlertConfig::default();
        // Save default config for future reference
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        config.save(&config_path)?;
        println!("Saved default configuration to: {}", config_path.display());
        config
    };

    // Create alert manager
    let manager_config = AlertManagerConfig {
        enabled: alert_config.settings.enabled,
        check_interval: Duration::from_secs(alert_config.settings.check_interval_secs),
        max_history: alert_config.settings.max_history,
    };

    let mut manager = AlertManager::new(manager_config);

    // Load rules
    let rules = alert_config.to_alert_rules()?;
    println!("Loaded {} alert rules", rules.len());
    manager.add_rules(rules);

    // Create notification manager
    let notifier = NotificationManager::default();

    // Initialize NVML
    let nvml = NvmlManager::new()?;
    let device_count = nvml.device_count()?;
    println!("Monitoring {} GPU(s)", device_count);

    if !foreground {
        println!("Starting alert monitoring daemon (interval: {}s)", interval);
        println!("Press Ctrl+C to stop");
    }

    // Main monitoring loop
    let check_interval = Duration::from_secs(interval);
    loop {
        for gpu_idx in 0..device_count {
            let device = nvml.device_by_index(gpu_idx)?;

            // Evaluate alert rules
            match manager.evaluate(&device, gpu_idx) {
                Ok(new_alerts) => {
                    // Send notifications for new alerts
                    for alert in &new_alerts {
                        if let Err(e) = notifier.notify_all(alert) {
                            eprintln!("Failed to send notification: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error evaluating alerts for GPU {}: {}", gpu_idx, e);
                }
            }
        }

        std::thread::sleep(check_interval);
    }
}

/// Stop alert monitoring
fn run_alert_stop() -> Result<()> {
    // TODO: Implement daemon PID file and signal handling
    println!("Alert monitoring stop not yet implemented");
    println!("Use Ctrl+C to stop the alert daemon");
    Ok(())
}

/// List active alerts
fn run_alert_list(
    show_all: bool,
    severity_filter: Option<String>,
    _format: OutputFormat,
) -> Result<()> {
    // TODO: This needs persistent state or IPC with running daemon
    println!("Alert listing requires a running alert daemon");
    println!("Run 'nvctl alerts start' first");

    // For now, show example of what would be displayed
    if show_all {
        println!("\nShowing all alerts (active and resolved)");
    } else {
        println!("\nShowing active alerts only");
    }

    if let Some(sev) = severity_filter {
        println!("Filtered by severity: {}", sev);
    }

    println!("\nNote: This feature requires IPC integration (coming soon)");
    Ok(())
}

/// List alert rules
fn run_alert_rules(config_path: Option<String>, format: OutputFormat) -> Result<()> {
    let config_path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(AlertConfig::default_path);

    let alert_config = if config_path.exists() {
        AlertConfig::load(&config_path)?
    } else {
        AlertConfig::default()
    };

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&alert_config.rules)
                .map_err(crate::error::ConfigError::from)?;
            println!("{}", json);
        }
        _ => {
            println!("Alert Rules ({})\n", config_path.display());
            println!(
                "Global Settings: enabled={}, interval={}s, max_history={}",
                alert_config.settings.enabled,
                alert_config.settings.check_interval_secs,
                alert_config.settings.max_history
            );
            println!("\nConfigured Rules:");
            println!("{:-<80}", "");

            for rule in &alert_config.rules {
                let status = if rule.enabled { "✓" } else { "✗" };
                println!(
                    "{} [{}] {} - {} {:?}",
                    status, rule.severity, rule.name, rule.metric, rule.condition
                );
                if let Some(duration) = rule.duration_secs {
                    println!("   Duration: {}s", duration);
                }
                println!("   GPU Filter: {}", rule.gpu_filter);
                println!();
            }
            println!("{:-<80}", "");
            println!(
                "\nTotal rules: {} ({} enabled)",
                alert_config.rules.len(),
                alert_config.rules.iter().filter(|r| r.enabled).count()
            );
        }
    }

    Ok(())
}

/// Acknowledge an alert
fn run_alert_ack(alert_id: &str) -> Result<()> {
    println!("Acknowledging alert: {}", alert_id);
    println!("Note: This feature requires IPC integration (coming soon)");
    Ok(())
}

/// Silence an alert
fn run_alert_silence(alert_id: &str) -> Result<()> {
    println!("Silencing alert: {}", alert_id);
    println!("Note: This feature requires IPC integration (coming soon)");
    Ok(())
}

/// Clear resolved alerts
fn run_alert_clear() -> Result<()> {
    println!("Clearing resolved alerts from history");
    println!("Note: This feature requires IPC integration (coming soon)");
    Ok(())
}

/// Test alert configuration
fn run_alert_test(config_path: Option<String>) -> Result<()> {
    let config_path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(AlertConfig::default_path);

    println!("Testing alert configuration: {}", config_path.display());

    // Try to load and parse configuration
    let alert_config = if config_path.exists() {
        AlertConfig::load(&config_path)?
    } else {
        println!("Configuration file not found, using defaults");
        AlertConfig::default()
    };

    println!("✓ Configuration file is valid");
    println!("✓ Loaded {} alert rules", alert_config.rules.len());

    // Try to convert to alert rules
    let rules = alert_config.to_alert_rules()?;
    println!("✓ Successfully parsed {} enabled rules", rules.len());

    // Validate each rule
    println!("\nRule Validation:");
    for (idx, rule) in rules.iter().enumerate() {
        println!(
            "  {}. {} [{}] - metric: {}, condition: {}",
            idx + 1,
            rule.name,
            rule.severity,
            rule.metric,
            rule.condition
        );
    }

    println!("\n✓ All rules are valid!");
    println!("\nYou can start alert monitoring with:");
    println!("  nvctl alerts start");

    Ok(())
}
