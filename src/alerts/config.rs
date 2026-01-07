//! Alert configuration
//!
//! Provides TOML-based configuration for alert rules and settings.

use super::types::{AlertRule, AlertSeverity, Condition, GpuFilter, MetricType};
use crate::error::{ConfigError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Alert configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Global alert settings
    #[serde(default)]
    pub settings: AlertSettings,
    /// Alert rules
    #[serde(default)]
    pub rules: Vec<AlertRuleConfig>,
}

impl AlertConfig {
    /// Load configuration from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().display().to_string();
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|_| ConfigError::FileNotFound(path_str.clone()))?;

        Ok(toml::from_str(&contents).map_err(|e| ConfigError::ParseError(format!("{}", e)))?)
    }

    /// Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::ParseError(format!("Failed to serialize: {}", e)))?;

        fs::write(path.as_ref(), contents)?;

        Ok(())
    }

    /// Get default configuration path
    pub fn default_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("nvctl").join("alerts.toml")
        } else {
            PathBuf::from("alerts.toml")
        }
    }

    /// Create default configuration
    pub fn default_rules() -> Self {
        Self {
            settings: AlertSettings::default(),
            rules: vec![
                // High temperature warning
                AlertRuleConfig {
                    id: "high-temp".to_string(),
                    name: "High GPU Temperature".to_string(),
                    metric: "temperature".to_string(),
                    condition: ConditionConfig::GreaterThan(80.0),
                    severity: "warning".to_string(),
                    duration_secs: Some(30),
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
                // Critical temperature
                AlertRuleConfig {
                    id: "critical-temp".to_string(),
                    name: "Critical GPU Temperature".to_string(),
                    metric: "temperature".to_string(),
                    condition: ConditionConfig::GreaterThan(85.0),
                    severity: "critical".to_string(),
                    duration_secs: Some(10),
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
                // Emergency temperature
                AlertRuleConfig {
                    id: "emergency-temp".to_string(),
                    name: "Emergency Temperature - Shutdown Risk".to_string(),
                    metric: "temperature".to_string(),
                    condition: ConditionConfig::GreaterThan(90.0),
                    severity: "emergency".to_string(),
                    duration_secs: None, // Fire immediately
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
                // High power usage
                AlertRuleConfig {
                    id: "high-power".to_string(),
                    name: "High Power Usage".to_string(),
                    metric: "power_percent".to_string(),
                    condition: ConditionConfig::GreaterThan(95.0),
                    severity: "warning".to_string(),
                    duration_secs: Some(60),
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
                // ECC uncorrectable errors
                AlertRuleConfig {
                    id: "ecc-uncorrectable".to_string(),
                    name: "ECC Uncorrectable Errors Detected".to_string(),
                    metric: "ecc_uncorrectable_errors".to_string(),
                    condition: ConditionConfig::GreaterThan(0.0),
                    severity: "emergency".to_string(),
                    duration_secs: None,
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
                // PCIe replay counter (link errors)
                AlertRuleConfig {
                    id: "pcie-errors".to_string(),
                    name: "PCIe Link Errors Detected".to_string(),
                    metric: "pcie_replay_counter".to_string(),
                    condition: ConditionConfig::GreaterThan(0.0),
                    severity: "warning".to_string(),
                    duration_secs: Some(30),
                    gpu_filter: "all".to_string(),
                    enabled: true,
                },
            ],
        }
    }

    /// Convert to alert rules
    pub fn to_alert_rules(&self) -> Result<Vec<AlertRule>> {
        self.rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.to_alert_rule())
            .collect()
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self::default_rules()
    }
}

/// Global alert settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSettings {
    /// Whether alerting is enabled globally
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Check interval in seconds
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,
    /// Maximum alerts to keep in history
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 5,
            max_history: 1000,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_check_interval() -> u64 {
    5
}

fn default_max_history() -> usize {
    1000
}

/// Alert rule configuration (TOML-friendly format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRuleConfig {
    /// Rule identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Metric name (string form)
    pub metric: String,
    /// Condition
    pub condition: ConditionConfig,
    /// Severity level (string form)
    pub severity: String,
    /// Optional sustained duration in seconds
    pub duration_secs: Option<u64>,
    /// GPU filter (string form)
    #[serde(default = "default_gpu_filter")]
    pub gpu_filter: String,
    /// Whether rule is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_gpu_filter() -> String {
    "all".to_string()
}

impl AlertRuleConfig {
    /// Convert to AlertRule
    pub fn to_alert_rule(&self) -> Result<AlertRule> {
        let metric = self.parse_metric()?;
        let condition = self.condition.to_condition();
        let severity = self.parse_severity()?;
        let gpu_filter = self.parse_gpu_filter()?;

        let mut rule = AlertRule::new(
            self.id.clone(),
            self.name.clone(),
            metric,
            condition,
            severity,
        );

        if let Some(duration) = self.duration_secs {
            rule = rule.with_duration(Duration::from_secs(duration));
        }

        rule = rule.with_gpu_filter(gpu_filter);

        if !self.enabled {
            rule = rule.disabled();
        }

        Ok(rule)
    }

    fn parse_metric(&self) -> Result<MetricType> {
        match self.metric.as_str() {
            "temperature" => Ok(MetricType::Temperature),
            "memory_temperature" => Ok(MetricType::MemoryTemperature),
            "power_usage" => Ok(MetricType::PowerUsage),
            "power_percent" => Ok(MetricType::PowerPercent),
            "gpu_utilization" => Ok(MetricType::GpuUtilization),
            "memory_utilization" => Ok(MetricType::MemoryUtilization),
            "fan_speed" => Ok(MetricType::FanSpeed),
            "clock_speed" => Ok(MetricType::ClockSpeed),
            "ecc_correctable_errors" => Ok(MetricType::EccCorrectableErrors),
            "ecc_uncorrectable_errors" => Ok(MetricType::EccUncorrectableErrors),
            "pcie_throughput" => Ok(MetricType::PcieThroughput),
            "pcie_replay_counter" => Ok(MetricType::PcieReplayCounter),
            _ => Err(ConfigError::InvalidValue {
                key: "metric".to_string(),
                message: format!("Unknown metric type: {}", self.metric),
            })?,
        }
    }

    fn parse_severity(&self) -> Result<AlertSeverity> {
        match self.severity.to_lowercase().as_str() {
            "info" => Ok(AlertSeverity::Info),
            "warning" => Ok(AlertSeverity::Warning),
            "critical" => Ok(AlertSeverity::Critical),
            "emergency" => Ok(AlertSeverity::Emergency),
            _ => Err(ConfigError::InvalidValue {
                key: "severity".to_string(),
                message: format!("Unknown severity level: {}", self.severity),
            })?,
        }
    }

    fn parse_gpu_filter(&self) -> Result<GpuFilter> {
        match self.gpu_filter.as_str() {
            "all" => Ok(GpuFilter::All),
            s if s.starts_with("index:") => {
                let idx: u32 = s[6..].parse().map_err(|_| ConfigError::InvalidValue {
                    key: "gpu_filter".to_string(),
                    message: format!("Invalid GPU index: {}", s),
                })?;
                Ok(GpuFilter::Index(idx))
            }
            s if s.starts_with("uuid:") => Ok(GpuFilter::Uuid(s[5..].to_string())),
            _ => Err(ConfigError::InvalidValue {
                key: "gpu_filter".to_string(),
                message: format!("Unknown GPU filter: {}", self.gpu_filter),
            })?,
        }
    }
}

/// Condition configuration (TOML-friendly format)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionConfig {
    GreaterThan(f64),
    LessThan(f64),
    Equals(f64),
    InRange { min: f64, max: f64 },
    OutsideRange { min: f64, max: f64 },
}

impl ConditionConfig {
    fn to_condition(&self) -> Condition {
        match self {
            Self::GreaterThan(v) => Condition::GreaterThan(*v),
            Self::LessThan(v) => Condition::LessThan(*v),
            Self::Equals(v) => Condition::Equals(*v),
            Self::InRange { min, max } => Condition::InRange(*min, *max),
            Self::OutsideRange { min, max } => Condition::OutsideRange(*min, *max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AlertConfig::default();
        assert!(config.settings.enabled);
        assert_eq!(config.settings.check_interval_secs, 5);
        assert!(!config.rules.is_empty());
    }

    #[test]
    fn test_parse_metric() {
        let rule = AlertRuleConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            metric: "temperature".to_string(),
            condition: ConditionConfig::GreaterThan(80.0),
            severity: "warning".to_string(),
            duration_secs: None,
            gpu_filter: "all".to_string(),
            enabled: true,
        };

        assert!(matches!(
            rule.parse_metric().unwrap(),
            MetricType::Temperature
        ));
    }

    #[test]
    fn test_parse_severity() {
        let rule = AlertRuleConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            metric: "temperature".to_string(),
            condition: ConditionConfig::GreaterThan(80.0),
            severity: "warning".to_string(),
            duration_secs: None,
            gpu_filter: "all".to_string(),
            enabled: true,
        };

        assert!(matches!(
            rule.parse_severity().unwrap(),
            AlertSeverity::Warning
        ));
    }

    #[test]
    fn test_parse_gpu_filter() {
        let mut rule = AlertRuleConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            metric: "temperature".to_string(),
            condition: ConditionConfig::GreaterThan(80.0),
            severity: "warning".to_string(),
            duration_secs: None,
            gpu_filter: "all".to_string(),
            enabled: true,
        };

        assert!(matches!(rule.parse_gpu_filter().unwrap(), GpuFilter::All));

        rule.gpu_filter = "index:1".to_string();
        assert!(matches!(
            rule.parse_gpu_filter().unwrap(),
            GpuFilter::Index(1)
        ));
    }

    #[test]
    fn test_to_alert_rule() {
        let rule_config = AlertRuleConfig {
            id: "high-temp".to_string(),
            name: "High Temperature".to_string(),
            metric: "temperature".to_string(),
            condition: ConditionConfig::GreaterThan(80.0),
            severity: "warning".to_string(),
            duration_secs: Some(30),
            gpu_filter: "all".to_string(),
            enabled: true,
        };

        let rule = rule_config.to_alert_rule().unwrap();
        assert_eq!(rule.id, "high-temp");
        assert_eq!(rule.name, "High Temperature");
        assert!(matches!(rule.metric, MetricType::Temperature));
        assert!(matches!(rule.severity, AlertSeverity::Warning));
        assert!(rule.duration.is_some());
        assert_eq!(rule.duration.unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_config_to_alert_rules() {
        let config = AlertConfig::default();
        let rules = config.to_alert_rules().unwrap();
        assert!(!rules.is_empty());
        assert!(rules.iter().all(|r| r.enabled));
    }

    #[test]
    fn test_condition_config_conversion() {
        let config = ConditionConfig::GreaterThan(80.0);
        let condition = config.to_condition();
        assert!(condition.evaluate(85.0));
        assert!(!condition.evaluate(75.0));
    }
}
