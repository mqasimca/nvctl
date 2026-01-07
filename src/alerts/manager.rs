//! Alert manager implementation
//!
//! Manages alert rules, evaluates conditions, and triggers notifications.

use super::types::{Alert, AlertRule, AlertState, MetricType};
use crate::error::Result;
use crate::nvml::GpuDevice;
use std::collections::HashMap;
use std::time::Duration;

/// Alert manager configuration
#[derive(Debug, Clone)]
pub struct AlertManagerConfig {
    /// Whether alerting is enabled
    pub enabled: bool,
    /// Check interval for evaluating rules
    pub check_interval: Duration,
    /// Maximum number of alerts to keep in history
    pub max_history: usize,
}

impl Default for AlertManagerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(5),
            max_history: 1000,
        }
    }
}

/// Alert manager
///
/// Manages alert rules and active alerts for all GPUs.
pub struct AlertManager {
    /// Alert rules
    rules: Vec<AlertRule>,
    /// Active alerts by GPU index
    active_alerts: HashMap<String, Alert>,
    /// Alert history (resolved alerts)
    history: Vec<Alert>,
    /// Configuration
    config: AlertManagerConfig,
}

impl AlertManager {
    /// Create a new alert manager
    pub fn new(config: AlertManagerConfig) -> Self {
        Self {
            rules: Vec::new(),
            active_alerts: HashMap::new(),
            history: Vec::new(),
            config,
        }
    }

    /// Add an alert rule
    pub fn add_rule(&mut self, rule: AlertRule) {
        self.rules.push(rule);
    }

    /// Add multiple alert rules
    pub fn add_rules(&mut self, rules: Vec<AlertRule>) {
        self.rules.extend(rules);
    }

    /// Get all rules
    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Get active alerts
    pub fn active_alerts(&self) -> Vec<&Alert> {
        self.active_alerts.values().collect()
    }

    /// Get alert history
    pub fn history(&self) -> &[Alert] {
        &self.history
    }

    /// Evaluate all rules for a GPU
    pub fn evaluate<D: GpuDevice>(&mut self, device: &D, gpu_index: u32) -> Result<Vec<Alert>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut new_alerts = Vec::new();
        let mut resolved_alerts = Vec::new();

        // Clone rules to avoid borrow issues
        let rules = self.rules.clone();

        for rule in &rules {
            if !rule.enabled || !rule.gpu_filter.matches(gpu_index) {
                continue;
            }

            // Extract metric value from device
            let value = match self.extract_metric_value(device, rule.metric) {
                Ok(Some(v)) => v,
                Ok(None) => continue, // Metric not available
                Err(_) => continue,   // Error reading metric
            };

            let threshold_value = self.get_threshold_value(&rule.condition);

            // Check if condition is met
            if rule.evaluate(value) {
                // Find or create alert
                let alert_key = format!("{}-{}", rule.id, gpu_index);

                if let Some(alert) = self.active_alerts.get_mut(&alert_key) {
                    // Update existing alert
                    alert.update_value(value);

                    // Check if alert should fire
                    if alert.should_fire(rule) && alert.state == AlertState::Pending {
                        alert.fire();
                        new_alerts.push(alert.clone());
                    }
                } else {
                    // Create new pending alert
                    let mut alert = Alert::new_pending(rule, gpu_index, value, threshold_value);

                    // Fire immediately if no duration requirement
                    if alert.should_fire(rule) {
                        alert.fire();
                        new_alerts.push(alert.clone());
                    }

                    self.active_alerts.insert(alert_key, alert);
                }
            } else {
                // Condition no longer met, resolve alert if exists
                let alert_key = format!("{}-{}", rule.id, gpu_index);
                if let Some(mut alert) = self.active_alerts.remove(&alert_key) {
                    if matches!(alert.state, AlertState::Firing | AlertState::Acknowledged) {
                        alert.resolve();
                        resolved_alerts.push(alert);
                    }
                }
            }
        }

        // Add resolved alerts to history
        for alert in resolved_alerts {
            self.add_to_history(alert);
        }

