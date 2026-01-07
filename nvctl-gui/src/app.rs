//! Main application structure
//!
//! Implements the Elm Architecture (TEA) pattern for nvctl-gui.

use crate::message::{KeyboardShortcut, Message, View};
use crate::services::{
    start_tray, CurveDaemon, GpuMonitor, GpuSettings, GuiConfig, Profile, ProfileService,
    TrayHandle,
};
use crate::state::{AppState, Notification};
use crate::theme::{colors, font_size, nvctl_theme, spacing};
use crate::views;

use iced::keyboard::{self, key::Named, Key, Modifiers};
use iced::widget::{button, column, container, horizontal_space, row, text, Column, Space};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};
use nvctl::domain::FanPolicy;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Main application
pub struct NvctlGui {
    /// Application state
    state: AppState,

    /// GPU monitoring service
    monitor: GpuMonitor,

    /// Profile management service
    profile_service: ProfileService,

    /// System tray handle (optional)
    tray: Option<TrayHandle>,

    /// GUI configuration (fan labels, preferences)
    config: GuiConfig,

    /// Fan curve daemon for automatic fan control
    curve_daemon: Arc<RwLock<CurveDaemon>>,
}

impl NvctlGui {
    /// Create a new application instance
    pub fn new() -> (Self, Task<Message>) {
        let monitor = GpuMonitor::new();
        let gpus = monitor.detect_gpus();
        let profile_service = ProfileService::new();

        // Load GUI config (fan labels, etc.)
        let mut config = GuiConfig::load();

        // Detect cooler targets for each GPU and store in config
        for gpu in &gpus {
            let targets = monitor.get_cooler_targets(gpu.index);
            let total_fans = targets.len() as u32;
            let gpu_config = config.get_gpu_fan_config(&gpu.info.uuid);

            for (idx, target) in targets.into_iter().enumerate() {
                gpu_config.set_detected_target(idx as u32, target, total_fans);
            }
        }

        // Save updated config with detected targets
        if let Err(e) = config.save() {
            log::warn!("Failed to save config: {}", e);
        }

        let mut state = AppState::new();
        state.gpus = gpus;

        // Initialize fan curves for the first GPU
        if let Some(gpu) = state.gpus.first() {
            state.init_curves_for_gpu(gpu.fan_speeds.len());
        }

        // Load profiles into state
        state.profiles = profile_service.list().into_iter().cloned().collect();
        state.active_profile = profile_service.active_name().map(String::from);

        if !monitor.is_available() {
            state.set_notification(Notification::warning(
                "NVML not available. Running in demo mode.",
            ));
        }

        // Start system tray
        let tray = start_tray();
        if tray.is_none() {
            log::info!("System tray not available on this system");
        }

        // Create curve daemon (not started yet)
        let curve_daemon = Arc::new(RwLock::new(CurveDaemon::new()));

        let app = Self {
            state,
            monitor,
            profile_service,
            tray,
            config,
            curve_daemon,
        };

        (app, Task::none())
    }

    /// Update application state based on a message
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Navigation
            Message::ViewChanged(view) => {
                self.state.current_view = view;
                Task::none()
            }

            Message::SidebarToggled => {
                self.state.sidebar_expanded = !self.state.sidebar_expanded;
                Task::none()
            }

            Message::KeyPressed(shortcut) => {
                // Handle keyboard shortcuts
                if let Some(view) = shortcut.to_view() {
                    self.state.current_view = view;
                } else {
                    match shortcut {
                        KeyboardShortcut::Refresh => {
                            self.state.gpus = self.monitor.detect_gpus();
                            if self.state.selected_gpu >= self.state.gpus.len() {
                                self.state.selected_gpu = 0;
                            }
                            self.state
                                .set_notification(Notification::success("GPU data refreshed"));
                        }
                        KeyboardShortcut::ToggleSidebar => {
                            self.state.sidebar_expanded = !self.state.sidebar_expanded;
                        }
                        _ => {}
                    }
                }
                Task::none()
            }

            // GPU selection
            Message::GpuSelected(index) => {
                if index < self.state.gpus.len() {
                    self.state.selected_gpu = index;
                    // Initialize curves for the new GPU's fan count
                    let fan_count = self.state.gpus[index].fan_speeds.len();
                    self.state.init_curves_for_gpu(fan_count);
                }
                Task::none()
            }

