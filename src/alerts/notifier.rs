//! Alert notification system
//!
//! Provides notification channels for alerts including terminal, desktop, email, and webhooks.

use super::types::{Alert, AlertSeverity};
use crate::error::Result;
use std::io::{self, IsTerminal, Write};

/// Notification channel trait
pub trait Notifier: Send + Sync {
    /// Send a notification for an alert
    fn notify(&self, alert: &Alert) -> Result<()>;

    /// Channel name for identification
    fn name(&self) -> &str;
}

/// Terminal/console notifier
///
/// Outputs alerts to stdout/stderr with colored formatting
pub struct TerminalNotifier {
    /// Use stderr instead of stdout
    use_stderr: bool,
    /// Use colors (ANSI escape codes)
    use_colors: bool,
}

impl TerminalNotifier {
    /// Create a new terminal notifier
    pub fn new() -> Self {
        Self {
            use_stderr: true,
            use_colors: Self::supports_color(true),
        }
    }

    /// Create a notifier that uses stdout
    pub fn stdout() -> Self {
        Self {
            use_stderr: false,
            use_colors: Self::supports_color(false),
        }
    }

    /// Create a notifier without colors
    pub fn no_color() -> Self {
        Self {
            use_stderr: true,
            use_colors: false,
        }
    }

    /// Check if terminal supports colors
    fn supports_color(use_stderr: bool) -> bool {
        let is_terminal = if use_stderr {
            io::stderr().is_terminal()
        } else {
            io::stdout().is_terminal()
        };
        is_terminal
            && std::env::var("TERM")
                .map(|term| term != "dumb")
                .unwrap_or(false)
    }

    /// Format alert with colors
    fn format_alert(&self, alert: &Alert) -> String {
        let severity_str = self.format_severity(alert.severity);
        let timestamp = alert
            .fired_at
            .or(Some(alert.started_at))
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                let seconds_of_day = d.as_secs() % (24 * 60 * 60);
                let hours = seconds_of_day / 3600;
                let mins = (seconds_of_day % 3600) / 60;
                let secs = seconds_of_day % 60;
                format!("{:02}:{:02}:{:02} UTC", hours, mins, secs)
            })
            .unwrap_or_else(|| "??:??:?? UTC".to_string());

        format!(
            "[{}] {} GPU {}: {}",
            timestamp, severity_str, alert.gpu_index, alert.message
        )
    }

    /// Format severity with colors
    fn format_severity(&self, severity: AlertSeverity) -> String {
        if !self.use_colors {
            return format!("{}", severity);
        }

        let (color_code, text) = match severity {
            AlertSeverity::Info => ("\x1b[36m", "INFO"), // Cyan
            AlertSeverity::Warning => ("\x1b[33m", "WARNING"), // Yellow
            AlertSeverity::Critical => ("\x1b[31m", "CRITICAL"), // Red
            AlertSeverity::Emergency => ("\x1b[35m\x1b[1m", "EMERGENCY"), // Bold Magenta
        };

        format!("{}{}\x1b[0m", color_code, text)
    }
}