        Ok(new_alerts)
    }

    /// Extract metric value from device
    fn extract_metric_value<D: GpuDevice>(
        &self,
        device: &D,
        metric: MetricType,
    ) -> Result<Option<f64>> {
        Ok(Some(match metric {
            MetricType::Temperature => device.temperature()?.as_celsius() as f64,
            MetricType::MemoryTemperature => {
                if let Some(temp) = device.memory_temperature()? {
                    temp.as_celsius() as f64
                } else {
                    return Ok(None);
                }
            }
            MetricType::PowerUsage => device.power_usage()?.as_watts() as f64,
            MetricType::PowerPercent => {
                let usage = device.power_usage()?.as_watts();
                let limit = device.power_limit()?.as_watts();
                if limit > 0 {
                    (usage as f64 / limit as f64) * 100.0
                } else {
                    return Ok(None);
                }
            }
            MetricType::GpuUtilization => device.utilization()?.gpu as f64,
            MetricType::MemoryUtilization => device.utilization()?.memory as f64,
            MetricType::FanSpeed => {
                if let Ok(speed) = device.fan_speed(0) {
                    speed.as_percentage() as f64
                } else {
                    return Ok(None);
                }
            }
            MetricType::ClockSpeed => {
                use crate::domain::performance::ClockType;
                device.clock_speed(ClockType::Graphics)?.as_mhz() as f64
            }
            MetricType::EccCorrectableErrors => {
                if let Some(ecc) = device.ecc_errors()? {
                    ecc.correctable_current as f64
                } else {
                    return Ok(None);
                }
            }
            MetricType::EccUncorrectableErrors => {
                if let Some(ecc) = device.ecc_errors()? {
                    ecc.uncorrectable_current as f64
                } else {
                    return Ok(None);
                }
            }
            MetricType::PcieThroughput => {
                let metrics = device.pcie_metrics()?;
                (metrics.throughput.tx_bytes_per_sec() + metrics.throughput.rx_bytes_per_sec())
                    as f64
                    / 1024.0
                    / 1024.0 // Convert to MB/s
            }
            MetricType::PcieReplayCounter => {
                let metrics = device.pcie_metrics()?;
                metrics.replay_counter.count() as f64
            }
        }))
    }

    /// Get threshold value from condition
    fn get_threshold_value(&self, condition: &super::types::Condition) -> f64 {
        use super::types::Condition;
        match condition {
            Condition::GreaterThan(v) | Condition::LessThan(v) | Condition::Equals(v) => *v,
            Condition::InRange(min, max) | Condition::OutsideRange(min, max) => {
                (min + max) / 2.0 // Use midpoint
            }
        }
    }

    /// Add alert to history
    fn add_to_history(&mut self, alert: Alert) {
        self.history.push(alert);

        // Trim history if needed
        if self.history.len() > self.config.max_history {
            self.history
                .drain(0..self.history.len() - self.config.max_history);
        }
    }

    /// Acknowledge an alert
    pub fn acknowledge_alert(&mut self, alert_id: &str) -> Result<()> {
        if let Some(alert) = self.active_alerts.get_mut(alert_id) {
            alert.acknowledge();
        }
        Ok(())
    }

    /// Silence an alert
    pub fn silence_alert(&mut self, alert_id: &str) -> Result<()> {
        if let Some(alert) = self.active_alerts.get_mut(alert_id) {
            alert.silence();
        }
        Ok(())
    }

    /// Clear all alerts
    pub fn clear_all(&mut self) {
        self.active_alerts.clear();
    }

    /// Get alert count by severity
    pub fn count_by_severity(&self) -> HashMap<super::types::AlertSeverity, usize> {
        let mut counts = HashMap::new();
        for alert in self.active_alerts.values() {
            if matches!(alert.state, AlertState::Firing | AlertState::Acknowledged) {
                *counts.entry(alert.severity).or_insert(0) += 1;
            }
        }
        counts
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new(AlertManagerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::{AlertRule, AlertSeverity, Condition, GpuFilter};
    use crate::domain::Temperature;
    use crate::mock::MockDevice;

    #[test]
    fn test_alert_manager_creation() {
        let manager = AlertManager::default();
        assert!(manager.config.enabled);
        assert_eq!(manager.rules().len(), 0);
        assert_eq!(manager.active_alerts().len(), 0);
    }

    #[test]
    fn test_add_rules() {
        let mut manager = AlertManager::default();

        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        manager.add_rule(rule);
        assert_eq!(manager.rules().len(), 1);
    }

    #[test]
    fn test_evaluate_temperature_alert() {
        let mut manager = AlertManager::default();

        // Add high temperature rule
        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        manager.add_rule(rule);

        // Create mock device with high temperature
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));

        // Evaluate
        let new_alerts = manager.evaluate(&device, 0).unwrap();

        // Should fire immediately (no duration requirement)
        assert_eq!(new_alerts.len(), 1);
        assert_eq!(new_alerts[0].severity, AlertSeverity::Warning);
        assert_eq!(new_alerts[0].current_value, 85.0);
    }

    #[test]
    fn test_evaluate_alert_resolution() {
        let mut manager = AlertManager::default();

        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        manager.add_rule(rule);

        // High temperature - should fire
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));
        let alerts = manager.evaluate(&device, 0).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(manager.active_alerts().len(), 1);

        // Lower temperature - should resolve
        device.set_temperature(Temperature::new(75));
        let alerts = manager.evaluate(&device, 0).unwrap();
        assert_eq!(alerts.len(), 0);
        assert_eq!(manager.active_alerts().len(), 0);
        assert_eq!(manager.history().len(), 1);
    }

    #[test]
    fn test_gpu_filter() {
        let mut manager = AlertManager::default();

        // Rule only for GPU 1
        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        )
        .with_gpu_filter(GpuFilter::Index(1));

        manager.add_rule(rule);

        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));

        // Should not fire for GPU 0
        let alerts = manager.evaluate(&device, 0).unwrap();
        assert_eq!(alerts.len(), 0);

        // Should fire for GPU 1
        let alerts = manager.evaluate(&device, 1).unwrap();
        assert_eq!(alerts.len(), 1);
    }
}
