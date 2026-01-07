//! GPU health scoring system
//!
//! Provides a 0-100 health score based on multiple factors:
//! - Thermal health (temperature, throttling)
//! - Power health (power usage, throttling)
//! - Memory health (ECC errors, VRAM usage)
//! - Performance health (utilization, clock speeds)
//! - PCIe health (link status, errors)

use crate::domain::{
    memory::EccErrors, pcie::PcieMetrics, performance::Utilization, thermal::Temperature,
    PowerLimit, ThermalThresholds,
};
use serde::{Deserialize, Serialize};

/// GPU health score (0-100)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HealthScore(u8);

impl HealthScore {
    /// Create a new health score (0-100)
    pub fn new(score: u8) -> Self {
        Self(score.min(100))
    }

    /// Get the score value
    pub fn score(&self) -> u8 {
        self.0
    }

    /// Get health status category
    pub fn status(&self) -> HealthStatus {
        match self.0 {
            90..=100 => HealthStatus::Excellent,
            75..=89 => HealthStatus::Good,
            50..=74 => HealthStatus::Fair,
            25..=49 => HealthStatus::Poor,
            _ => HealthStatus::Critical,
        }
    }

    /// Get color for display
    pub fn color_code(&self) -> &'static str {
        match self.status() {
            HealthStatus::Excellent => "green",
            HealthStatus::Good => "cyan",
            HealthStatus::Fair => "yellow",
            HealthStatus::Poor => "orange",
            HealthStatus::Critical => "red",
        }
    }
}

impl std::fmt::Display for HealthScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/100", self.0)
    }
}

