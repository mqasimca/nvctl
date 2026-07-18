//! Fan curve daemon service
//!
//! Background service that monitors GPU temperature and adjusts fan speeds
//! according to configured fan curves.

use crate::services::GpuMonitor;
use nvctl::domain::{FanCurve, FanPolicy};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

struct RunningReset(Arc<AtomicBool>);

impl Drop for RunningReset {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Configuration for a single fan's curve control
#[derive(Debug, Clone)]
pub struct FanCurveConfig {
    /// Stable GPU UUID
    pub gpu_uuid: String,
    /// Fan index
    pub fan_index: u32,
    /// The fan curve to apply
    pub curve: FanCurve,
    /// Whether this curve control is enabled
    pub enabled: bool,
}

/// Shared state for the curve daemon
#[derive(Debug, Clone, Default)]
pub struct CurveDaemonState {
    /// Fan curve configurations keyed by (GPU UUID, fan index)
    curves: HashMap<(String, u32), FanCurveConfig>,
    /// Last applied speed per fan (to avoid unnecessary writes)
    last_speeds: HashMap<(String, u32), u8>,
    /// Fans whose policy is owned by this daemon and must be restored on release
    controlled_fans: HashSet<(String, u32)>,
}

impl CurveDaemonState {
    /// Set or update a fan curve configuration
    pub fn set_curve(
        &mut self,
        gpu_uuid: impl Into<String>,
        fan_index: u32,
        curve: FanCurve,
        enabled: bool,
    ) {
        let gpu_uuid = gpu_uuid.into();
        let config = FanCurveConfig {
            gpu_uuid: gpu_uuid.clone(),
            fan_index,
            curve,
            enabled,
        };
        self.curves.insert((gpu_uuid, fan_index), config);
    }

    /// Enable or disable curve control for a fan
    #[allow(dead_code)]
    pub fn set_enabled(&mut self, gpu_uuid: &str, fan_index: u32, enabled: bool) {
        if let Some(config) = self.curves.get_mut(&(gpu_uuid.to_string(), fan_index)) {
            config.enabled = enabled;
        }
    }

    /// Get all enabled curve configurations
    pub fn enabled_curves(&self) -> Vec<&FanCurveConfig> {
        self.curves.values().filter(|c| c.enabled).collect()
    }

    /// Check if any curves are enabled
    pub fn has_enabled_curves(&self) -> bool {
        self.curves.values().any(|c| c.enabled)
    }

    /// Record last applied speed
    pub fn set_last_speed(&mut self, gpu_uuid: &str, fan_index: u32, speed: u8) {
        self.last_speeds
            .insert((gpu_uuid.to_string(), fan_index), speed);
    }

    /// Get last applied speed
    pub fn last_speed(&self, gpu_uuid: &str, fan_index: u32) -> Option<u8> {
        self.last_speeds
            .get(&(gpu_uuid.to_string(), fan_index))
            .copied()
    }

    fn mark_controlled(&mut self, gpu_uuid: &str, fan_index: u32) {
        self.controlled_fans
            .insert((gpu_uuid.to_string(), fan_index));
    }

    fn release_controlled(&mut self, gpu_uuid: &str, fan_index: u32) {
        self.controlled_fans
            .remove(&(gpu_uuid.to_string(), fan_index));
        self.last_speeds.remove(&(gpu_uuid.to_string(), fan_index));
    }

    fn is_controlled(&self, gpu_uuid: &str, fan_index: u32) -> bool {
        self.controlled_fans
            .contains(&(gpu_uuid.to_string(), fan_index))
    }

