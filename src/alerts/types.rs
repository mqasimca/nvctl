//! Alert system domain types
//!
//! Defines validated types for the alerting system including rules, conditions, and alert states.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Informational, no action needed
    Info,
    /// Attention recommended
    Warning,
    /// Action required soon
    Critical,
    /// Immediate action required
    Emergency,
}

impl fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
            Self::Emergency => write!(f, "EMERGENCY"),
        }
    }
}

/// Alert condition type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Value greater than threshold
    GreaterThan(f64),
    /// Value less than threshold
    LessThan(f64),
    /// Value equals threshold (with epsilon for floats)
    Equals(f64),
    /// Value within range (inclusive)
    InRange(f64, f64),
    /// Value outside range
    OutsideRange(f64, f64),
}

impl Condition {
    /// Evaluate condition against a value
    pub fn evaluate(&self, value: f64) -> bool {
        const EPSILON: f64 = 1e-6;

        match self {
            Self::GreaterThan(threshold) => value > *threshold,
            Self::LessThan(threshold) => value < *threshold,
            Self::Equals(target) => (value - target).abs() < EPSILON,
            Self::InRange(min, max) => value >= *min && value <= *max,
            Self::OutsideRange(min, max) => value < *min || value > *max,
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GreaterThan(v) => write!(f, "> {}", v),
            Self::LessThan(v) => write!(f, "< {}", v),
            Self::Equals(v) => write!(f, "= {}", v),
            Self::InRange(min, max) => write!(f, "in [{}, {}]", min, max),
            Self::OutsideRange(min, max) => write!(f, "outside [{}, {}]", min, max),
        }
    }
}

/// Metric type for alert rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// GPU core temperature
    Temperature,
    /// Memory temperature
    MemoryTemperature,
    /// Power usage (watts)
    PowerUsage,
    /// Power usage as percentage of limit
    PowerPercent,
    /// GPU utilization percentage
    GpuUtilization,
    /// Memory utilization percentage
    MemoryUtilization,
    /// Fan speed percentage
    FanSpeed,
    /// Clock speed
    ClockSpeed,
    /// ECC correctable errors
    EccCorrectableErrors,
    /// ECC uncorrectable errors
    EccUncorrectableErrors,
    /// PCIe throughput
    PcieThroughput,
    /// PCIe replay counter
    PcieReplayCounter,
}

impl fmt::Display for MetricType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temperature => write!(f, "temperature"),
            Self::MemoryTemperature => write!(f, "memory_temperature"),
            Self::PowerUsage => write!(f, "power_usage"),
            Self::PowerPercent => write!(f, "power_percent"),
            Self::GpuUtilization => write!(f, "gpu_utilization"),
            Self::MemoryUtilization => write!(f, "memory_utilization"),
            Self::FanSpeed => write!(f, "fan_speed"),
            Self::ClockSpeed => write!(f, "clock_speed"),
            Self::EccCorrectableErrors => write!(f, "ecc_correctable_errors"),
            Self::EccUncorrectableErrors => write!(f, "ecc_uncorrectable_errors"),
            Self::PcieThroughput => write!(f, "pcie_throughput"),
            Self::PcieReplayCounter => write!(f, "pcie_replay_counter"),
        }
    }
}

/// GPU filter for alert rules
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GpuFilter {
    /// Apply to all GPUs
    #[default]
    All,
    /// Apply to specific GPU by index
    Index(u32),
    /// Apply to GPUs by indices
    Indices(Vec<u32>),
    /// Apply to GPU by UUID
    Uuid(String),
}

impl GpuFilter {
    /// Check if this filter matches a GPU index
    pub fn matches(&self, gpu_index: u32) -> bool {
        match self {
            Self::All => true,
            Self::Index(idx) => *idx == gpu_index,
            Self::Indices(indices) => indices.contains(&gpu_index),
            Self::Uuid(_) => false, // UUID matching requires GPU info
        }
    }
}

/// Alert rule definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    /// Unique rule identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Metric to monitor
    pub metric: MetricType,
    /// Condition to evaluate
    pub condition: Condition,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Optional sustained duration before firing
    pub duration: Option<Duration>,
    /// GPU filter
    pub gpu_filter: GpuFilter,
    /// Whether rule is enabled
    pub enabled: bool,
}

impl AlertRule {
    /// Create a new alert rule
    pub fn new(
        id: String,
        name: String,
        metric: MetricType,
        condition: Condition,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            id,
            name,
            metric,
            condition,
            severity,
            duration: None,
            gpu_filter: GpuFilter::All,
            enabled: true,
        }
    }

    /// Set sustained duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set GPU filter
    pub fn with_gpu_filter(mut self, filter: GpuFilter) -> Self {
        self.gpu_filter = filter;
        self
    }

    /// Disable the rule
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Evaluate the rule condition against a value
    pub fn evaluate(&self, value: f64) -> bool {
        if !self.enabled {
            return false;
        }
        self.condition.evaluate(value)
    }
}

/// Alert state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertState {
    /// Condition met but not yet fired (waiting for duration)
    Pending,
    /// Alert active
    Firing,
    /// User acknowledged but not resolved
    Acknowledged,
    /// Condition no longer met
    Resolved,
    /// Manually silenced
    Silenced,
}

