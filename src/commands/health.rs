//! Health command implementation
//!
//! Shows GPU health scores and recommendations.

use crate::cli::args::OutputFormat;
use crate::cli::output::{print_output, HealthStatus};
use crate::domain::performance::PerformanceState;
use crate::error::Result;
use crate::health::{HealthCalculator, HealthParams};
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the health command
pub fn run_health(format: OutputFormat, gpu_index: Option<u32>) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Determine which GPUs to check
    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    let calculator = HealthCalculator::default();

    for idx in indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;

        // Gather metrics for health calculation
        let temperature = device.temperature()?;
        let thermal_thresholds = device.thermal_thresholds()?;
        let power_usage = device.power_usage()?;
        let power_limit = device.power_limit()?;
        let memory_info = device.memory_info().ok();
        let utilization = device.utilization().ok();
        let performance_state = device.performance_state().ok();
        let ecc_errors = device.ecc_errors().ok().flatten();
        let pcie_metrics = device.pcie_metrics().ok();

        // Determine throttling status
        let is_thermal_throttling = if let Some(thresholds) = thermal_thresholds.slowdown {
            temperature.as_celsius() >= thresholds.as_celsius()
        } else {
            false
        };

        let is_power_throttling =
            power_usage.as_watts() as f64 >= power_limit.as_watts() as f64 * 0.99; // 99% threshold

        // Calculate VRAM usage ratio
        let vram_usage_ratio = memory_info.map(|info| {
            if info.total > 0 {
                info.used as f64 / info.total as f64
            } else {
                0.0
            }
        });

        // Get uptime for ECC error rate calculation (use 1 hour as estimate)
        let uptime_seconds = 3600;

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
            throttle_reasons: if let Some(state) = performance_state {
                if state != PerformanceState::P0 {
                    Some(format!("GPU in power state {:?}", state))
                } else {
                    None
                }
            } else {
                None
            },
        };

        print_output(&health_status, format)?;
        println!(); // Separator between GPUs
    }

    Ok(())
}