    fn controlled_fans(&self) -> Vec<(String, u32)> {
        self.controlled_fans.iter().cloned().collect()
    }
}

fn restore_auto_after_failure<F>(
    state: &mut CurveDaemonState,
    gpu_uuid: &str,
    fan_index: u32,
    restore: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    restore()?;
    state.release_controlled(gpu_uuid, fan_index);
    Ok(())
}

/// Fan curve daemon that runs in the background
pub struct CurveDaemon {
    /// Shared state
    state: Arc<RwLock<CurveDaemonState>>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Thread handle
    handle: Option<thread::JoinHandle<()>>,
}

impl CurveDaemon {
    /// Create a new curve daemon (not started)
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(CurveDaemonState::default())),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Get a handle to the shared state for updating curves
    pub fn state(&self) -> Arc<RwLock<CurveDaemonState>> {
        Arc::clone(&self.state)
    }

    /// Check if the daemon is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start the daemon
    pub fn start(&mut self) {
        if self.is_running() {
            return;
        }

        // Reap an unexpectedly exited worker before replacing its handle.
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                log::error!("Previous fan curve daemon thread terminated unexpectedly");
            }
        }

        self.running.store(true, Ordering::SeqCst);

        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);

        let handle = thread::spawn(move || {
            let _running_reset = RunningReset(Arc::clone(&running));
            log::info!("Fan curve daemon started");
            let monitor = GpuMonitor::new();

            while running.load(Ordering::SeqCst) {
                // Process curves
                {
                    let mut state_guard = match state.write() {
                        Ok(state) => state,
                        Err(poisoned) => {
                            log::error!("Fan curve state lock was poisoned; recovering it");
                            poisoned.into_inner()
                        }
                    };
                    let configs: Vec<FanCurveConfig> = state_guard
                        .enabled_curves()
                        .iter()
                        .map(|c| (*c).clone())
                        .collect();
                    let enabled_keys: HashSet<(String, u32)> = configs
                        .iter()
                        .map(|config| (config.gpu_uuid.clone(), config.fan_index))
                        .collect();

                    // A disabled or removed curve immediately gives policy back to NVML.
                    for (gpu_uuid, fan_index) in state_guard.controlled_fans() {
                        if !enabled_keys.contains(&(gpu_uuid.clone(), fan_index)) {
                            match restore_auto_after_failure(
                                &mut state_guard,
                                &gpu_uuid,
                                fan_index,
                                || {
                                    monitor.set_fan_policy_by_uuid(
                                        &gpu_uuid,
                                        fan_index,
                                        FanPolicy::Auto,
                                    )
                                },
                            ) {
                                Ok(()) => {}
                                Err(error) => log::warn!(
                                    "Failed to restore GPU {} Fan {} to auto: {}",
                                    gpu_uuid,
                                    fan_index,
                                    error
                                ),
                            }
                        }
                    }

                    for config in configs {
                        // Read actual hardware state every cycle so external policy/speed changes
                        // are corrected even when the curve target itself did not change.
                        match monitor.poll_fan_control_by_uuid(&config.gpu_uuid, config.fan_index) {
                            Ok((temperature, current_speed, policy)) => {
                                let temp = temperature.as_celsius();
                                let target_speed = config.curve.speed_for_temperature(temp);
                                let target_pct = target_speed.as_percentage();
                                let last =
                                    state_guard.last_speed(&config.gpu_uuid, config.fan_index);

                                let policy_changed = policy != FanPolicy::Manual;
                                if policy_changed {
                                    if let Err(error) = monitor.set_fan_policy_by_uuid(
                                        &config.gpu_uuid,
                                        config.fan_index,
                                        FanPolicy::Manual,
                                    ) {
                                        log::warn!(
                                            "Failed to set GPU {} Fan {} to manual mode: {}",
                                            config.gpu_uuid,
                                            config.fan_index,
                                            error
                                        );
                                        continue;
                                    }
                                }

                                state_guard.mark_controlled(&config.gpu_uuid, config.fan_index);
                                if policy_changed
                                    || current_speed.as_percentage() != target_pct
                                    || last != Some(target_pct)
                                {
                                    let write_failed = match monitor.set_fan_speed_by_uuid(
                                        &config.gpu_uuid,
                                        config.fan_index,
                                        target_speed,
                                    ) {
                                        Ok(()) => {
                                            log::debug!(
                                                "GPU {} Fan {}: {}°C -> {}%",
                                                config.gpu_uuid,
                                                config.fan_index,
                                                temp,
                                                target_pct
                                            );
                                            state_guard.set_last_speed(
                                                &config.gpu_uuid,
                                                config.fan_index,
                                                target_pct,
                                            );
                                            false
                                        }
                                        Err(error) => {
                                            log::warn!(
                                                "Failed to set fan speed for GPU {} Fan {}: {}",
                                                config.gpu_uuid,
                                                config.fan_index,
                                                error
                                            );
                                            true
                                        }
                                    };
                                    if write_failed {
                                        // A fixed manual speed without reliable updates is
                                        // unsafe. Give control back to the driver immediately.
                                        match restore_auto_after_failure(
                                            &mut state_guard,
                                            &config.gpu_uuid,
                                            config.fan_index,
                                            || {
                                                monitor.set_fan_policy_by_uuid(
                                                    &config.gpu_uuid,
                                                    config.fan_index,
                                                    FanPolicy::Auto,
                                                )
                                            },
                                        ) {
                                            Ok(()) => {}
                                            Err(restore_error) => log::error!(
                                                "Failed to restore GPU {} Fan {} to auto after a speed-write failure: {}",
                                                config.gpu_uuid,
                                                config.fan_index,
                                                restore_error
                                            ),
                                        }
                                    }
                                } else {
                                    state_guard.set_last_speed(
                                        &config.gpu_uuid,
                                        config.fan_index,
                                        target_pct,
                                    );
                                }
                            }
                            Err(error) => {
                                log::warn!(
                                    "Failed to poll GPU {} for curve control: {}",
                                    config.gpu_uuid,
                                    error
                                );
                                if state_guard.is_controlled(&config.gpu_uuid, config.fan_index) {
                                    match restore_auto_after_failure(
                                        &mut state_guard,
                                        &config.gpu_uuid,
                                        config.fan_index,
                                        || {
                                            monitor.set_fan_policy_by_uuid(
                                                &config.gpu_uuid,
                                                config.fan_index,
                                                FanPolicy::Auto,
                                            )
                                        },
                                    ) {
                                        Ok(()) => {}
                                        Err(restore_error) => log::error!(
                                            "Failed to restore GPU {} Fan {} to auto after a telemetry failure: {}",
                                            config.gpu_uuid,
                                            config.fan_index,
                                            restore_error
                                        ),
                                    }
                                }
                            }
                        }
                    }
                }

                // Sleep for 1 second before next check
                thread::sleep(Duration::from_secs(1));
            }

            log::info!("Fan curve daemon stopped");
        });

        self.handle = Some(handle);
    }

    /// Stop the daemon
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                log::error!("Fan curve daemon thread terminated unexpectedly");
            }
        }

        self.restore_controlled_fans();
    }

    fn restore_controlled_fans(&self) {
        let controlled = match self.state.read() {
            Ok(state) => state.controlled_fans(),
            Err(poisoned) => {
                log::error!("Fan curve state lock was poisoned during shutdown; recovering it");
                poisoned.into_inner().controlled_fans()
            }
        };
        if controlled.is_empty() {
            return;
        }

        let monitor = GpuMonitor::new();
        for (gpu_uuid, fan_index) in controlled {
            match monitor.set_fan_policy_by_uuid(&gpu_uuid, fan_index, FanPolicy::Auto) {
                Ok(()) => match self.state.write() {
                    Ok(mut state) => state.release_controlled(&gpu_uuid, fan_index),
                    Err(poisoned) => poisoned
                        .into_inner()
                        .release_controlled(&gpu_uuid, fan_index),
                },
                Err(error) => log::warn!(
                    "Failed to restore GPU {} Fan {} to auto: {}",
                    gpu_uuid,
                    fan_index,
                    error
                ),
            }
        }
    }

    /// Restart the daemon
    #[allow(dead_code)]
    pub fn restart(&mut self) {
        self.stop();
        self.start();
    }
}

