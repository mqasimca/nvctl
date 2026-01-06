//! Application state definitions
//!
//! Contains all state types for the nvctl-gui application.

#![allow(dead_code)]

use crate::message::{GpuStateSnapshot, View};
use crate::services::Profile;
use nvctl::domain::{
    FanCurve, FanPolicy, FanSpeed, GpuInfo, PowerConstraints, PowerLimit, Temperature,
    ThermalThresholds,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum history length for metrics (5 minutes at 1Hz)
const MAX_HISTORY_LEN: usize = 300;

/// Main application state
#[derive(Debug)]
pub struct AppState {
    /// Current view
    pub current_view: View,

    /// Whether sidebar is expanded
    pub sidebar_expanded: bool,

    /// All detected GPUs
    pub gpus: Vec<GpuState>,

    /// Currently selected GPU index
    pub selected_gpu: usize,

    /// Whether GPUs are linked (apply settings to all)
    pub linked_gpus: bool,

    /// Current notification/error message
    pub notification: Option<Notification>,

    /// Fan curves being edited (one per fan on current GPU)
    pub editing_curves: Vec<Option<FanCurve>>,

    /// Whether curve control is enabled per fan
    pub curve_control_enabled: Vec<bool>,

    /// Currently selected fan for curve editing
    pub selected_curve_fan: usize,

    /// Available profiles
    pub profiles: Vec<Profile>,

    /// Currently active profile name
    pub active_profile: Option<String>,

    /// New profile name input
    pub new_profile_name: String,

    /// Profile pending deletion (for confirmation dialog)
    pub pending_delete_profile: Option<String>,

    /// Profile being edited (original name)
    pub editing_profile: Option<String>,

    /// New name for profile being edited
    pub edit_profile_name: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_view: View::Dashboard,
            sidebar_expanded: true,
            gpus: Vec::new(),
            selected_gpu: 0,
            linked_gpus: false,
            notification: None,
            editing_curves: Vec::new(),
            curve_control_enabled: Vec::new(),
            selected_curve_fan: 0,
            profiles: Vec::new(),
            active_profile: None,
            new_profile_name: String::new(),
            pending_delete_profile: None,
            editing_profile: None,
            edit_profile_name: String::new(),
        }
    }
}

impl AppState {
    /// Create a new application state
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the currently selected GPU, if any
    pub fn current_gpu(&self) -> Option<&GpuState> {
        self.gpus.get(self.selected_gpu)
    }

    /// Get mutable reference to the currently selected GPU
    pub fn current_gpu_mut(&mut self) -> Option<&mut GpuState> {
        self.gpus.get_mut(self.selected_gpu)
    }

    /// Update GPU state from a snapshot
    pub fn update_gpu(&mut self, snapshot: GpuStateSnapshot) {
        if let Some(gpu) = self.gpus.iter_mut().find(|g| g.index == snapshot.index) {
            gpu.update_from_snapshot(snapshot);
        }
    }

    /// Set a notification
    pub fn set_notification(&mut self, notification: Notification) {
        self.notification = Some(notification);
    }

    /// Clear the current notification
    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    /// Get the editing curve for a specific fan, or the stored curve
    #[allow(dead_code)]
    pub fn get_active_curve(&self, fan_idx: usize) -> Option<&FanCurve> {
        self.editing_curves
            .get(fan_idx)
            .and_then(|c| c.as_ref())
            .or_else(|| {
                self.current_gpu()
                    .and_then(|gpu| gpu.fan_curves.get(fan_idx))
            })
    }

    /// Get the curve for the currently selected fan
    pub fn get_selected_curve(&self) -> Option<FanCurve> {
        self.editing_curves
            .get(self.selected_curve_fan)
            .and_then(|c| c.clone())
            .or_else(|| {
                self.current_gpu()
                    .and_then(|gpu| gpu.fan_curves.get(self.selected_curve_fan).cloned())
            })
    }

    /// Check if curve control is enabled for a specific fan
    pub fn is_curve_enabled(&self, fan_idx: usize) -> bool {
        self.curve_control_enabled
            .get(fan_idx)
            .copied()
            .unwrap_or(false)
    }