/// Health status category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Excellent, // 90-100
    Good,      // 75-89
    Fair,      // 50-74
    Poor,      // 25-49
    Critical,  // 0-24
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Excellent => write!(f, "Excellent"),
            Self::Good => write!(f, "Good"),
            Self::Fair => write!(f, "Fair"),
            Self::Poor => write!(f, "Poor"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

/// Detailed health breakdown by factor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthBreakdown {
    /// Overall health score
    pub overall: HealthScore,
    /// Thermal health score
    pub thermal: HealthScore,
    /// Power health score
    pub power: HealthScore,
    /// Memory health score
    pub memory: HealthScore,
    /// Performance health score
    pub performance: HealthScore,
    /// PCIe health score
    pub pcie: HealthScore,
    /// Issues detected
    pub issues: Vec<HealthIssue>,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Health issue with severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthIssue {
    pub severity: IssueSeverity,
    pub category: String,
    pub description: String,
}

/// Issue severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Health calculator
pub struct HealthCalculator {
    /// Weights for each factor (must sum to 1.0)
    thermal_weight: f64,
    power_weight: f64,
    memory_weight: f64,
    performance_weight: f64,
    pcie_weight: f64,
}

impl Default for HealthCalculator {
    fn default() -> Self {
        Self {
            thermal_weight: 0.30,     // 30% - thermal is critical
            power_weight: 0.20,       // 20% - power management important
            memory_weight: 0.25,      // 25% - memory errors critical
            performance_weight: 0.15, // 15% - performance secondary
            pcie_weight: 0.10,        // 10% - PCIe usually stable
        }
    }
}

impl HealthCalculator {
    /// Create a new health calculator with custom weights
    pub fn new(
        thermal_weight: f64,
        power_weight: f64,
        memory_weight: f64,
        performance_weight: f64,
        pcie_weight: f64,
    ) -> Self {
        let total =
            thermal_weight + power_weight + memory_weight + performance_weight + pcie_weight;
        assert!(
            (total - 1.0).abs() < 0.001,
            "Weights must sum to 1.0, got {}",
            total
        );

        Self {
            thermal_weight,
            power_weight,
            memory_weight,
            performance_weight,
            pcie_weight,
        }
    }

    /// Calculate overall health score
    pub fn calculate(&self, params: &HealthParams) -> HealthBreakdown {
        let thermal_score = self.calculate_thermal_health(params);
        let power_score = self.calculate_power_health(params);
        let memory_score = self.calculate_memory_health(params);
        let performance_score = self.calculate_performance_health(params);
        let pcie_score = self.calculate_pcie_health(params);

        // Weighted average
        let overall_value = (thermal_score.score() as f64 * self.thermal_weight
            + power_score.score() as f64 * self.power_weight
            + memory_score.score() as f64 * self.memory_weight
            + performance_score.score() as f64 * self.performance_weight
            + pcie_score.score() as f64 * self.pcie_weight)
            .round() as u8;

        let overall = HealthScore::new(overall_value);

        // Collect issues and recommendations
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        self.analyze_thermal(params, &mut issues, &mut recommendations);
        self.analyze_power(params, &mut issues, &mut recommendations);
        self.analyze_memory(params, &mut issues, &mut recommendations);
        self.analyze_performance(params, &mut issues, &mut recommendations);
        self.analyze_pcie(params, &mut issues, &mut recommendations);

        HealthBreakdown {
            overall,
            thermal: thermal_score,
            power: power_score,
            memory: memory_score,
            performance: performance_score,
            pcie: pcie_score,
            issues,
            recommendations,
        }
    }

    /// Calculate thermal health score (0-100)
    fn calculate_thermal_health(&self, params: &HealthParams) -> HealthScore {
        let temp_celsius = params.temperature.as_celsius();
        let mut score = 100;

        // Temperature penalty
        if let Some(slowdown) = params.thresholds.slowdown {
            let slowdown_temp = slowdown.as_celsius();
            if temp_celsius >= slowdown_temp {
                score = 0; // At or above thermal throttle = critical
            } else if temp_celsius >= slowdown_temp - 10 {
                // Within 10°C of throttle
                let ratio = (slowdown_temp - temp_celsius) as f64 / 10.0;
                score = (ratio * 50.0) as u8; // 0-50 score
            } else if temp_celsius >= 80 {
                // 80-85°C range
                score = 75;
            } else if temp_celsius >= 70 {
                score = 90;
            }
        } else {
            // No slowdown threshold, use generic limits
            if temp_celsius >= 90 {
                score = 10;
            } else if temp_celsius >= 85 {
                score = 40;
            } else if temp_celsius >= 80 {
                score = 70;
            } else if temp_celsius >= 70 {
                score = 90;
            }
        }

        // Throttling penalty
        if params.is_thermal_throttling {
            score = score.saturating_sub(30); // -30 for active throttling
        }
        if params.is_power_throttling {
            score = score.saturating_sub(10); // -10 for power throttling
        }

        HealthScore::new(score)
    }

    /// Calculate power health score (0-100)
    fn calculate_power_health(&self, params: &HealthParams) -> HealthScore {
        let power_ratio =
            params.power_usage.as_watts() as f64 / params.power_limit.as_watts().max(1) as f64;
        let mut score: u8 = 100;

        // Power usage penalty
        if power_ratio >= 0.98 {
            score = 50; // Near limit
        } else if power_ratio >= 0.90 {
            score = 75; // High usage
        } else if power_ratio >= 0.80 {
            score = 90;
        }

        // Throttling penalty
        if params.is_power_throttling {
            score = score.saturating_sub(40); // Heavy penalty for power throttling
        }

        HealthScore::new(score)
    }

    /// Calculate memory health score (0-100)
    fn calculate_memory_health(&self, params: &HealthParams) -> HealthScore {
        let mut score: u8 = 100;

        // ECC errors penalty
        if let Some(ecc) = &params.ecc_errors {
            if ecc.has_uncorrectable() {
                score = 0; // Uncorrectable errors = critical failure
            } else if ecc.correctable_exceeds_threshold(params.uptime_seconds) {
                score = 40; // High correctable error rate
            } else if ecc.correctable_current > 0 {
                score = 85; // Some errors but not critical
            }
        }

        // VRAM usage penalty (if available)
        if let Some(vram_ratio) = params.vram_usage_ratio {
            if vram_ratio >= 0.95 {
                score = score.saturating_sub(20); // Very high VRAM usage
            } else if vram_ratio >= 0.85 {
                score = score.saturating_sub(10);
            }
        }

        HealthScore::new(score)
    }

    /// Calculate performance health score (0-100)
    fn calculate_performance_health(&self, params: &HealthParams) -> HealthScore {
        let mut score = 100;

        // This is informational, not critical
        // Low utilization doesn't mean unhealthy, just idle
        // Only penalize if there are signs of performance issues

        if params.is_thermal_throttling || params.is_power_throttling {
            score = 70; // Performance degraded due to throttling
        }

        // Clock throttling (if clocks are unusually low while under load)
        if let Some(util) = &params.utilization {
            if util.gpu_percent() > 80 {
                // Under heavy load - this is fine
                score = 100;
            }
        }

        HealthScore::new(score)
    }

    /// Calculate PCIe health score (0-100)
    fn calculate_pcie_health(&self, params: &HealthParams) -> HealthScore {
        let mut score: u8 = 100;

        if let Some(pcie) = &params.pcie_metrics {
            // Link efficiency penalty
            let efficiency = pcie.link_status.bandwidth_efficiency_percent();
            if efficiency < 50.0 {
                score = 60; // Link not using full capability
            } else if efficiency < 75.0 {
                score = 85;
            }

            // PCIe replay errors penalty
            if pcie.replay_counter.count() > 1000 {
                score = score.saturating_sub(30); // Many link errors
            } else if pcie.replay_counter.count() > 100 {
                score = score.saturating_sub(15);
            } else if pcie.replay_counter.count() > 0 {
                score = score.saturating_sub(5);
            }
        }

        HealthScore::new(score)
    }

    // Analysis functions for issues and recommendations

    fn analyze_thermal(
        &self,
        params: &HealthParams,
        issues: &mut Vec<HealthIssue>,
        recommendations: &mut Vec<String>,
    ) {
        let temp = params.temperature.as_celsius();

        if params.is_thermal_throttling {
            issues.push(HealthIssue {
                severity: IssueSeverity::Critical,
                category: "Thermal".to_string(),
                description: format!("GPU is thermal throttling at {}°C", temp),
            });
            recommendations.push(
                "Improve cooling: clean dust filters, increase fan speed, or improve case airflow"
                    .to_string(),
            );
        } else if temp >= 85 {
            issues.push(HealthIssue {
                severity: IssueSeverity::Warning,
                category: "Thermal".to_string(),
                description: format!("High temperature: {}°C", temp),
            });
            recommendations.push("Consider increasing fan speed or improving cooling".to_string());
        }
    }

    fn analyze_power(
        &self,
        params: &HealthParams,
        issues: &mut Vec<HealthIssue>,
        recommendations: &mut Vec<String>,
    ) {
        let power_ratio =
            params.power_usage.as_watts() as f64 / params.power_limit.as_watts().max(1) as f64;

        if params.is_power_throttling {
            issues.push(HealthIssue {
                severity: IssueSeverity::Critical,
                category: "Power".to_string(),
                description: "GPU is power throttling".to_string(),
            });
            recommendations.push("Increase power limit or reduce workload intensity".to_string());
        } else if power_ratio >= 0.95 {
            issues.push(HealthIssue {
                severity: IssueSeverity::Warning,
                category: "Power".to_string(),
                description: format!("Power usage near limit: {:.0}%", power_ratio * 100.0),
            });
            recommendations
                .push("Consider increasing power limit if thermal headroom allows".to_string());
        }
    }

    fn analyze_memory(
        &self,
        params: &HealthParams,
        issues: &mut Vec<HealthIssue>,
        recommendations: &mut Vec<String>,
    ) {
        if let Some(ecc) = &params.ecc_errors {
            if ecc.has_uncorrectable() {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Critical,
                    category: "Memory".to_string(),
                    description: format!(
                        "Uncorrectable ECC errors detected: {}",
                        ecc.uncorrectable_current
                    ),
                });
                recommendations.push("CRITICAL: Uncorrectable memory errors indicate hardware failure. Consider RMA or replacement".to_string());
            } else if ecc.correctable_exceeds_threshold(params.uptime_seconds) {
                let rate = ecc.correctable_rate_per_hour(params.uptime_seconds);
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "Memory".to_string(),
                    description: format!("High correctable ECC error rate: {:.1}/hour", rate),
                });
                recommendations.push(
                    "Monitor ECC errors; sustained high rates may indicate degrading memory"
                        .to_string(),
                );
            }
        }

        if let Some(vram_ratio) = params.vram_usage_ratio {
            if vram_ratio >= 0.95 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "Memory".to_string(),
                    description: format!("VRAM usage very high: {:.0}%", vram_ratio * 100.0),
                });
                recommendations
                    .push("Reduce VRAM usage or close unnecessary applications".to_string());
            }
        }
    }

    fn analyze_performance(
        &self,
        params: &HealthParams,
        issues: &mut Vec<HealthIssue>,
        _recommendations: &mut Vec<String>,
    ) {
        if params.is_thermal_throttling || params.is_power_throttling {
            issues.push(HealthIssue {
                severity: IssueSeverity::Info,
                category: "Performance".to_string(),
                description: "Performance reduced due to throttling".to_string(),
            });
        }
    }

    fn analyze_pcie(
        &self,
        params: &HealthParams,
        issues: &mut Vec<HealthIssue>,
        recommendations: &mut Vec<String>,
    ) {
        if let Some(pcie) = &params.pcie_metrics {
            let efficiency = pcie.link_status.bandwidth_efficiency_percent();

            if efficiency < 50.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "PCIe".to_string(),
                    description: format!(
                        "PCIe link running at reduced capability: {} (max: {})",
                        pcie.link_status.current_generation, pcie.link_status.max_generation
                    ),
                });
                recommendations.push(
                    "Check PCIe slot configuration and ensure GPU is in appropriate slot"
                        .to_string(),
                );
            }

            if pcie.replay_counter.count() > 100 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "PCIe".to_string(),
                    description: format!(
                        "PCIe link errors detected: {} replays",
                        pcie.replay_counter.count()
                    ),
                });
                recommendations.push(
                    "PCIe link instability detected; check PCIe power cables and slot connection"
                        .to_string(),
                );
            }
        }
    }
}