impl Default for TerminalNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for TerminalNotifier {
    fn notify(&self, alert: &Alert) -> Result<()> {
        let message = self.format_alert(alert);

        if self.use_stderr {
            let stderr = io::stderr();
            let mut handle = stderr.lock();
            writeln!(handle, "{}", message)?;
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writeln!(handle, "{}", message)?;
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "terminal"
    }
}

/// Notification manager
///
/// Manages multiple notification channels and dispatches alerts to them
pub struct NotificationManager {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            notifiers: Vec::new(),
        }
    }

    /// Add a notifier
    pub fn add_notifier(&mut self, notifier: Box<dyn Notifier>) {
        self.notifiers.push(notifier);
    }

    /// Send notification to all channels
    pub fn notify_all(&self, alert: &Alert) -> Result<()> {
        let mut first_error = None;
        let mut delivered = false;
        for notifier in &self.notifiers {
            match notifier.notify(alert) {
                Ok(()) => delivered = true,
                Err(error) => {
                    eprintln!("Failed to notify via {}: {}", notifier.name(), error);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }

        if delivered {
            Ok(())
        } else {
            Err(first_error.unwrap_or_else(|| {
                io::Error::other("no notification channels are configured").into()
            }))
        }
    }

    /// Send notifications for multiple alerts
    pub fn notify_batch(&self, alerts: &[Alert]) -> Result<()> {
        let mut first_error = None;
        for alert in alerts {
            if let Err(error) = self.notify_all(alert) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Get number of active notifiers
    pub fn notifier_count(&self) -> usize {
        self.notifiers.len()
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        let mut manager = Self::new();
        // Add default terminal notifier
        manager.add_notifier(Box::new(TerminalNotifier::new()));
        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::types::{AlertRule, Condition, MetricType};

    struct FailingNotifier;

    struct SuccessfulNotifier;

    impl Notifier for FailingNotifier {
        fn notify(&self, _alert: &Alert) -> Result<()> {
            Err(io::Error::other("notification failed").into())
        }

        fn name(&self) -> &str {
            "failing"
        }
    }

    impl Notifier for SuccessfulNotifier {
        fn notify(&self, _alert: &Alert) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> &str {
            "successful"
        }
    }

    #[test]
    fn test_terminal_notifier_creation() {
        let notifier = TerminalNotifier::new();
        assert_eq!(notifier.name(), "terminal");
        assert!(notifier.use_stderr);
    }

    #[test]
    fn test_terminal_notifier_stdout() {
        let notifier = TerminalNotifier::stdout();
        assert!(!notifier.use_stderr);
    }

    #[test]
    fn test_terminal_notifier_no_color() {
        let notifier = TerminalNotifier::no_color();
        assert!(!notifier.use_colors);
    }

    #[test]
    fn test_format_severity() {
        let notifier = TerminalNotifier::no_color();
        assert_eq!(notifier.format_severity(AlertSeverity::Info), "INFO");
        assert_eq!(notifier.format_severity(AlertSeverity::Warning), "WARNING");
        assert_eq!(
            notifier.format_severity(AlertSeverity::Critical),
            "CRITICAL"
        );
    }

    #[test]
    fn terminal_timestamp_wraps_at_midnight_and_is_labeled_utc() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        let mut alert = Alert::new_pending(&rule, 0, 85.0, 80.0);
        alert.started_at =
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(25 * 3600 + 60 + 2);

        assert!(TerminalNotifier::no_color()
            .format_alert(&alert)
            .starts_with("[01:01:02 UTC]"));
    }

    #[test]
    fn test_notification_manager_creation() {
        let manager = NotificationManager::new();
        assert_eq!(manager.notifier_count(), 0);
    }

    #[test]
    fn test_notification_manager_default() {
        let manager = NotificationManager::default();
        assert_eq!(manager.notifier_count(), 1); // Default terminal notifier
    }

    #[test]
    fn test_notification_manager_add_notifier() {
        let mut manager = NotificationManager::new();
        manager.add_notifier(Box::new(TerminalNotifier::new()));
        assert_eq!(manager.notifier_count(), 1);
    }

    #[test]
    fn notification_failures_are_returned() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        let alert = Alert::new_pending(&rule, 0, 85.0, 80.0);
        let mut manager = NotificationManager::new();
        manager.add_notifier(Box::new(FailingNotifier));

        assert!(manager.notify_all(&alert).is_err());
    }

    #[test]
    fn notification_succeeds_when_any_channel_delivers() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        let alert = Alert::new_pending(&rule, 0, 85.0, 80.0);
        let mut manager = NotificationManager::new();
        manager.add_notifier(Box::new(FailingNotifier));
        manager.add_notifier(Box::new(SuccessfulNotifier));

        assert!(manager.notify_all(&alert).is_ok());
    }

    #[test]
    fn no_notification_channels_is_an_error() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        let alert = Alert::new_pending(&rule, 0, 85.0, 80.0);

        assert!(NotificationManager::new().notify_all(&alert).is_err());
    }

    #[test]
    fn test_notify() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        let alert = Alert::new_pending(&rule, 0, 85.0, 80.0);

        let notifier = TerminalNotifier::stdout(); // Use stdout to avoid cluttering test output
        let result = notifier.notify(&alert);
        assert!(result.is_ok());
    }

    #[test]
    fn test_notify_batch() {
        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        let alerts = vec![
            Alert::new_pending(&rule, 0, 85.0, 80.0),
            Alert::new_pending(&rule, 1, 90.0, 80.0),
        ];

        let manager = NotificationManager::default();
        let result = manager.notify_batch(&alerts);
        assert!(result.is_ok());
    }
}