    /// Initialize curves for current GPU's fan count
    pub fn init_curves_for_gpu(&mut self, fan_count: usize) {
        if self.editing_curves.len() != fan_count {
            self.editing_curves = vec![None; fan_count];
        }
        if self.curve_control_enabled.len() != fan_count {
            self.curve_control_enabled = vec![false; fan_count];
        }
        if self.selected_curve_fan >= fan_count && fan_count > 0 {
            self.selected_curve_fan = 0;
        }
    }

    /// Get GPU indices to apply settings to (respects linked_gpus mode)
    pub fn target_gpu_indices(&self) -> Vec<u32> {
        if self.linked_gpus {
            self.gpus.iter().map(|g| g.index).collect()
        } else if let Some(gpu) = self.current_gpu() {
            vec![gpu.index]
        } else {
            vec![]
        }
    }

    /// Check if multiple GPUs are available
    pub fn has_multiple_gpus(&self) -> bool {
        self.gpus.len() > 1
    }

    /// Get summary stats across all GPUs
    pub fn all_gpus_summary(&self) -> MultiGpuSummary {
        if self.gpus.is_empty() {
            return MultiGpuSummary::default();
        }

        let temps: Vec<i32> = self
            .gpus
            .iter()
            .map(|g| g.temperature.as_celsius())
            .collect();
        let powers: Vec<u32> = self.gpus.iter().map(|g| g.power_usage.as_watts()).collect();

        MultiGpuSummary {
            count: self.gpus.len(),
            max_temp: temps.iter().copied().max().unwrap_or(0),
            min_temp: temps.iter().copied().min().unwrap_or(0),
            avg_temp: temps.iter().sum::<i32>() / temps.len() as i32,
            total_power: powers.iter().sum(),
        }
    }
}

/// Summary statistics for multiple GPUs
#[derive(Debug, Clone, Default)]
pub struct MultiGpuSummary {
    /// Number of GPUs
    pub count: usize,
    /// Maximum temperature
    pub max_temp: i32,
    /// Minimum temperature
    pub min_temp: i32,
    /// Average temperature
    pub avg_temp: i32,
    /// Total power usage
    pub total_power: u32,
}

/// State for a single GPU
#[derive(Debug, Clone)]
pub struct GpuState {
    /// GPU index
    pub index: u32,

    /// GPU information
    pub info: GpuInfo,

    /// Current temperature
    pub temperature: Temperature,

    /// Temperature history
    pub temp_history: MetricsHistory,

    /// Thermal thresholds
    pub thresholds: ThermalThresholds,

    /// Fan speeds (one per fan)
    pub fan_speeds: Vec<FanSpeed>,

    /// Fan policies (one per fan)
    pub fan_policies: Vec<FanPolicy>,

    /// Fan curves (one per fan controller)
    pub fan_curves: Vec<FanCurve>,

    /// Current power usage
    pub power_usage: PowerLimit,

    /// Current power limit
    pub power_limit: PowerLimit,

    /// Power constraints
    pub power_constraints: Option<PowerConstraints>,

    /// Power usage history
    pub power_history: MetricsHistory,

    /// Last update timestamp
    pub last_update: Instant,
}

impl GpuState {
    /// Create a new GPU state from info
    pub fn new(info: GpuInfo) -> Self {
        Self {
            index: info.index,
            info,
            temperature: Temperature::new(0),
            temp_history: MetricsHistory::new(),
            thresholds: ThermalThresholds::default(),
            fan_speeds: Vec::new(),
            fan_policies: Vec::new(),
            fan_curves: Vec::new(),
            power_usage: PowerLimit::from_watts(0),
            power_limit: PowerLimit::from_watts(0),
            power_constraints: None,
            power_history: MetricsHistory::new(),
            last_update: Instant::now(),
        }
    }

    /// Update state from a snapshot
    pub fn update_from_snapshot(&mut self, snapshot: GpuStateSnapshot) {
        self.temperature = snapshot.temperature;
        self.fan_speeds = snapshot.fan_speeds;
        self.fan_policies = snapshot.fan_policies;
        self.power_usage = snapshot.power_usage;
        self.power_limit = snapshot.power_limit;
        self.last_update = snapshot.timestamp;

        // Add to history
        self.temp_history
            .push(snapshot.temperature.as_celsius() as f32);
        self.power_history
            .push(snapshot.power_usage.as_watts() as f32);
    }

