//! Fan curve daemon service
//!
//! Background service that monitors GPU temperature and adjusts fan speeds
//! according to configured fan curves.

use crate::services::GpuMonitor;
use nvctl::domain::{FanCurve, FanPolicy, FanSpeed};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// Configuration for a single fan's curve control
#[derive(Debug, Clone)]
pub struct FanCurveConfig {
    /// GPU index
    pub gpu_index: u32,
    /// Fan index
    pub fan_index: u32,
    /// The fan curve to apply
    pub curve: FanCurve,
    /// Whether this curve control is enabled
    pub enabled: bool,
}

/// Shared state for the curve daemon
#[derive(Debug, Default)]
pub struct CurveDaemonState {
    /// Fan curve configurations keyed by (gpu_index, fan_index)
    curves: HashMap<(u32, u32), FanCurveConfig>,
    /// Last applied speed per fan (to avoid unnecessary writes)
    last_speeds: HashMap<(u32, u32), u8>,
}

impl CurveDaemonState {
    /// Set or update a fan curve configuration
    pub fn set_curve(&mut self, gpu_index: u32, fan_index: u32, curve: FanCurve, enabled: bool) {
        let config = FanCurveConfig {
            gpu_index,
            fan_index,
            curve,
            enabled,
        };
        self.curves.insert((gpu_index, fan_index), config);
    }

    /// Enable or disable curve control for a fan
    #[allow(dead_code)]
    pub fn set_enabled(&mut self, gpu_index: u32, fan_index: u32, enabled: bool) {
        if let Some(config) = self.curves.get_mut(&(gpu_index, fan_index)) {
            config.enabled = enabled;
        }
    }

    /// Remove a fan curve configuration
    pub fn remove_curve(&mut self, gpu_index: u32, fan_index: u32) {
        self.curves.remove(&(gpu_index, fan_index));
        self.last_speeds.remove(&(gpu_index, fan_index));
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
    pub fn set_last_speed(&mut self, gpu_index: u32, fan_index: u32, speed: u8) {
        self.last_speeds.insert((gpu_index, fan_index), speed);
    }

    /// Get last applied speed
    pub fn last_speed(&self, gpu_index: u32, fan_index: u32) -> Option<u8> {
        self.last_speeds.get(&(gpu_index, fan_index)).copied()
    }
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

        self.running.store(true, Ordering::SeqCst);

        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);

        let handle = thread::spawn(move || {
            log::info!("Fan curve daemon started");
            let monitor = GpuMonitor::new();

            while running.load(Ordering::SeqCst) {
                // Process curves
                if let Ok(mut state_guard) = state.write() {
                    let configs: Vec<FanCurveConfig> = state_guard
                        .enabled_curves()
                        .iter()
                        .map(|c| (*c).clone())
                        .collect();

                    for config in configs {
                        // Get current temperature
                        if let Some(snapshot) = monitor.poll_gpu(config.gpu_index) {
                            let temp = snapshot.temperature.as_celsius();
                            let target_speed = config.curve.speed_for_temperature(temp);
                            let target_pct = target_speed.as_percentage();

                            // Check if speed changed
                            let last = state_guard.last_speed(config.gpu_index, config.fan_index);
                            if last != Some(target_pct) {
                                // Ensure fan is in manual mode first
                                if let Some(policy) =
                                    snapshot.fan_policies.get(config.fan_index as usize)
                                {
                                    if *policy != FanPolicy::Manual {
                                        if let Err(e) = monitor.set_fan_policy(
                                            config.gpu_index,
                                            config.fan_index,
                                            FanPolicy::Manual,
                                        ) {
                                            log::warn!(
                                                "Failed to set fan {} to manual mode: {}",
                                                config.fan_index,
                                                e
                                            );
                                            continue;
                                        }
                                    }
                                }

                                // Set fan speed
                                if let Ok(speed) = FanSpeed::new(target_pct) {
                                    match monitor.set_fan_speed(
                                        config.gpu_index,
                                        config.fan_index,
                                        speed,
                                    ) {
                                        Ok(()) => {
                                            log::debug!(
                                                "GPU {} Fan {}: {}°C -> {}%",
                                                config.gpu_index,
                                                config.fan_index,
                                                temp,
                                                target_pct
                                            );
                                            state_guard.set_last_speed(
                                                config.gpu_index,
                                                config.fan_index,
                                                target_pct,
                                            );
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "Failed to set fan speed for GPU {} Fan {}: {}",
                                                config.gpu_index,
                                                config.fan_index,
                                                e
                                            );
                                        }
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
            let _ = handle.join();
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

    // Load config (reserved for future use with per-fan curve configuration)
    let _config = if let Some(path) = config_path {
        GuiConfig::load_from(path).map_err(|e| format!("Failed to load config: {}", e))?
    } else {
        GuiConfig::load()
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

    // For standalone mode, we need to load curves from a config file
    // For now, use default balanced curve for all fans if curve_control is enabled
    {
        let daemon_state = daemon.state();
        let mut state = daemon_state.write().map_err(|e| e.to_string())?;

        for gpu in &gpus {
            let fan_count = gpu.fan_speeds.len();
            for fan_idx in 0..fan_count {
                // Check if curve control is enabled in config
                // For standalone, default to using balanced curve
                let curve = nvctl::domain::FanCurve::default();
                state.set_curve(gpu.index, fan_idx as u32, curve, true);
                log::info!(
                    "Enabled curve control for GPU {} Fan {}",
                    gpu.index,
                    fan_idx
                );
            }
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
        thread::sleep(Duration::from_millis(100));
    }

    // Stop daemon
    daemon.stop();

    // Restore fans to auto mode
    log::info!("Restoring fans to auto mode...");
    for gpu in &gpus {
        let fan_count = gpu.fan_speeds.len();
        for fan_idx in 0..fan_count {
            if let Err(e) = monitor.set_fan_policy(gpu.index, fan_idx as u32, FanPolicy::Auto) {
                log::warn!("Failed to restore fan {} to auto: {}", fan_idx, e);
            }
        }
    }

    log::info!("Daemon stopped");
    Ok(())
}