/// Parameters for health calculation
pub struct HealthParams<'a> {
    pub temperature: Temperature,
    pub thresholds: &'a ThermalThresholds,
    pub power_usage: PowerLimit,
    pub power_limit: PowerLimit,
    pub is_thermal_throttling: bool,
    pub is_power_throttling: bool,
    pub ecc_errors: Option<&'a EccErrors>,
    pub vram_usage_ratio: Option<f64>,
    pub utilization: Option<&'a Utilization>,
    pub pcie_metrics: Option<&'a PcieMetrics>,
    pub uptime_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{memory::EccErrors, PowerLimit, Temperature, ThermalThresholds};

    #[test]
    fn test_health_score_status() {
        assert_eq!(HealthScore::new(95).status(), HealthStatus::Excellent);
        assert_eq!(HealthScore::new(80).status(), HealthStatus::Good);
        assert_eq!(HealthScore::new(60).status(), HealthStatus::Fair);
        assert_eq!(HealthScore::new(30).status(), HealthStatus::Poor);
        assert_eq!(HealthScore::new(10).status(), HealthStatus::Critical);
    }

    #[test]
    fn test_healthy_gpu() {
        let calculator = HealthCalculator::default();
        let thresholds = ThermalThresholds::default();

        let params = HealthParams {
            temperature: Temperature::new(65),
            thresholds: &thresholds,
            power_usage: PowerLimit::from_watts(150),
            power_limit: PowerLimit::from_watts(250),
            is_thermal_throttling: false,
            is_power_throttling: false,
            ecc_errors: None,
            vram_usage_ratio: Some(0.5),
            utilization: None,
            pcie_metrics: None,
            uptime_seconds: 3600,
        };

        let breakdown = calculator.calculate(&params);
        assert!(breakdown.overall.score() >= 90);
        assert_eq!(breakdown.overall.status(), HealthStatus::Excellent);
    }

    #[test]
    fn test_throttling_gpu() {
        let calculator = HealthCalculator::default();
        let thresholds = ThermalThresholds::default();

        let params = HealthParams {
            temperature: Temperature::new(88),
            thresholds: &thresholds,
            power_usage: PowerLimit::from_watts(240),
            power_limit: PowerLimit::from_watts(250),
            is_thermal_throttling: true,
            is_power_throttling: false,
            ecc_errors: None,
            vram_usage_ratio: Some(0.7),
            utilization: None,
            pcie_metrics: None,
            uptime_seconds: 3600,
        };

        let breakdown = calculator.calculate(&params);
        assert!(breakdown.overall.score() < 70);
        assert!(!breakdown.issues.is_empty());
    }

    #[test]
    fn test_ecc_errors_critical() {
        let calculator = HealthCalculator::default();
        let thresholds = ThermalThresholds::default();
        let ecc = EccErrors::new(0, 0, 1, 1); // Uncorrectable error

        let params = HealthParams {
            temperature: Temperature::new(65),
            thresholds: &thresholds,
            power_usage: PowerLimit::from_watts(150),
            power_limit: PowerLimit::from_watts(250),
            is_thermal_throttling: false,
            is_power_throttling: false,
            ecc_errors: Some(&ecc),
            vram_usage_ratio: Some(0.5),
            utilization: None,
            pcie_metrics: None,
            uptime_seconds: 3600,
        };

        let breakdown = calculator.calculate(&params);
        assert_eq!(breakdown.memory.score(), 0); // Critical memory failure
        assert!(breakdown
            .issues
            .iter()
            .any(|i| matches!(i.severity, IssueSeverity::Critical)));
    }
}