impl Default for CurveDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CurveDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the daemon in standalone mode (no GUI)
/// This is used when running with --daemon flag
pub fn run_daemon_standalone(config_path: Option<&str>) -> Result<(), String> {
    use crate::services::GuiConfig;

    log::info!("Starting fan curve daemon in standalone mode");

    let config = if let Some(path) = config_path {
        GuiConfig::load_from(path).map_err(|e| format!("Failed to load config: {}", e))?
    } else {
        GuiConfig::load().map_err(|e| format!("Failed to load config: {}", e))?
    };

    let monitor = GpuMonitor::new();
    if !monitor.is_available() {
        return Err("NVML not available".to_string());
    }

    // Detect GPUs
    let gpus = monitor.detect_gpus();
    if gpus.is_empty() {
        return Err("No GPUs found".to_string());
    }

    log::info!("Found {} GPU(s)", gpus.len());

    // Create daemon and configure curves from saved config
    let mut daemon = CurveDaemon::new();

    let enabled_curves: Vec<_> = config
        .curve_control
        .iter()
        .filter(|curve| curve.enabled)
        .collect();
    if enabled_curves.is_empty() {
        return Err(
            "No enabled curve_control entries found; refusing to take over every fan by default"
                .to_string(),
        );
    }

    {
        let daemon_state = daemon.state();
        let mut state = daemon_state.write().map_err(|e| e.to_string())?;

        for curve_config in enabled_curves {
            let gpu = gpus
                .iter()
                .find(|gpu| gpu.info.uuid == curve_config.gpu_uuid)
                .ok_or_else(|| {
                    format!(
                        "Configured GPU UUID {} was not detected",
                        curve_config.gpu_uuid
                    )
                })?;
            if curve_config.fan_index as usize >= gpu.fan_speeds.len() {
                return Err(format!(
                    "Configured fan {} is out of range for GPU {} ({} fans)",
                    curve_config.fan_index,
                    curve_config.gpu_uuid,
                    gpu.fan_speeds.len()
                ));
            }

            state.set_curve(
                curve_config.gpu_uuid.clone(),
                curve_config.fan_index,
                curve_config.curve.clone(),
                true,
            );
            log::info!(
                "Enabled curve control for GPU {} Fan {}",
                curve_config.gpu_uuid,
                curve_config.fan_index
            );
        }
    }

    // Start daemon
    daemon.start();

    // Wait for SIGINT/SIGTERM
    log::info!("Daemon running. Press Ctrl+C to stop.");

    // Set up signal handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        log::info!("Received shutdown signal");
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| format!("Failed to set signal handler: {}", e))?;

    // Wait for shutdown signal
    while running.load(Ordering::SeqCst) {
        if !daemon.is_running() {
            return Err("Fan curve daemon stopped unexpectedly".to_string());
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Stop daemon
    daemon.stop();

    log::info!("Daemon stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_restore_releases_daemon_ownership() {
        let mut state = CurveDaemonState::default();
        state.mark_controlled("GPU-TWO", 1);
        state.set_last_speed("GPU-TWO", 1, 75);

        restore_auto_after_failure(&mut state, "GPU-TWO", 1, || Ok(())).unwrap();

        assert!(!state.is_controlled("GPU-TWO", 1));
        assert_eq!(state.last_speed("GPU-TWO", 1), None);
    }

    #[test]
    fn failed_restore_keeps_ownership_for_a_later_retry() {
        let mut state = CurveDaemonState::default();
        state.mark_controlled("GPU-TWO", 1);

        assert!(restore_auto_after_failure(&mut state, "GPU-TWO", 1, || {
            Err("policy write failed".to_string())
        })
        .is_err());
        assert!(state.is_controlled("GPU-TWO", 1));
    }

    #[test]
    fn running_reset_clears_the_thread_flag() {
        let running = Arc::new(AtomicBool::new(true));
        {
            let _reset = RunningReset(Arc::clone(&running));
        }
        assert!(!running.load(Ordering::SeqCst));
    }
}
