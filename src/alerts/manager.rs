//! Alert manager implementation
//!
//! Manages alert rules, evaluates conditions, and triggers notifications.

use super::types::{Alert, AlertRule, AlertState, MetricType};
use crate::error::{AppError, DomainError, NvmlError, Result};
use crate::nvml::GpuDevice;
use std::collections::{HashMap, HashSet};
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
    /// Active alerts by stable rule/GPU identifier
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
        if let Some(existing) = self
            .rules
            .iter_mut()
            .find(|existing| existing.id == rule.id)
        {
            *existing = rule;
        } else {
            self.rules.push(rule);
        }
    }

    /// Add multiple alert rules
    pub fn add_rules(&mut self, rules: Vec<AlertRule>) {
        for rule in rules {
            self.add_rule(rule);
        }
    }

    /// Get all rules
    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Get active alerts
    pub fn active_alerts(&self) -> Vec<&Alert> {
        let mut alerts: Vec<_> = self.active_alerts.values().collect();
        alerts.sort_by(|left, right| left.id.cmp(&right.id));
        alerts
    }

    /// Get alert history
    pub fn history(&self) -> &[Alert] {
        &self.history
    }

    /// Restore persisted runtime state, discarding alerts for removed rules.
    pub fn restore_state(&mut self, active: Vec<Alert>, mut history: Vec<Alert>) {
        let now = std::time::SystemTime::now();
        self.active_alerts = active
            .into_iter()
            .filter_map(|mut alert| {
                let valid = self.rules.iter().any(|rule| rule.id == alert.rule_id)
                    && alert
                        .gpu_uuid
                        .as_deref()
                        .is_some_and(|uuid| !uuid.trim().is_empty())
                    && alert.id
                        == Alert::stable_id(
                            &alert.rule_id,
                            alert.gpu_index,
                            alert.gpu_uuid.as_deref(),
                        )
                    && !matches!(alert.state, AlertState::Resolved);
                if !valid {
                    return None;
                }
                // A wall-clock correction must not leave a pending alert stuck
                // until a timestamp in the future is reached again.
                if alert.started_at.duration_since(now).is_ok() {
                    alert.started_at = now;
                }
                if alert.state == AlertState::Pending {
                    alert.fired_at = None;
                }
                Some((alert.id.clone(), alert))
            })
            .collect();
        history.retain(|alert| alert.state == AlertState::Resolved);
        if history.len() > self.config.max_history {
            history.drain(0..history.len() - self.config.max_history);
        }
        self.history = history;
    }

    /// Remove resolved alert history without touching active alerts.
    pub fn clear_history(&mut self) {
        self.history.clear();
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
        // GPU indices can change after resets and hot-plug events. UUIDs keep
        // active alerts attached to the same physical GPU.
        let gpu_uuid = device.uuid()?;

        for rule in &rules {
            let alert_key = Alert::stable_id(&rule.id, gpu_index, Some(&gpu_uuid));
            if !rule.enabled || !rule.gpu_filter.matches(gpu_index, Some(&gpu_uuid)) {
                if let Some(alert) = self.take_resolved_alert(&alert_key) {
                    resolved_alerts.push(alert);
                }
                continue;
            }

            // Extract metric value from device
            let value = match self.extract_metric_value(device, rule.metric) {
                Ok(Some(v)) => v,
                Ok(None) => continue, // Metric not available
                Err(AppError::Nvml(NvmlError::NotSupported(_))) => continue,
                Err(error) => return Err(error),
            };

            let threshold_value = self.get_threshold_value(&rule.condition);

            // Check if condition is met
            if rule.evaluate(value) {
                // Find or create alert
                if let Some(alert) = self.active_alerts.get_mut(&alert_key) {
                    // Update existing alert
                    alert.gpu_index = gpu_index;
                    alert.update_from_rule(rule, value, threshold_value);
                    if alert.state == AlertState::Pending
                        && alert
                            .started_at
                            .duration_since(std::time::SystemTime::now())
                            .is_ok()
                    {
                        // A backwards wall-clock adjustment must not leave a
                        // sustained alert pending until the old time returns.
                        alert.started_at = std::time::SystemTime::now();
                    }

                    // Check if alert should fire
                    if alert.should_fire(rule) && alert.state == AlertState::Pending {
                        alert.fire();
                        new_alerts.push(alert.clone());
                    }
                } else {
                    // Create new pending alert
                    let mut alert = Alert::new_pending_for_gpu(
                        rule,
                        gpu_index,
                        Some(&gpu_uuid),
                        value,
                        threshold_value,
                    );

                    // Fire immediately if no duration requirement
                    if alert.should_fire(rule) {
                        alert.fire();
                        new_alerts.push(alert.clone());
                    }

                    self.active_alerts.insert(alert.id.clone(), alert);
                }
            } else {
                // Condition no longer met, resolve alert if exists
                if let Some(alert) = self.take_resolved_alert(&alert_key) {
                    resolved_alerts.push(alert);
                }
            }
        }

        // Add resolved alerts to history
        for alert in resolved_alerts {
            self.add_to_history(alert);
        }

        Ok(new_alerts)
    }

    /// Resolve alerts belonging to GPUs that are no longer present.
    ///
    /// Callers should only invoke this after a complete, successful inventory
    /// pass. Otherwise a transient enumeration failure could resolve a valid
    /// alert.
    pub fn reconcile_present_gpus(&mut self, present_uuids: &HashSet<String>) {
        let missing_alert_ids: Vec<_> = self
            .active_alerts
            .values()
            .filter(|alert| {
                alert
                    .gpu_uuid
                    .as_ref()
                    .is_none_or(|uuid| !present_uuids.contains(uuid))
            })
            .map(|alert| alert.id.clone())
            .collect();

        for alert_id in missing_alert_ids {
            if let Some(alert) = self.take_resolved_alert(&alert_id) {
                self.add_to_history(alert);
            }
        }
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
                let usage = device.power_usage()?.as_milliwatts();
                let limit = device.power_limit()?.as_milliwatts();
                if limit > 0 {
                    (usage as f64 / limit as f64) * 100.0
                } else {
                    return Ok(None);
                }
            }
            MetricType::GpuUtilization => device.utilization()?.gpu as f64,
            MetricType::MemoryUtilization => device.utilization()?.memory as f64,
            MetricType::FanSpeed => {
                if device.fan_count()? == 0 {
                    return Ok(None);
                }
                device.fan_speed(0)?.as_percentage() as f64
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
                metrics.throughput.tx_mbps() + metrics.throughput.rx_mbps()
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
                min.mul_add(0.5, max * 0.5) // Use an overflow-resistant midpoint
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

    /// Remove an active alert, preserving fired alerts as resolved history.
    /// Pending alerts were never delivered, so they are simply discarded.
    fn take_resolved_alert(&mut self, alert_id: &str) -> Option<Alert> {
        let mut alert = self.active_alerts.remove(alert_id)?;
        if matches!(
            alert.state,
            AlertState::Firing | AlertState::Acknowledged | AlertState::Silenced
        ) {
            alert.resolve();
            Some(alert)
        } else {
            None
        }
    }

    /// Acknowledge an alert
    pub fn acknowledge_alert(&mut self, alert_id: &str) -> Result<()> {
        let alert = self.active_alerts.get_mut(alert_id).ok_or_else(|| {
            DomainError::InvalidValue(format!("active alert '{}' was not found", alert_id))
        })?;
        alert.acknowledge();
        Ok(())
    }

    /// Silence an alert
    pub fn silence_alert(&mut self, alert_id: &str) -> Result<()> {
        let alert = self.active_alerts.get_mut(alert_id).ok_or_else(|| {
            DomainError::InvalidValue(format!("active alert '{}' was not found", alert_id))
        })?;
        alert.silence();
        Ok(())
    }

    /// Return a newly fired alert to pending when every notification channel
    /// failed, allowing the next evaluation to retry delivery.
    pub fn retry_notification(&mut self, alert_id: &str) {
        if let Some(alert) = self.active_alerts.get_mut(alert_id) {
            if alert.state == AlertState::Firing {
                alert.state = AlertState::Pending;
                alert.fired_at = None;
            }
        }
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

    #[test]
    fn uuid_filter_and_alert_id_control_work() {
        let mut manager = AlertManager::default();
        manager.add_rule(
            AlertRule::new(
                "high-temp".to_string(),
                "High Temperature".to_string(),
                MetricType::Temperature,
                Condition::GreaterThan(80.0),
                AlertSeverity::Warning,
            )
            .with_gpu_filter(GpuFilter::Uuid("GPU-MOCK-0000".to_string())),
        );

        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));
        let alerts = manager.evaluate(&device, 0).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].id, "high-temp-GPU-MOCK-0000");

        manager.acknowledge_alert(&alerts[0].id).unwrap();
        assert_eq!(manager.active_alerts()[0].state, AlertState::Acknowledged);
        assert!(manager.acknowledge_alert("missing-alert").is_err());
    }

    #[test]
    fn failed_notification_can_fire_again_on_the_next_evaluation() {
        let mut manager = AlertManager::default();
        manager.add_rule(AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));

        let first = manager.evaluate(&device, 0).unwrap();
        assert_eq!(first.len(), 1);
        manager.retry_notification(&first[0].id);
        assert_eq!(manager.active_alerts()[0].state, AlertState::Pending);

        let retried = manager.evaluate(&device, 0).unwrap();
        assert_eq!(retried.len(), 1);
        assert_eq!(retried[0].id, first[0].id);
        assert_eq!(manager.active_alerts()[0].state, AlertState::Firing);
    }

    #[test]
    fn restore_discards_invalid_state_and_trims_history() {
        let mut manager = AlertManager::new(AlertManagerConfig {
            max_history: 1,
            ..AlertManagerConfig::default()
        });
        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        manager.add_rule(rule.clone());

        let mut valid = Alert::new_pending_for_gpu(&rule, 0, Some("GPU-MOCK-0000"), 85.0, 80.0);
        valid.started_at = std::time::SystemTime::now() + Duration::from_secs(3600);
        valid.fired_at = Some(std::time::SystemTime::now());
        let mut malformed_id =
            Alert::new_pending_for_gpu(&rule, 1, Some("GPU-MOCK-0001"), 85.0, 80.0);
        malformed_id.id = "wrong".to_string();
        let stale_rule = AlertRule::new(
            "removed".to_string(),
            "Removed".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        let stale = Alert::new_pending(&stale_rule, 0, 85.0, 80.0);

        let mut older = Alert::new_pending(&rule, 2, 85.0, 80.0);
        older.resolve();
        let mut newest = Alert::new_pending(&rule, 3, 85.0, 80.0);
        newest.resolve();
        manager.restore_state(
            vec![valid, malformed_id, stale],
            vec![older, newest.clone()],
        );

        let active = manager.active_alerts();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "high-temp-GPU-MOCK-0000");
        assert_eq!(active[0].fired_at, None);
        assert!(active[0].started_at <= std::time::SystemTime::now());
        assert_eq!(manager.history(), &[newest]);
    }

    #[test]
    fn active_alert_follows_gpu_uuid_when_index_changes() {
        let mut manager = AlertManager::default();
        manager.add_rule(AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));

        let first = manager.evaluate(&device, 0).unwrap();
        assert_eq!(first.len(), 1);
        let id = first[0].id.clone();

        device.set_temperature(Temperature::new(88));
        assert!(manager.evaluate(&device, 7).unwrap().is_empty());
        let active = manager.active_alerts();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id);
        assert_eq!(active[0].gpu_index, 7);
        assert_eq!(active[0].gpu_uuid.as_deref(), Some("GPU-MOCK-0000"));
        assert_eq!(active[0].current_value, 88.0);
        assert!(active[0].message.contains("88.00"));

        manager.add_rule(AlertRule::new(
            "high-temp".to_string(),
            "Critical Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(86.0),
            AlertSeverity::Critical,
        ));
        assert!(manager.evaluate(&device, 7).unwrap().is_empty());
        let active = manager.active_alerts();
        assert_eq!(active[0].severity, AlertSeverity::Critical);
        assert_eq!(active[0].threshold_value, 86.0);
        assert!(active[0].message.contains("Critical Temperature"));
    }

    #[test]
    fn missing_gpu_alerts_are_resolved_without_leaving_pending_history() {
        let mut manager = AlertManager::default();
        manager.add_rule(AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));
        manager.evaluate(&device, 0).unwrap();

        manager.reconcile_present_gpus(&HashSet::new());

        assert!(manager.active_alerts().is_empty());
        assert_eq!(manager.history().len(), 1);
        assert_eq!(manager.history()[0].state, AlertState::Resolved);
        assert_eq!(
            manager.history()[0].gpu_uuid.as_deref(),
            Some("GPU-MOCK-0000")
        );
    }

    #[test]
    fn filter_changes_resolve_an_existing_alert() {
        let mut manager = AlertManager::default();
        let base_rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        manager.add_rule(base_rule.clone());
        let device = MockDevice::new(0);
        device.set_temperature(Temperature::new(85));
        manager.evaluate(&device, 0).unwrap();

        manager.add_rule(base_rule.with_gpu_filter(GpuFilter::Index(1)));
        assert!(manager.evaluate(&device, 0).unwrap().is_empty());

        assert!(manager.active_alerts().is_empty());
        assert_eq!(manager.history().len(), 1);
        assert_eq!(manager.history()[0].state, AlertState::Resolved);
    }
}
