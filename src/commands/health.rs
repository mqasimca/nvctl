//! Health command implementation
//!
//! Shows GPU health scores and recommendations.

use crate::cli::args::OutputFormat;
use crate::cli::output::{HealthStatus, OutputWriter};
use crate::commands::selection::{select_gpu_indices, GpuSelector};
use crate::domain::{EccMode, ThrottleReasons};
use crate::error::{NvmlError, Result};
use crate::health::{system_uptime_seconds, HealthCalculator, HealthParams};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the health command
pub fn run_health(format: OutputFormat, selector: &GpuSelector) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Determine which GPUs to check
    let indices = select_gpu_indices(&manager, selector)?;

    let calculator = HealthCalculator::default();
    let mut output = OutputWriter::new(format);

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        // Gather metrics for health calculation
        let temperature = device.temperature()?;
        let thermal_thresholds = device.thermal_thresholds()?;
        let power_usage = device.power_usage()?;
        let power_limit = device.power_limit()?;
        let memory_info = optional_metric(device.memory_info())?;
        let utilization = optional_metric(device.utilization())?;
        let performance_state = optional_metric(device.performance_state())?;
        let ecc_mode = device.ecc_mode()?;
        let ecc_errors = if matches!(ecc_mode, Some(EccMode::Enabled)) {
            device.ecc_errors()?
        } else {
            None
        };
        let pcie_metrics = optional_metric(device.pcie_metrics())?;
        let throttle_reasons = optional_metric(device.throttle_reasons())?;

        let is_thermal_throttling = throttle_reasons
            .as_ref()
            .is_some_and(|reasons| reasons.sw_thermal || reasons.hw_thermal);
        let is_power_throttling = throttle_reasons
            .as_ref()
            .is_some_and(|reasons| reasons.sw_power_cap || reasons.hw_power_brake);

        // Calculate VRAM usage ratio
        let vram_usage_ratio = memory_info
            .filter(|info| info.total > 0)
            .map(|info| (info.used as f64 / info.total as f64).clamp(0.0, 1.0));

        let uptime_seconds = system_uptime_seconds().unwrap_or(0);

        // Build health params
        let params = HealthParams {
            temperature,
            thresholds: &thermal_thresholds,
            power_usage,
            power_limit,
            is_thermal_throttling,
            is_power_throttling,
            ecc_errors: ecc_errors.as_ref(),
            vram_usage_ratio,
            utilization: utilization.as_ref(),
            pcie_metrics: pcie_metrics.as_ref(),
            uptime_seconds,
        };

        // Calculate health
        let health = calculator.calculate(&params);

        // Create output status
        let health_status = HealthStatus {
            gpu_name: info.name.clone(),
            gpu_index: idx,
            overall_score: health.overall.score(),
            thermal_score: health.thermal.score(),
            power_score: health.power.score(),
            memory_score: health.memory.score(),
            performance_score: health.performance.score(),
            pcie_score: health.pcie.score(),
            status: health.overall.status().to_string(),
            issues: health
                .issues
                .iter()
                .map(|issue| format!("{} - {}", issue.severity, issue.description))
                .collect(),
            recommendations: health.recommendations,
            throttle_reasons: throttle_description(throttle_reasons.as_ref(), performance_state),
        };

        output.write(&health_status)?;
    }

    output.finish()?;
    Ok(())
}

fn optional_metric<T>(metric: std::result::Result<T, NvmlError>) -> Result<Option<T>> {
    match metric {
        Ok(value) => Ok(Some(value)),
        Err(NvmlError::NotSupported(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn throttle_description(
    throttle_reasons: Option<&ThrottleReasons>,
    performance_state: Option<crate::domain::PerformanceState>,
) -> Option<String> {
    let throttle_reasons = throttle_reasons?;
    let reasons = throttle_reasons.active_reasons();
    if reasons.is_empty() {
        None
    } else {
        Some(match performance_state {
            Some(performance_state) => format!(
                "{} (performance state {})",
                reasons.join(", "),
                performance_state
            ),
            None => reasons.join(", "),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PerformanceState;

    #[test]
    fn throttle_description_handles_optional_metrics() {
        let reasons = ThrottleReasons {
            sw_power_cap: true,
            ..ThrottleReasons::default()
        };
        assert_eq!(
            throttle_description(Some(&reasons), Some(PerformanceState::P2)).as_deref(),
            Some("Power Cap (performance state P2)")
        );
        assert_eq!(
            throttle_description(Some(&reasons), None).as_deref(),
            Some("Power Cap")
        );
        assert_eq!(throttle_description(None, None), None);
    }
}