impl fmt::Display for AlertState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "PENDING"),
            Self::Firing => write!(f, "FIRING"),
            Self::Acknowledged => write!(f, "ACKNOWLEDGED"),
            Self::Resolved => write!(f, "RESOLVED"),
            Self::Silenced => write!(f, "SILENCED"),
        }
    }
}

/// Active alert instance
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert ID
    pub id: String,
    /// Rule ID that triggered this alert
    pub rule_id: String,
    /// GPU index
    pub gpu_index: u32,
    /// Timestamp when condition first met
    pub started_at: std::time::SystemTime,
    /// Timestamp when alert fired (if fired)
    pub fired_at: Option<std::time::SystemTime>,
    /// Timestamp when resolved (if resolved)
    pub resolved_at: Option<std::time::SystemTime>,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Metric type
    pub metric: MetricType,
    /// Current metric value
    pub current_value: f64,
    /// Threshold value
    pub threshold_value: f64,
    /// Alert message
    pub message: String,
    /// Current state
    pub state: AlertState,
}

impl Alert {
    /// Create a new pending alert
    pub fn new_pending(
        rule: &AlertRule,
        gpu_index: u32,
        current_value: f64,
        threshold_value: f64,
    ) -> Self {
        let id = format!(
            "{}-{}-{}",
            rule.id,
            gpu_index,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        let message = format!(
            "{}: {} {} (current: {:.2})",
            rule.name, rule.metric, rule.condition, current_value
        );

        Self {
            id,
            rule_id: rule.id.clone(),
            gpu_index,
            started_at: std::time::SystemTime::now(),
            fired_at: None,
            resolved_at: None,
            severity: rule.severity,
            metric: rule.metric,
            current_value,
            threshold_value,
            message,
            state: AlertState::Pending,
        }
    }

    /// Mark alert as firing
    pub fn fire(&mut self) {
        self.state = AlertState::Firing;
        self.fired_at = Some(std::time::SystemTime::now());
    }

    /// Mark alert as resolved
    pub fn resolve(&mut self) {
        self.state = AlertState::Resolved;
        self.resolved_at = Some(std::time::SystemTime::now());
    }

    /// Mark alert as acknowledged
    pub fn acknowledge(&mut self) {
        self.state = AlertState::Acknowledged;
    }

    /// Mark alert as silenced
    pub fn silence(&mut self) {
        self.state = AlertState::Silenced;
    }

    /// Update current value
    pub fn update_value(&mut self, value: f64) {
        self.current_value = value;
    }

    /// Check if alert should fire based on duration
    pub fn should_fire(&self, rule: &AlertRule) -> bool {
        if self.state != AlertState::Pending {
            return false;
        }

        if let Some(duration) = rule.duration {
            if let Ok(elapsed) = self.started_at.elapsed() {
                return elapsed >= duration;
            }
        } else {
            // No duration requirement, fire immediately
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_greater_than() {
        let cond = Condition::GreaterThan(80.0);
        assert!(cond.evaluate(85.0));
        assert!(!cond.evaluate(75.0));
        assert!(!cond.evaluate(80.0));
    }

    #[test]
    fn test_condition_less_than() {
        let cond = Condition::LessThan(50.0);
        assert!(cond.evaluate(45.0));
        assert!(!cond.evaluate(55.0));
        assert!(!cond.evaluate(50.0));
    }

    #[test]
    fn test_condition_in_range() {
        let cond = Condition::InRange(40.0, 80.0);
        assert!(cond.evaluate(60.0));
        assert!(cond.evaluate(40.0));
        assert!(cond.evaluate(80.0));
        assert!(!cond.evaluate(30.0));
        assert!(!cond.evaluate(90.0));
    }

    #[test]
    fn test_gpu_filter_all() {
        let filter = GpuFilter::All;
        assert!(filter.matches(0));
        assert!(filter.matches(5));
    }

    #[test]
    fn test_gpu_filter_index() {
        let filter = GpuFilter::Index(2);
        assert!(!filter.matches(0));
        assert!(filter.matches(2));
        assert!(!filter.matches(3));
    }

    #[test]
    fn test_alert_rule_creation() {
        let rule = AlertRule::new(
            "test-rule".to_string(),
            "Test Rule".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        assert_eq!(rule.id, "test-rule");
        assert_eq!(rule.metric, MetricType::Temperature);
        assert!(rule.enabled);
        assert!(rule.evaluate(85.0));
        assert!(!rule.evaluate(75.0));
    }

    #[test]
    fn test_alert_severity_ordering() {
        assert!(AlertSeverity::Info < AlertSeverity::Warning);
        assert!(AlertSeverity::Warning < AlertSeverity::Critical);
        assert!(AlertSeverity::Critical < AlertSeverity::Emergency);
    }

    #[test]
    fn test_alert_creation_and_firing() {
        let rule = AlertRule::new(
            "high-temp".to_string(),
            "High Temperature".to_string(),
            MetricType::Temperature,
            Condition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );

        let mut alert = Alert::new_pending(&rule, 0, 85.0, 80.0);
        assert_eq!(alert.state, AlertState::Pending);
        assert_eq!(alert.gpu_index, 0);
        assert_eq!(alert.current_value, 85.0);

        alert.fire();
        assert_eq!(alert.state, AlertState::Firing);
        assert!(alert.fired_at.is_some());

        alert.resolve();
        assert_eq!(alert.state, AlertState::Resolved);
        assert!(alert.resolved_at.is_some());
    }
}