            Message::LinkedGpusToggled(linked) => {
                self.state.linked_gpus = linked;
                let msg = if linked {
                    "GPUs linked - settings apply to all"
                } else {
                    "GPUs unlinked - settings apply to selected GPU only"
                };
                self.state.set_notification(Notification::success(msg));
                Task::none()
            }

            Message::RefreshGpus => {
                self.state.gpus = self.monitor.detect_gpus();
                if self.state.selected_gpu >= self.state.gpus.len() {
                    self.state.selected_gpu = 0;
                }
                Task::none()
            }

            // Polling
            Message::Tick(_now) => {
                // Collect GPU indices first to avoid borrow issues
                let gpu_indices: Vec<u32> = self.state.gpus.iter().map(|g| g.index).collect();

                // Poll all GPUs
                for index in gpu_indices {
                    if let Some(snapshot) = self.monitor.poll_gpu(index) {
                        self.state.update_gpu(snapshot);
                    }
                }

                // Update system tray with current GPU temperature
                if let Some(ref tray) = self.tray {
                    if let Some(gpu) = self.state.current_gpu() {
                        tray.update_temperature(gpu.temperature.as_celsius());
                    }
                    tray.update_profile(self.state.active_profile.clone());

                    // Check for tray quit request
                    if tray.quit_requested() {
                        return iced::exit();
                    }
                }

                // Check for notification dismissal
                if let Some(ref notif) = self.state.notification {
                    if notif.should_dismiss() {
                        self.state.clear_notification();
                    }
                }

                Task::none()
            }

            Message::GpuStateUpdated(snapshot) => {
                self.state.update_gpu(*snapshot);
                Task::none()
            }

