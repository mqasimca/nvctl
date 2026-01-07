//! Alert service
//!
//! Manages alert evaluation and notification during monitoring loops.

use crate::alerts::{Alert, AlertManager, AlertManagerConfig, AlertRule, NotificationManager};
use crate::error::AppError;
use crate::nvml::GpuDevice;

/// Alert service for GPU monitoring
pub struct AlertService {
    manager: AlertManager,
    notifier: NotificationManager,
    enabled: bool,
}

impl AlertService {
    /// Create a new alert service
    pub fn new(config: AlertManagerConfig, rules: Vec<AlertRule>) -> Self {
        let enabled = config.enabled;
        let mut manager = AlertManager::new(config);
        manager.add_rules(rules);
        let notifier = NotificationManager::default();

        Self {
            manager,
            notifier,
            enabled,
        }
    }

    /// Create a disabled alert service (no-op)
    pub fn disabled() -> Self {
        Self {
            manager: AlertManager::new(AlertManagerConfig {
                enabled: false,
                check_interval: std::time::Duration::from_secs(5),
                max_history: 0,
            }),
            notifier: NotificationManager::default(),
            enabled: false,
        }
    }

    /// Evaluate alerts for a device and send notifications
    pub fn evaluate<D: GpuDevice>(&mut self, device: &D, gpu_index: u32) -> Result<(), AppError> {
        if !self.enabled {
            return Ok(());
        }

        let new_alerts = self.manager.evaluate(device, gpu_index)?;

        // Send notifications for new alerts
        for alert in &new_alerts {
            if let Err(e) = self.notifier.notify_all(alert) {
                log::warn!("Failed to send notification for alert {}: {}", alert.id, e);
            }
        }

        Ok(())
    }

    /// Get active alerts
    pub fn active_alerts(&self) -> Vec<&Alert> {
        self.manager.active_alerts()
    }

    /// Get alert history
    pub fn history(&self) -> &[Alert] {
        self.manager.history()
    }

    /// Check if service is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::{AlertRule, AlertSeverity, Condition, MetricType};
    use crate::domain::Temperature;
    use crate::mock::MockDevice;

    #[test]
    fn test_alert_service_creation() {
        let config = AlertManagerConfig {
            enabled: true,
            check_interval: std::time::Duration::from_secs(5),
            max_history: 100,
        };

        let rule = AlertRule::new(
            "test".to_string(),
            "Test Alert".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        let service = AlertService::new(config, vec![rule]);
        assert!(service.is_enabled());
    }

    #[test]
    fn test_alert_service_disabled() {
        let service = AlertService::disabled();
        assert!(!service.is_enabled());
    }

    #[test]
    fn test_alert_service_evaluate() {
        let config = AlertManagerConfig {
            enabled: true,
            check_interval: std::time::Duration::from_secs(5),
            max_history: 100,
        };

        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        let mut service = AlertService::new(config, vec![rule]);

        // Create a mock device with high temperature
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));

        // Evaluate should succeed
        let result = service.evaluate(&device, 0);
        assert!(result.is_ok());

        // Should have one active alert
        assert_eq!(service.active_alerts().len(), 1);
    }

    #[test]
    fn test_alert_service_disabled_no_op() {
        let mut service = AlertService::disabled();
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(100));

        // Evaluate should succeed but do nothing
        let result = service.evaluate(&device, 0);
        assert!(result.is_ok());

        // Should have no active alerts
        assert_eq!(service.active_alerts().len(), 0);
    }
}