    /// Get average fan speed across all fans
    pub fn average_fan_speed(&self) -> Option<u8> {
        if self.fan_speeds.is_empty() {
            return None;
        }

        let sum: u32 = self
            .fan_speeds
            .iter()
            .map(|s| s.as_percentage() as u32)
            .sum();
        Some((sum / self.fan_speeds.len() as u32) as u8)
    }

    /// Check if any fan is in manual mode
    pub fn has_manual_fans(&self) -> bool {
        self.fan_policies.contains(&FanPolicy::Manual)
    }

    /// Get power usage ratio (current / limit)
    pub fn power_ratio(&self) -> f32 {
        if self.power_limit.as_watts() == 0 {
            return 0.0;
        }
        self.power_usage.as_watts() as f32 / self.power_limit.as_watts() as f32
    }
}

/// History buffer for metrics
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MetricsHistory {
    /// Data points
    data: VecDeque<f32>,

    /// Maximum value seen
    pub max: f32,

    /// Minimum value seen
    pub min: f32,
}

impl MetricsHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self {
            data: VecDeque::with_capacity(MAX_HISTORY_LEN),
            max: f32::MIN,
            min: f32::MAX,
        }
    }

    /// Push a new value
    pub fn push(&mut self, value: f32) {
        if self.data.len() >= MAX_HISTORY_LEN {
            self.data.pop_front();
        }
        self.data.push_back(value);

        // Update min/max
        if value > self.max {
            self.max = value;
        }
        if value < self.min {
            self.min = value;
        }
    }

    /// Get all data points
    pub fn data(&self) -> &VecDeque<f32> {
        &self.data
    }

    /// Get the most recent value
    pub fn latest(&self) -> Option<f32> {
        self.data.back().copied()
    }

    /// Get the number of data points
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all data
    pub fn clear(&mut self) {
        self.data.clear();
        self.max = f32::MIN;
        self.min = f32::MAX;
    }
}

impl Default for MetricsHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification message to display
#[derive(Debug, Clone)]
pub struct Notification {
    /// Message content
    pub message: String,

    /// Notification level
    pub level: NotificationLevel,

    /// When the notification was created
    pub created_at: Instant,

    /// Duration before auto-dismiss (None = manual dismiss)
    pub duration: Option<Duration>,
}

impl Notification {
    /// Create a success notification
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Success,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(3)),
        }
    }

    /// Create a warning notification
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Warning,
            created_at: Instant::now(),
            duration: Some(Duration::from_secs(5)),
        }
    }

    /// Create an error notification
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Error,
            created_at: Instant::now(),
            duration: None, // Errors require manual dismiss
        }
    }

    /// Check if notification should be dismissed
    pub fn should_dismiss(&self) -> bool {
        match self.duration {
            Some(duration) => self.created_at.elapsed() >= duration,
            None => false,
        }
    }
}

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Success message
    Success,
    /// Warning message
    Warning,
    /// Error message
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_history() {
        let mut history = MetricsHistory::new();
        assert!(history.is_empty());

        history.push(50.0);
        history.push(60.0);
        history.push(55.0);

        assert_eq!(history.len(), 3);
        assert_eq!(history.latest(), Some(55.0));
        assert_eq!(history.max, 60.0);
        assert_eq!(history.min, 50.0);
    }

    #[test]
    fn test_metrics_history_max_size() {
        let mut history = MetricsHistory::new();

        // Fill beyond capacity
        for i in 0..=MAX_HISTORY_LEN {
            history.push(i as f32);
        }

        assert_eq!(history.len(), MAX_HISTORY_LEN);
        // Oldest value (0) should be gone
        assert_eq!(history.data().front(), Some(&1.0));
    }

    #[test]
    fn test_notification_auto_dismiss() {
        let notif = Notification::success("Test");
        assert!(!notif.should_dismiss()); // Just created

        let error = Notification::error("Error");
        assert!(!error.should_dismiss()); // Errors never auto-dismiss
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert_eq!(state.current_view, View::Dashboard);
        assert!(state.gpus.is_empty());
        assert_eq!(state.selected_gpu, 0);
    }
}