            // Fan control
            Message::FanControl(msg) => {
                use crate::message::FanControlMessage;

                match msg {
                    FanControlMessage::PolicyChanged(fan_idx, policy) => {
                        // Get GPU indices to apply to
                        let gpu_indices = self.state.target_gpu_indices();

                        for gpu_index in gpu_indices {
                            match self.monitor.set_fan_policy(gpu_index, fan_idx, policy) {
                                Ok(()) => {
                                    // Update state on success
                                    if let Some(gpu) =
                                        self.state.gpus.iter_mut().find(|g| g.index == gpu_index)
                                    {
                                        if let Some(fan_policy) =
                                            gpu.fan_policies.get_mut(fan_idx as usize)
                                        {
                                            *fan_policy = policy;
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.state.set_notification(Notification::error(e));
                                    return Task::none();
                                }
                            }
                        }

                        self.state.set_notification(Notification::success(format!(
                            "Fan {} policy set to {:?}",
                            fan_idx + 1,
                            policy
                        )));
                    }
                    FanControlMessage::SpeedChanged(fan_idx, speed) => {
                        // Get GPU indices to apply to
                        let gpu_indices = self.state.target_gpu_indices();

                        for gpu_index in gpu_indices {
                            match self.monitor.set_fan_speed(gpu_index, fan_idx, speed) {
                                Ok(()) => {
                                    // Update state on success
                                    if let Some(gpu) =
                                        self.state.gpus.iter_mut().find(|g| g.index == gpu_index)
                                    {
                                        if let Some(fan_speed) =
                                            gpu.fan_speeds.get_mut(fan_idx as usize)
                                        {
                                            *fan_speed = speed;
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.state.set_notification(Notification::error(e));
                                    return Task::none();
                                }
                            }
                        }
                    }
                    FanControlMessage::SelectCurveFan(fan_idx) => {
                        self.state.selected_curve_fan = fan_idx;
                    }
                    FanControlMessage::CurvePointMoved { index, temp, speed } => {
                        // Update curve point for selected fan
                        let fan_idx = self.state.selected_curve_fan;

                        // Get the current curve (editing or from GPU state)
                        let current_curve = self
                            .state
                            .editing_curves
                            .get(fan_idx)
                            .and_then(|c| c.clone())
                            .or_else(|| {
                                self.state
                                    .current_gpu()
                                    .and_then(|gpu| gpu.fan_curves.get(fan_idx).cloned())
                            })
                            .unwrap_or_default();

                        // Clone points and update the moved point
                        let mut points: Vec<_> = current_curve.points().to_vec();
                        if let Some(point) = points.get_mut(index) {
                            // Clamp values to valid range
                            let clamped_temp = temp.clamp(20, 100);
                            let clamped_speed = speed.clamp(0, 100);

                            if let Ok(new_speed) = nvctl::domain::FanSpeed::new(clamped_speed) {
                                point.temperature = clamped_temp;
                                point.speed = new_speed;

                                // Create new curve with updated points
                                if let Ok(new_curve) = nvctl::domain::FanCurve::new(
                                    points,
                                    current_curve.default_speed(),
                                ) {
                                    // Ensure the editing_curves vec is large enough
                                    if fan_idx < self.state.editing_curves.len() {
                                        self.state.editing_curves[fan_idx] = Some(new_curve);
                                    }
                                }
                            }
                        }
                    }
                    FanControlMessage::CurvePointAdded(temp, speed) => {
                        use nvctl::domain::{FanCurvePoint, FanSpeed};

                        let fan_idx = self.state.selected_curve_fan;

                        // Get the current curve
                        let current_curve = self
                            .state
                            .editing_curves
                            .get(fan_idx)
                            .and_then(|c| c.clone())
                            .or_else(|| {
                                self.state
                                    .current_gpu()
                                    .and_then(|gpu| gpu.fan_curves.get(fan_idx).cloned())
                            })
                            .unwrap_or_default();

                        // Clone points and add new point
                        let mut points: Vec<_> = current_curve.points().to_vec();

                        // Clamp values
                        let clamped_temp = temp.clamp(20, 100);
                        let clamped_speed = speed.clamp(0, 100);

                        if let Ok(new_speed) = FanSpeed::new(clamped_speed) {
                            let new_point = FanCurvePoint::new(clamped_temp, new_speed);
                            points.push(new_point);

                            // Create new curve (will sort by temperature)
                            if let Ok(new_curve) =
                                nvctl::domain::FanCurve::new(points, current_curve.default_speed())
                            {
                                if fan_idx < self.state.editing_curves.len() {
                                    self.state.editing_curves[fan_idx] = Some(new_curve);
                                }
                                self.state.set_notification(Notification::success(format!(
                                    "Added point at {}°C, {}%",
                                    clamped_temp, clamped_speed
                                )));
                            }
                        }
                    }
                    FanControlMessage::CurvePointRemoved(index) => {
                        let fan_idx = self.state.selected_curve_fan;

                        // Get the current curve
                        let current_curve = self
                            .state
                            .editing_curves
                            .get(fan_idx)
                            .and_then(|c| c.clone())
                            .or_else(|| {
                                self.state
                                    .current_gpu()
                                    .and_then(|gpu| gpu.fan_curves.get(fan_idx).cloned())
                            })
                            .unwrap_or_default();

                        // Clone points and remove the specified point
                        let mut points: Vec<_> = current_curve.points().to_vec();

                        // Don't remove if only one point left
                        if points.len() > 1 && index < points.len() {
                            points.remove(index);

                            // Create new curve
                            if let Ok(new_curve) =
                                nvctl::domain::FanCurve::new(points, current_curve.default_speed())
                            {
                                if fan_idx < self.state.editing_curves.len() {
                                    self.state.editing_curves[fan_idx] = Some(new_curve);
                                }
                                self.state
                                    .set_notification(Notification::success("Point removed"));
                            }
                        }
                    }
                    FanControlMessage::PresetSelected(preset) => {
                        // Apply preset to selected fan's curve
                        let fan_idx = self.state.selected_curve_fan;
                        if fan_idx < self.state.editing_curves.len() {
                            self.state.editing_curves[fan_idx] = Some(preset.to_curve());
                        }
                        self.state.set_notification(Notification::success(format!(
                            "Applied {} preset to Fan {}",
                            preset.name(),
                            fan_idx + 1
                        )));
                    }
                    FanControlMessage::CurveControlToggled(enabled) => {
                        // Toggle curve control for selected fan
                        let fan_idx = self.state.selected_curve_fan;
                        if fan_idx < self.state.curve_control_enabled.len() {
                            self.state.curve_control_enabled[fan_idx] = enabled;
                        }
                        let status = if enabled { "enabled" } else { "disabled" };
                        self.state.set_notification(Notification::success(format!(
                            "Curve control {} for Fan {}",
                            status,
                            fan_idx + 1
                        )));
                    }
                    FanControlMessage::ApplyCurve => {
                        // Apply curve for selected fan
                        let fan_idx = self.state.selected_curve_fan;
                        let enabled = self
                            .state
                            .curve_control_enabled
                            .get(fan_idx)
                            .copied()
                            .unwrap_or(false);

                        // Get the curve for this fan
                        let curve = self
                            .state
                            .editing_curves
                            .get(fan_idx)
                            .and_then(|c| c.clone())
                            .or_else(|| {
                                self.state
                                    .current_gpu()
                                    .and_then(|gpu| gpu.fan_curves.get(fan_idx).cloned())
                            })
                            .unwrap_or_default();

                        // Get GPU indices to apply to
                        let gpu_indices = self.state.target_gpu_indices();

                        // Update daemon state
                        if let Ok(daemon) = self.curve_daemon.read() {
                            let daemon_state = daemon.state();
                            if let Ok(mut state) = daemon_state.write() {
                                for gpu_index in &gpu_indices {
                                    state.set_curve(
                                        *gpu_index,
                                        fan_idx as u32,
                                        curve.clone(),
                                        enabled,
                                    );
                                }
                            }
                            drop(daemon_state);
                        }

                        // Start or stop daemon based on enabled curves
                        if let Ok(mut daemon) = self.curve_daemon.write() {
                            let daemon_state = daemon.state();
                            let has_enabled = daemon_state
                                .read()
                                .map(|s| s.has_enabled_curves())
                                .unwrap_or(false);
                            drop(daemon_state);

                            if has_enabled && !daemon.is_running() {
                                daemon.start();
                                log::info!("Fan curve daemon started");
                            } else if !has_enabled && daemon.is_running() {
                                daemon.stop();
                                log::info!("Fan curve daemon stopped");

                                // Restore fans to auto mode
                                for gpu_index in &gpu_indices {
                                    let _ = self.monitor.set_fan_policy(
                                        *gpu_index,
                                        fan_idx as u32,
                                        FanPolicy::Auto,
                                    );
                                }
                            }
                        }

                        let status = if enabled { "enabled" } else { "disabled" };
                        self.state.set_notification(Notification::success(format!(
                            "Fan {} curve {} and saved",
                            fan_idx + 1,
                            status
                        )));
                    }
                }
                Task::none()
            }

            // Power control
            Message::PowerControl(msg) => {
                use crate::message::PowerControlMessage;

                match msg {
                    PowerControlMessage::LimitChanged(limit) => {
                        // Update power limit in state
                        if let Some(gpu) = self.state.current_gpu_mut() {
                            gpu.power_limit = limit;
                        }
                    }
                    PowerControlMessage::ResetToDefault => {
                        // Reset to default power limit
                        let default_watts = self
                            .state
                            .current_gpu()
                            .and_then(|g| g.power_constraints.as_ref())
                            .map(|c| c.default);

                        if let Some(default) = default_watts {
                            if let Some(gpu) = self.state.current_gpu_mut() {
                                gpu.power_limit = default;
                            }
                            self.state.set_notification(Notification::success(format!(
                                "Power limit reset to default ({}W)",
                                default.as_watts()
                            )));
                        }
                    }
                    PowerControlMessage::ApplyLimit => {
                        // Get current power limit from state
                        let limit = match self.state.current_gpu() {
                            Some(gpu) => gpu.power_limit,
                            None => return Task::none(),
                        };

                        // Get GPU indices to apply to
                        let gpu_indices = self.state.target_gpu_indices();

                        for gpu_index in gpu_indices {
                            match self.monitor.set_power_limit(gpu_index, limit) {
                                Ok(()) => {
                                    log::info!(
                                        "Power limit set to {}W for GPU {}",
                                        limit.as_watts(),
                                        gpu_index
                                    );
                                }
                                Err(e) => {
                                    self.state.set_notification(Notification::error(e));
                                    return Task::none();
                                }
                            }
                        }

                        self.state.set_notification(Notification::success(format!(
                            "Power limit set to {}W",
                            limit.as_watts()
                        )));
                    }
                }
                Task::none()
            }

            // Profiles
            Message::Profile(msg) => {
                use crate::message::ProfileMessage;

                match msg {
                    ProfileMessage::Selected(name) => {
                        // Set as active profile
                        self.profile_service.set_active(Some(name.clone()));
                        self.state.active_profile = Some(name.clone());

                        // Apply profile settings
                        if let Some(profile) = self.profile_service.get(&name) {
                            // Apply settings to current GPU if available
                            if let Some(settings) = profile.gpu_settings.get("default") {
                                if settings.apply_fan_curve {
                                    if let Some(ref curve) = settings.fan_curve {
                                        // Apply curve to all fans
                                        for i in 0..self.state.editing_curves.len() {
                                            self.state.editing_curves[i] = Some(curve.clone());
                                            if i < self.state.curve_control_enabled.len() {
                                                self.state.curve_control_enabled[i] = true;
                                            }
                                        }
                                    }
                                }
                                if settings.apply_power_limit {
                                    if let (Some(limit), Some(gpu)) =
                                        (settings.power_limit, self.state.current_gpu_mut())
                                    {
                                        gpu.power_limit = limit;
                                    }
                                }
                            }
                        }

                        self.state.set_notification(Notification::success(format!(
                            "Profile '{}' applied",
                            name
                        )));
                    }
                    ProfileMessage::SaveCurrent(name) => {
                        // Create profile from current settings
                        let mut profile = Profile::new(&name);

                        // Get current GPU settings
                        if let Some(gpu) = self.state.current_gpu() {
                            let mut settings = GpuSettings::new();

                            // Use the first fan's curve for the profile
                            if let Some(Some(ref curve)) = self.state.editing_curves.first() {
                                settings = settings.with_fan_curve(curve.clone());
                            }

                            settings = settings.with_power_limit(gpu.power_limit);

                            profile = profile.with_gpu_settings("default", settings);
                        }

                        // Save profile
                        match self.profile_service.save(profile) {
                            Ok(()) => {
                                // Refresh profiles in state
                                self.state.profiles =
                                    self.profile_service.list().into_iter().cloned().collect();
                                self.state.new_profile_name.clear();
                                self.state.set_notification(Notification::success(format!(
                                    "Profile '{}' saved",
                                    name
                                )));
                            }
                            Err(e) => {
                                self.state.set_notification(Notification::error(format!(
                                    "Failed to save profile: {}",
                                    e
                                )));
                            }
                        }
                    }
                    ProfileMessage::RequestDelete(name) => {
                        // Show confirmation dialog
                        self.state.pending_delete_profile = Some(name);
                    }
                    ProfileMessage::ConfirmDelete => {
                        if let Some(name) = self.state.pending_delete_profile.take() {
                            match self.profile_service.delete(&name) {
                                Ok(()) => {
                                    // Refresh profiles in state
                                    self.state.profiles =
                                        self.profile_service.list().into_iter().cloned().collect();
                                    if self.state.active_profile.as_ref() == Some(&name) {
                                        self.state.active_profile = None;
                                    }
                                    self.state.set_notification(Notification::success(format!(
                                        "Profile '{}' deleted",
                                        name
                                    )));
                                }
                                Err(e) => {
                                    self.state.set_notification(Notification::error(format!(
                                        "Failed to delete profile: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    }
                    ProfileMessage::CancelDelete => {
                        self.state.pending_delete_profile = None;
                    }
                    ProfileMessage::StartEdit(name) => {
                        self.state.editing_profile = Some(name.clone());
                        self.state.edit_profile_name = name;
                    }
                    ProfileMessage::EditNameChanged(name) => {
                        self.state.edit_profile_name = name;
                    }
                    ProfileMessage::ConfirmEdit => {
                        if let Some(old_name) = self.state.editing_profile.take() {
                            let new_name = self.state.edit_profile_name.trim().to_string();
                            if !new_name.is_empty() && new_name != old_name {
                                match self.profile_service.rename(&old_name, &new_name) {
                                    Ok(()) => {
                                        self.state.profiles = self
                                            .profile_service
                                            .list()
                                            .into_iter()
                                            .cloned()
                                            .collect();
                                        if self.state.active_profile.as_ref() == Some(&old_name) {
                                            self.state.active_profile = Some(new_name.clone());
                                        }
                                        self.state.set_notification(Notification::success(
                                            format!("Profile renamed to '{}'", new_name),
                                        ));
                                    }
                                    Err(e) => {
                                        self.state.set_notification(Notification::error(format!(
                                            "Failed to rename profile: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            self.state.edit_profile_name.clear();
                        }
                    }
                    ProfileMessage::CancelEdit => {
                        self.state.editing_profile = None;
                        self.state.edit_profile_name.clear();
                    }
                    ProfileMessage::Apply => {
                        if let Some(ref name) = self.state.active_profile.clone() {
                            // Re-apply current profile
                            return self
                                .update(Message::Profile(ProfileMessage::Selected(name.clone())));
                        }
                    }
                    ProfileMessage::NameInputChanged(name) => {
                        self.state.new_profile_name = name;
                    }
                    ProfileMessage::SetDefault(name) => {
                        match self.profile_service.set_default(&name) {
                            Ok(()) => {
                                self.state.profiles =
                                    self.profile_service.list().into_iter().cloned().collect();
                                self.state.set_notification(Notification::success(format!(
                                    "'{}' set as default profile",
                                    name
                                )));
                            }
                            Err(e) => {
                                self.state.set_notification(Notification::error(format!(
                                    "Failed to set default: {}",
                                    e
                                )));
                            }
                        }
                    }
                    ProfileMessage::Refresh => {
                        if let Err(e) = self.profile_service.load_all() {
                            log::warn!("Failed to refresh profiles: {}", e);
                        }
                        self.state.profiles =
                            self.profile_service.list().into_iter().cloned().collect();
                    }
                }
                Task::none()
            }

            // Actions
            Message::ApplySettings => {
                // Apply all current settings to GPU
                let mut errors = Vec::new();
                let gpu_indices = self.state.target_gpu_indices();

                // Apply power limit
                if let Some(gpu) = self.state.current_gpu() {
                    let limit = gpu.power_limit;
                    for gpu_index in &gpu_indices {
                        if let Err(e) = self.monitor.set_power_limit(*gpu_index, limit) {
                            errors.push(format!("Power limit: {}", e));
                        }
                    }
                }

                // Apply all fan curves - collect data first to avoid borrow issues
                let curve_data: Vec<_> = self
                    .state
                    .editing_curves
                    .iter()
                    .enumerate()
                    .map(|(fan_idx, maybe_curve)| {
                        let enabled = self
                            .state
                            .curve_control_enabled
                            .get(fan_idx)
                            .copied()
                            .unwrap_or(false);
                        let curve = maybe_curve.clone().unwrap_or_default();
                        (fan_idx, curve, enabled)
                    })
                    .collect();

                // Update daemon state for all fans
                if let Ok(daemon) = self.curve_daemon.read() {
                    let daemon_state = daemon.state();
                    if let Ok(mut state) = daemon_state.write() {
                        for (fan_idx, curve, enabled) in &curve_data {
                            for gpu_index in &gpu_indices {
                                state.set_curve(
                                    *gpu_index,
                                    *fan_idx as u32,
                                    curve.clone(),
                                    *enabled,
                                );
                            }
                        }
                    };
                }

                // Start daemon if any curves are enabled
                if let Ok(mut daemon) = self.curve_daemon.write() {
                    let daemon_state = daemon.state();
                    let has_enabled = daemon_state
                        .read()
                        .map(|s| s.has_enabled_curves())
                        .unwrap_or(false);
                    drop(daemon_state);

                    if has_enabled && !daemon.is_running() {
                        daemon.start();
                        log::info!("Fan curve daemon started");
                    }
                }

                if errors.is_empty() {
                    self.state
                        .set_notification(Notification::success("All settings applied"));
                } else {
                    self.state.set_notification(Notification::warning(format!(
                        "Some settings failed: {}",
                        errors.join(", ")
                    )));
                }
                Task::none()
            }

            Message::ResetSettings => {
                // Reset all settings to defaults
                let gpu_indices = self.state.target_gpu_indices();

                // Reset power limit to default
                if let Some(gpu) = self.state.current_gpu() {
                    if let Some(ref constraints) = gpu.power_constraints {
                        let default_limit = constraints.default;
                        for gpu_index in &gpu_indices {
                            let _ = self.monitor.set_power_limit(*gpu_index, default_limit);
                        }
                        if let Some(gpu_mut) = self.state.current_gpu_mut() {
                            gpu_mut.power_limit = default_limit;
                        }
                    }
                }

                // Reset all curves to default and disable curve control
                for i in 0..self.state.editing_curves.len() {
                    self.state.editing_curves[i] = Some(nvctl::domain::FanCurve::default());
                    if i < self.state.curve_control_enabled.len() {
                        self.state.curve_control_enabled[i] = false;
                    }
                }

                // Stop daemon and restore fans to auto
                let num_fans = self.state.editing_curves.len();
                if let Ok(mut daemon) = self.curve_daemon.write() {
                    if daemon.is_running() {
                        daemon.stop();
                        log::info!("Fan curve daemon stopped");
                    }

                    // Clear all curves from daemon state
                    let daemon_state = daemon.state();
                    if let Ok(mut state) = daemon_state.write() {
                        for gpu_index in &gpu_indices {
                            for fan_idx in 0..num_fans {
                                state.remove_curve(*gpu_index, fan_idx as u32);
                            }
                        }
                    }
                    drop(daemon_state);
                }

                // Restore fans to auto mode
                for gpu_index in &gpu_indices {
                    for fan_idx in 0..self.state.editing_curves.len() {
                        let _ = self.monitor.set_fan_policy(
                            *gpu_index,
                            fan_idx as u32,
                            FanPolicy::Auto,
                        );
                    }
                }

                self.state
                    .set_notification(Notification::success("Settings reset to defaults"));
                Task::none()
            }

            // Results
            Message::OperationResult(result) => {
                match result {
                    Ok(msg) => {
                        self.state.set_notification(Notification::success(msg));
                    }
                    Err(msg) => {
                        self.state.set_notification(Notification::error(msg));
                    }
                }
                Task::none()
            }

            Message::Error(msg) => {
                self.state.set_notification(Notification::error(msg));
                Task::none()
            }

            Message::DismissNotification => {
                self.state.clear_notification();
                Task::none()
            }
        }
    }

    /// Build the view
    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let content = self.view_content();

        let main_layout = row![sidebar, content].height(Length::Fill);

        // Add notification if present
        let with_notification = if let Some(ref notif) = self.state.notification {
            let notification = self.view_notification(notif);
            column![main_layout, notification]
        } else {
            column![main_layout]
        };

        container(with_notification)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(colors::BG_BASE.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render the sidebar with glossy glass effect
    fn view_sidebar(&self) -> Element<'_, Message> {
        let width = if self.state.sidebar_expanded {
            Length::Fixed(200.0)
        } else {
            Length::Fixed(68.0)
        };

        let nav_items = [
            (View::Dashboard, "Dashboard", colors::ACCENT_CYAN),
            (View::FanControl, "Fan Control", colors::ACCENT_PURPLE),
            (View::PowerControl, "Power", colors::ACCENT_GREEN),
            (View::ThermalControl, "Thermal", colors::ACCENT_ORANGE),
            (View::Profiles, "Profiles", colors::ACCENT_MAGENTA),
        ];

        let nav_buttons: Vec<Element<'_, Message>> = nav_items
            .iter()
            .map(|(view, label, color)| self.view_nav_button(*view, label, *color))
            .collect();

        let nav_column = Column::with_children(nav_buttons).spacing(spacing::SM);

        let settings_button = self.view_nav_button(View::Settings, "Settings", colors::ACCENT_SKY);

        let content = column![
            nav_column,
            Space::with_height(Length::Fill),
            settings_button,
        ]
        .spacing(spacing::SM)
        .padding(spacing::MD)
        .height(Length::Fill);

        container(content)
            .width(width)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(colors::BG_SURFACE.into()),
                border: iced::Border {
                    color: colors::GLASS_BORDER,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Render a colorful glossy navigation button
    fn view_nav_button<'a>(
        &'a self,
        view: View,
        label: &'a str,
        accent: iced::Color,
    ) -> Element<'a, Message> {
        let is_active = self.state.current_view == view;

        let style = move |_theme: &Theme, status: button::Status| {
            if is_active {
                button::Style {
                    background: Some(colors::with_alpha(accent, 0.15).into()),
                    text_color: accent,
                    border: iced::Border {
                        color: colors::with_alpha(accent, 0.4),
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: colors::with_alpha(accent, 0.1),
                        offset: iced::Vector::new(0.0, 2.0),
                        blur_radius: 8.0,
                    },
                }
            } else {
                let bg = match status {
                    button::Status::Hovered => colors::with_alpha(accent, 0.08),
                    _ => colors::BG_SURFACE,
                };
                let text_col = match status {
                    button::Status::Hovered => colors::lerp(colors::TEXT_SECONDARY, accent, 0.5),
                    _ => colors::TEXT_SECONDARY,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: text_col,
                    border: iced::Border {
                        color: colors::with_alpha(accent, 0.0),
                        width: 0.0,
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                }
            }
        };

        let label_text: Element<'_, Message> = if self.state.sidebar_expanded {
            text(label).size(font_size::BASE).into()
        } else {
            // Just show first letter when collapsed
            text(&label[..1]).size(font_size::LG).into()
        };

        button(label_text)
            .on_press(Message::ViewChanged(view))
            .padding([spacing::SM, spacing::MD])
            .width(Length::Fill)
            .style(style)
            .into()
    }

    /// Render the main content area
    fn view_content(&self) -> Element<'_, Message> {
        let content = match self.state.current_view {
            View::Dashboard => views::view_dashboard(&self.state),
            View::FanControl => views::view_fan_control(&self.state, &self.config),
            View::PowerControl => views::view_power_control(&self.state),
            View::ThermalControl => views::view_thermal_control(&self.state),
            View::Profiles => views::view_profiles(&self.state),
            View::Settings => views::view_settings(&self.state),
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(colors::BG_BASE.into()),
                ..Default::default()
            })
            .into()
    }

    /// Render glossy notification toast
    fn view_notification<'a>(&'a self, notif: &'a Notification) -> Element<'a, Message> {
        use crate::state::NotificationLevel;

        let color = match notif.level {
            NotificationLevel::Success => colors::ACCENT_GREEN,
            NotificationLevel::Warning => colors::ACCENT_ORANGE,
            NotificationLevel::Error => colors::ACCENT_RED,
        };

        let dismiss_btn = button(text("x").size(font_size::SM))
            .on_press(Message::DismissNotification)
            .padding(spacing::XS)
            .style(move |_theme: &Theme, status| {
                let text_col = match status {
                    button::Status::Hovered => color,
                    _ => colors::TEXT_SECONDARY,
                };
                button::Style {
                    background: None,
                    text_color: text_col,
                    ..Default::default()
                }
            });

        let content = row![
            text(&notif.message)
                .size(font_size::BASE)
                .color(colors::TEXT_PRIMARY),
            horizontal_space(),
            dismiss_btn,
        ]
        .align_y(Alignment::Center)
        .spacing(spacing::SM);

        container(content)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(colors::BG_SURFACE.into()),
                border: iced::Border {
                    color: colors::with_alpha(color, 0.6),
                    width: 1.5,
                    radius: 12.0.into(),
                },
                shadow: iced::Shadow {
                    color: colors::with_alpha(color, 0.15),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 16.0,
                },
                ..Default::default()
            })
            .into()
    }

    /// Get theme
    pub fn theme(&self) -> Theme {
        nvctl_theme()
    }

    /// Get title
    pub fn title(&self) -> String {
        String::from("nvctl - GPU Control")
    }

    /// Set up subscriptions
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // Poll GPU state every second
            iced::time::every(Duration::from_secs(1)).map(Message::Tick),
            // Listen for keyboard shortcuts
            keyboard::on_key_press(handle_keyboard_shortcut),
        ])
    }
}

/// Handle keyboard shortcuts
fn handle_keyboard_shortcut(key: Key, modifiers: Modifiers) -> Option<Message> {
    // Navigation with Ctrl+number
    if modifiers.control() {
        if let Key::Character(c) = &key {
            match c.as_str() {
                "1" => return Some(Message::KeyPressed(KeyboardShortcut::GotoDashboard)),
                "2" => return Some(Message::KeyPressed(KeyboardShortcut::GotoFanControl)),
                "3" => return Some(Message::KeyPressed(KeyboardShortcut::GotoPower)),
                "4" => return Some(Message::KeyPressed(KeyboardShortcut::GotoThermal)),
                "5" => return Some(Message::KeyPressed(KeyboardShortcut::GotoProfiles)),
                "," => return Some(Message::KeyPressed(KeyboardShortcut::GotoSettings)),
                "b" | "B" => return Some(Message::KeyPressed(KeyboardShortcut::ToggleSidebar)),
                _ => {}
            }
        }
    }

    // F5 for refresh
    if let Key::Named(Named::F5) = key {
        return Some(Message::KeyPressed(KeyboardShortcut::Refresh));
    }

    None
}

// Manual Default impl needed because GpuMonitor::new() is used instead of Default
#[allow(clippy::derivable_impls)]
impl Default for NvctlGui {
    fn default() -> Self {
        Self {
            state: AppState::default(),
            monitor: GpuMonitor::new(),
            profile_service: ProfileService::new(),
            config: GuiConfig::default(),
            tray: None,
            curve_daemon: Arc::new(RwLock::new(CurveDaemon::new())),
        }
    }
}
