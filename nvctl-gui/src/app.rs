//! Main application structure
//!
//! Implements the Elm Architecture (TEA) pattern for nvctl-gui.

use crate::message::{KeyboardShortcut, Message, SettingsMessage, View};
use crate::services::{
    start_tray, CurveDaemon, GpuMonitor, GpuSettings, GuiConfig, Profile, ProfileService,
    TrayHandle,
};
use crate::state::{AppState, Notification, PendingAction};
use crate::theme::{colors, font_size, nvctl_theme, spacing};
use crate::views;

use iced::keyboard::{self, key::Named, Key, Modifiers};
use iced::widget::{button, column, container, horizontal_space, row, text, Column, Space};
use iced::{alignment, Alignment, Element, Length, Subscription, Task, Theme};
use nvctl::domain::{FanCurve, FanCurvePoint, FanPolicy, FanSpeed};
use std::sync::{Arc, RwLock};
use std::time::Duration;

type CurveUpdate = (u32, String, usize, usize, FanCurve, bool);

const CURVE_MIN_TEMP_C: i32 = 20;
const CURVE_MAX_TEMP_C: i32 = 100;

fn move_curve_point(
    curve: &FanCurve,
    index: usize,
    temperature: i32,
    speed: u8,
) -> Result<FanCurve, String> {
    let mut points = curve.points().to_vec();
    if index >= points.len() {
        return Err("Curve point is no longer available".to_string());
    }

    // Do not let a dragged point cross its neighbours. Apart from preserving the
    // curve invariants, this keeps the canvas' drag index attached to the same point.
    let min_temperature = points
        .get(index.wrapping_sub(1))
        .filter(|_| index > 0)
        .map_or(CURVE_MIN_TEMP_C, |point| {
            point.temperature.saturating_add(1).max(CURVE_MIN_TEMP_C)
        });
    let max_temperature = points.get(index + 1).map_or(CURVE_MAX_TEMP_C, |point| {
        point.temperature.saturating_sub(1).min(CURVE_MAX_TEMP_C)
    });
    if min_temperature > max_temperature {
        return Err("Neighbouring points leave no editable temperature range".to_string());
    }

    let min_speed = if index == 0 {
        curve.default_speed().as_percentage()
    } else {
        points[index - 1].speed.as_percentage()
    };
    let max_speed = points
        .get(index + 1)
        .map_or(FanSpeed::MAX, |point| point.speed.as_percentage());

    points[index] = FanCurvePoint::new(
        temperature.clamp(min_temperature, max_temperature),
        FanSpeed::from_percentage_clamped(speed.clamp(min_speed, max_speed)),
    );

    FanCurve::new(points, curve.default_speed()).map_err(|error| error.to_string())
}

fn add_curve_point(
    curve: &FanCurve,
    temperature: i32,
    speed: u8,
) -> Result<(FanCurve, i32, u8), String> {
    let temperature = temperature.clamp(CURVE_MIN_TEMP_C, CURVE_MAX_TEMP_C);
    let mut points = curve.points().to_vec();
    let insertion_index = points.partition_point(|point| point.temperature < temperature);

    if points
        .get(insertion_index)
        .is_some_and(|point| point.temperature == temperature)
    {
        return Err(format!("A curve point already exists at {temperature}°C"));
    }

    let min_speed = if insertion_index == 0 {
        curve.default_speed().as_percentage()
    } else {
        points[insertion_index - 1].speed.as_percentage()
    };
    let max_speed = points
        .get(insertion_index)
        .map_or(FanSpeed::MAX, |point| point.speed.as_percentage());
    let speed = speed.clamp(min_speed, max_speed);
    points.insert(
        insertion_index,
        FanCurvePoint::new(temperature, FanSpeed::from_percentage_clamped(speed)),
    );

    FanCurve::new(points, curve.default_speed())
        .map(|curve| (curve, temperature, speed))
        .map_err(|error| error.to_string())
}

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

    /// Main window identifier, used for tray hide/show behavior.
    window_id: Option<iced::window::Id>,

    /// GUI configuration (fan labels, preferences)
    config: GuiConfig,

    /// False when startup found a malformed config that must not be overwritten
    config_writable: bool,

    /// Fan curve daemon for automatic fan control
    curve_daemon: Arc<RwLock<CurveDaemon>>,
}

impl NvctlGui {
    /// Create a new application instance
    pub fn new() -> (Self, Task<Message>) {
        let monitor = GpuMonitor::new();
        let gpus = monitor.detect_gpus();
        let profile_service = ProfileService::new();

        // Never overwrite a malformed user config with defaults.
        let (mut config, config_load_error) = match GuiConfig::load() {
            Ok(config) => (config, None),
            Err(error) => (GuiConfig::default(), Some(error)),
        };
        let config_writable = config_load_error.is_none();

        // Detect cooler targets for each GPU and store in config
        for gpu in &gpus {
            let targets = match monitor.get_cooler_targets(gpu.index) {
                Ok(targets) => targets,
                Err(error) => {
                    log::warn!("Could not detect cooler targets: {}", error);
                    continue;
                }
            };
            let total_fans = targets.len() as u32;
            let gpu_config = config.get_gpu_fan_config(&gpu.info.uuid);

            for (idx, target) in targets.into_iter().enumerate() {
                gpu_config.set_detected_target(idx as u32, target, total_fans);
            }
        }

        // Save updated config with detected targets
        if config_load_error.is_none() {
            if let Err(e) = config.save() {
                log::warn!("Failed to save config: {}", e);
            }
        }

        let mut state = AppState::new();
        state.gpus = gpus;
        state.sidebar_expanded = config.preferences.sidebar_expanded;

        // Initialize fan curves for the first GPU
        if let Some(gpu) = state.gpus.first() {
            state.init_curves_for_gpu(gpu.fan_speeds.len());
        }

        // Load profiles into state
        state.profiles = profile_service.list().into_iter().cloned().collect();
        state.active_profile = None;
        let startup_profile = profile_service
            .default_profile()
            .map(|profile| profile.name.clone());

        if let Some(error) = config_load_error {
            state.set_notification(Notification::error(format!(
                "GUI config was not loaded and was left unchanged: {}",
                error
            )));
        } else if !monitor.is_available() {
            state.set_notification(Notification::warning(
                "NVML is not available; GPU telemetry and controls are disabled.",
            ));
        } else if state.gpus.is_empty() {
            state.set_notification(Notification::error(
                "No GPUs with complete telemetry were detected",
            ));
        }

        // Start system tray
        let tray = config.preferences.enable_tray.then(start_tray).flatten();
        if tray.is_none() {
            log::info!("System tray not available on this system");
        }

        // Restore only explicitly persisted curve controls.
        let mut daemon = CurveDaemon::new();
        {
            let daemon_state = daemon.state();
            if let Ok(mut daemon_state) = daemon_state.write() {
                for curve_config in &config.curve_control {
                    let Some(gpu) = state
                        .gpus
                        .iter()
                        .find(|gpu| gpu.info.uuid == curve_config.gpu_uuid)
                    else {
                        log::warn!(
                            "Ignoring curve for unavailable GPU {}",
                            curve_config.gpu_uuid
                        );
                        continue;
                    };
                    let gpu_index = gpu.index;
                    let fan_count = gpu.fan_speeds.len();
                    if curve_config.fan_index as usize >= fan_count {
                        log::warn!(
                            "Ignoring out-of-range fan {} for GPU {}",
                            curve_config.fan_index,
                            curve_config.gpu_uuid
                        );
                        continue;
                    }

                    state.set_gpu_editor_curve(
                        gpu_index,
                        fan_count,
                        curve_config.fan_index as usize,
                        curve_config.curve.clone(),
                        curve_config.enabled,
                    );
                    daemon_state.set_curve(
                        curve_config.gpu_uuid.clone(),
                        curve_config.fan_index,
                        curve_config.curve.clone(),
                        curve_config.enabled,
                    );
                }
            };
        }
        if daemon
            .state()
            .read()
            .map(|state| state.has_enabled_curves())
            .unwrap_or(false)
        {
            daemon.start();
        }
        let curve_daemon = Arc::new(RwLock::new(daemon));

        let app = Self {
            state,
            monitor,
            profile_service,
            tray,
            window_id: None,
            config,
            config_writable,
            curve_daemon,
        };

        let startup_task = startup_profile
            .map(|name| {
                Task::done(Message::Profile(crate::message::ProfileMessage::Selected(
                    name,
                )))
            })
            .unwrap_or_else(Task::none);

        (app, startup_task)
    }

    fn save_gui_config_candidate(&self, candidate: &GuiConfig) -> Result<(), String> {
        if !self.config_writable {
            return Err(
                "the existing GUI config is invalid; fix or move it before saving settings"
                    .to_string(),
            );
        }
        candidate.save()
    }

    /// Persist and install curve changes as one operation. Both the in-memory
    /// daemon state and the config file stay unchanged if persistence fails.
    fn apply_curve_updates(&mut self, updates: &[CurveUpdate]) -> Result<(), String> {
        if updates.is_empty() {
            return Ok(());
        }
        if !self.config_writable {
            return Err(
                "the existing GUI config is invalid; fix or move it before saving settings"
                    .to_string(),
            );
        }

        let mut next_config = self.config.clone();
        for (_, gpu_uuid, _, fan_index, curve, enabled) in updates {
            next_config.set_curve_control(gpu_uuid, *fan_index as u32, curve.clone(), *enabled);
        }

        let mut daemon = self
            .curve_daemon
            .write()
            .map_err(|_| "Fan curve daemon lock is poisoned".to_string())?;
        let daemon_state = daemon.state();
        let has_enabled = {
            let mut state = daemon_state
                .write()
                .map_err(|_| "Fan curve state lock is poisoned".to_string())?;
            let previous_state = state.clone();
            for (_, gpu_uuid, _, fan_index, curve, enabled) in updates {
                state.set_curve(gpu_uuid.clone(), *fan_index as u32, curve.clone(), *enabled);
            }
            if let Err(error) = next_config.save() {
                *state = previous_state;
                return Err(error);
            }
            state.has_enabled_curves()
        };

        self.config = next_config;
        if has_enabled && !daemon.is_running() {
            daemon.start();
            log::info!("Fan curve daemon started");
        } else if !has_enabled && daemon.is_running() {
            daemon.stop();
            log::info!("Fan curve daemon stopped");
        }
        drop(daemon);

        for (gpu_index, _, fan_count, fan_index, curve, enabled) in updates {
            self.state.set_gpu_editor_curve(
                *gpu_index,
                *fan_count,
                *fan_index,
                curve.clone(),
                *enabled,
            );
        }
        Ok(())
    }

    fn toggle_sidebar(&mut self) {
        self.state.sidebar_expanded = !self.state.sidebar_expanded;
        let mut next_config = self.config.clone();
        next_config.preferences.sidebar_expanded = self.state.sidebar_expanded;
        match self.save_gui_config_candidate(&next_config) {
            Ok(()) => self.config = next_config,
            Err(error) => {
                log::warn!("Failed to persist sidebar preference: {}", error);
                self.state.set_notification(Notification::warning(format!(
                    "Sidebar changed for this session but could not be saved: {}",
                    error
                )));
            }
        }
    }

    fn apply_profile(&mut self, name: &str) -> Result<(), String> {
        let profile = self
            .profile_service
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Profile '{}' no longer exists", name))?;
        let gpu_indices = self.state.target_gpu_indices();
        if gpu_indices.is_empty() {
            return Err("No GPU selected".to_string());
        }

        let mut targets = Vec::new();
        for gpu in self
            .state
            .gpus
            .iter()
            .filter(|gpu| gpu_indices.contains(&gpu.index))
        {
            let settings = profile
                .gpu_settings
                .get(&gpu.info.uuid)
                .or_else(|| profile.gpu_settings.get("default"))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Profile '{}' has no settings for GPU {}",
                        name, gpu.info.uuid
                    )
                })?;

            if settings.apply_fan_curve {
                if gpu.fan_speeds.is_empty() {
                    return Err(format!(
                        "Profile '{}' enables fan curves but GPU {} exposes no controllable fans",
                        name, gpu.info.uuid
                    ));
                }
                if settings.fan_curve.is_none() && settings.fan_curves.is_empty() {
                    return Err(format!(
                        "Profile '{}' enables fan curves but contains none",
                        name
                    ));
                }
                if !settings.fan_curves.is_empty()
                    && settings.fan_curves.len() != gpu.fan_speeds.len()
                {
                    return Err(format!(
                        "Profile '{}' has {} fan curves for GPU {} but the GPU exposes {} fans",
                        name,
                        settings.fan_curves.len(),
                        gpu.info.uuid,
                        gpu.fan_speeds.len()
                    ));
                }
            }
            if settings.apply_power_limit {
                let limit = settings.power_limit.ok_or_else(|| {
                    format!("Profile '{}' enables a power limit but contains none", name)
                })?;
                let constraints = gpu.power_constraints.ok_or_else(|| {
                    format!("GPU {} does not expose power constraints", gpu.info.uuid)
                })?;
                if !constraints.contains(&limit) {
                    return Err(format!(
                        "Profile power limit {} is outside GPU {} range {}",
                        limit, gpu.info.uuid, constraints
                    ));
                }
            }

            let old_acoustic_limit = if let Some(target) = settings.acoustic_limit {
                let limits = self
                    .monitor
                    .acoustic_limits_by_uuid(&gpu.info.uuid)?
                    .ok_or_else(|| {
                        format!("GPU {} does not support acoustic limits", gpu.info.uuid)
                    })?;
                let (Some(current), Some(min), Some(max)) =
                    (limits.current, limits.min, limits.max)
                else {
                    return Err(format!(
                        "GPU {} does not expose complete acoustic limits",
                        gpu.info.uuid
                    ));
                };
                if target < min || target > max {
                    return Err(format!(
                        "Profile acoustic limit {} is outside GPU {} range {}-{}",
                        target, gpu.info.uuid, min, max
                    ));
                }
                Some(current)
            } else {
                None
            };

            targets.push((
                gpu.index,
                gpu.info.uuid.clone(),
                gpu.fan_speeds.len(),
                gpu.power_limit,
                old_acoustic_limit,
                settings,
            ));
        }

        // Power changes are rolled back if a later selected GPU fails.
        let mut applied_power: Vec<(String, nvctl::domain::PowerLimit)> = Vec::new();
        for (_, gpu_uuid, _, old_limit, _, settings) in &targets {
            if settings.apply_power_limit {
                if let Some(limit) = settings.power_limit {
                    if let Err(error) = self.monitor.set_power_limit_by_uuid(gpu_uuid, limit) {
                        for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                            if let Err(rollback_error) = self
                                .monitor
                                .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                            {
                                log::error!(
                                    "Failed to roll back GPU {} profile power limit: {}",
                                    rollback_uuid,
                                    rollback_error
                                );
                            }
                        }
                        return Err(format!("Failed to apply profile power limit: {}", error));
                    }
                    applied_power.push((gpu_uuid.clone(), *old_limit));
                }
            }
        }

        // Acoustic changes are also transactional and are restored before
        // power if any later setting cannot be applied.
        let mut applied_acoustic: Vec<(String, nvctl::domain::Temperature)> = Vec::new();
        for (_, gpu_uuid, _, _, old_acoustic_limit, settings) in &targets {
            if let Some(limit) = settings.acoustic_limit {
                if let Err(error) = self.monitor.set_acoustic_limit_by_uuid(gpu_uuid, limit) {
                    for (rollback_uuid, rollback_limit) in applied_acoustic.into_iter().rev() {
                        if let Err(rollback_error) = self
                            .monitor
                            .set_acoustic_limit_by_uuid(&rollback_uuid, rollback_limit)
                        {
                            log::error!(
                                "Failed to roll back GPU {} acoustic limit: {}",
                                rollback_uuid,
                                rollback_error
                            );
                        }
                    }
                    for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                        if let Err(rollback_error) = self
                            .monitor
                            .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                        {
                            log::error!(
                                "Failed to roll back GPU {} profile power limit: {}",
                                rollback_uuid,
                                rollback_error
                            );
                        }
                    }
                    return Err(format!("Failed to apply profile acoustic limit: {}", error));
                }
                let Some(rollback_limit) = *old_acoustic_limit else {
                    return Err(format!(
                        "GPU {} lost its acoustic rollback value during profile validation",
                        gpu_uuid
                    ));
                };
                applied_acoustic.push((gpu_uuid.clone(), rollback_limit));
            }
        }

        let mut curve_updates = Vec::new();
        for (gpu_index, gpu_uuid, fan_count, _, _, settings) in &targets {
            if settings.apply_fan_curve {
                for (fan_index, (curve, enabled)) in settings
                    .resolved_fan_curves(*fan_count)
                    .into_iter()
                    .enumerate()
                {
                    curve_updates.push((
                        *gpu_index,
                        gpu_uuid.clone(),
                        *fan_count,
                        fan_index,
                        curve,
                        enabled,
                    ));
                }
            }
        }
        if let Err(error) = self.apply_curve_updates(&curve_updates) {
            for (rollback_uuid, rollback_limit) in applied_acoustic.into_iter().rev() {
                if let Err(rollback_error) = self
                    .monitor
                    .set_acoustic_limit_by_uuid(&rollback_uuid, rollback_limit)
                {
                    log::error!(
                        "Failed to roll back GPU {} acoustic limit after profile error: {}",
                        rollback_uuid,
                        rollback_error
                    );
                }
            }
            for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                if let Err(rollback_error) = self
                    .monitor
                    .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                {
                    log::error!(
                        "Failed to roll back GPU {} power after profile error: {}",
                        rollback_uuid,
                        rollback_error
                    );
                }
            }
            return Err(format!("Failed to apply profile fan curves: {}", error));
        }

        for (gpu_index, gpu_uuid, _, _, _, settings) in targets {
            if settings.apply_power_limit {
                if let Some(limit) = settings.power_limit {
                    if let Some(gpu) = self
                        .state
                        .gpus
                        .iter_mut()
                        .find(|gpu| gpu.info.uuid == gpu_uuid)
                    {
                        gpu.power_limit = limit;
                    }
                }
                self.state.set_gpu_pending_power(gpu_index, None);
            }
        }

        self.profile_service.set_active(Some(name.to_string()));
        self.state.active_profile = Some(name.to_string());
        Ok(())
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
                self.toggle_sidebar();
                Task::none()
            }

            Message::KeyPressed(shortcut) => {
                // Handle keyboard shortcuts
                if let Some(view) = shortcut.to_view() {
                    self.state.current_view = view;
                } else {
                    match shortcut {
                        KeyboardShortcut::Refresh => {
                            self.state
                                .replace_gpus_preserving_selection(self.monitor.detect_gpus());
                            self.state
                                .set_notification(Notification::success("GPU data refreshed"));
                        }
                        KeyboardShortcut::ToggleSidebar => {
                            self.toggle_sidebar();
                        }
                        _ => {}
                    }
                }
                Task::none()
            }

            // GPU selection
            Message::GpuSelected(index) => {
                if index < self.state.gpus.len() {
                    self.state.select_gpu(index);
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
                self.state
                    .replace_gpus_preserving_selection(self.monitor.detect_gpus());
                Task::none()
            }

            // Polling
            Message::WindowOpened(window_id) => {
                self.window_id = Some(window_id);
                if self.config.preferences.start_minimized
                    && self.tray.as_ref().is_some_and(TrayHandle::is_running)
                {
                    return iced::window::change_mode(window_id, iced::window::Mode::Hidden);
                }
                Task::none()
            }

            Message::Tick(_now) => {
                // Poll by UUID so an NVML re-enumeration cannot attach one
                // physical GPU's telemetry to another GPU's state.
                let gpu_uuids: Vec<String> = self
                    .state
                    .gpus
                    .iter()
                    .map(|gpu| gpu.info.uuid.clone())
                    .collect();

                // Poll all GPUs
                for gpu_uuid in gpu_uuids {
                    match self.monitor.poll_gpu_by_uuid(&gpu_uuid) {
                        Ok(snapshot) => self.state.update_gpu_by_uuid(&gpu_uuid, snapshot),
                        Err(error) => {
                            // Preserve the previous valid snapshot; never replace it with zeroes.
                            log::warn!("{}", error);
                        }
                    }
                }

                // A panic in the background curve worker must not silently
                // leave fans fixed at their last manual speed.
                let curve_restarted = match self.curve_daemon.write() {
                    Ok(mut daemon) => {
                        let daemon_state = daemon.state();
                        let has_enabled_curves = match daemon_state.read() {
                            Ok(state) => state.has_enabled_curves(),
                            Err(poisoned) => {
                                log::error!("Fan curve state lock is poisoned; recovering it");
                                poisoned.into_inner().has_enabled_curves()
                            }
                        };
                        if has_enabled_curves && !daemon.is_running() {
                            daemon.start();
                            true
                        } else {
                            false
                        }
                    }
                    Err(_) => {
                        log::error!("Fan curve daemon lock is poisoned");
                        false
                    }
                };
                if curve_restarted {
                    log::error!("Fan curve daemon stopped unexpectedly and was restarted");
                    if !self
                        .state
                        .notification
                        .as_ref()
                        .is_some_and(|notification| {
                            notification.level == crate::state::NotificationLevel::Error
                        })
                    {
                        self.state.set_notification(Notification::warning(
                            "Fan curve control stopped unexpectedly and was restarted",
                        ));
                    }
                }

                // Never leave the only window hidden if the tray service exits.
                if self.tray.as_ref().is_some_and(|tray| !tray.is_running()) {
                    log::warn!("System tray service stopped; restoring the main window");
                    self.tray = None;
                    self.state.set_notification(Notification::warning(
                        "System tray stopped; the main window was restored",
                    ));
                    if let Some(window_id) = self.window_id {
                        return Task::batch([
                            iced::window::change_mode(window_id, iced::window::Mode::Windowed),
                            iced::window::minimize(window_id, false),
                        ]);
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
                    if tray.take_show_window_request() {
                        if let Some(window_id) = self.window_id {
                            return Task::batch([
                                iced::window::change_mode(window_id, iced::window::Mode::Windowed),
                                iced::window::minimize(window_id, false),
                            ]);
                        }
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
                        let gpu_indices = self.state.target_gpu_indices();
                        let mut targets = Vec::new();
                        for gpu in self
                            .state
                            .gpus
                            .iter()
                            .filter(|gpu| gpu_indices.contains(&gpu.index))
                        {
                            if fan_idx as usize >= gpu.fan_speeds.len() {
                                self.state.set_notification(Notification::error(format!(
                                    "Fan {} is out of range for GPU {}",
                                    fan_idx + 1,
                                    gpu.info.uuid
                                )));
                                return Task::none();
                            }
                            let Some(old_policy) =
                                gpu.fan_policies.get(fan_idx as usize).copied().flatten()
                            else {
                                self.state.set_notification(Notification::error(format!(
                                    "Fan policy control is unavailable for GPU {} Fan {}",
                                    gpu.info.uuid,
                                    fan_idx + 1
                                )));
                                return Task::none();
                            };
                            targets.push((
                                gpu.info.uuid.clone(),
                                old_policy,
                                gpu.fan_speeds[fan_idx as usize],
                            ));
                        }
                        if targets.is_empty() {
                            self.state
                                .set_notification(Notification::error("No GPU selected"));
                            return Task::none();
                        }

                        let mut applied: Vec<(String, FanPolicy, FanSpeed)> = Vec::new();
                        for (gpu_uuid, old_policy, old_speed) in &targets {
                            if let Err(error) = self
                                .monitor
                                .set_fan_policy_by_uuid(gpu_uuid, fan_idx, policy)
                            {
                                for (rollback_uuid, rollback_policy, rollback_speed) in
                                    applied.into_iter().rev()
                                {
                                    if let Err(rollback_error) =
                                        self.monitor.set_fan_policy_by_uuid(
                                            &rollback_uuid,
                                            fan_idx,
                                            rollback_policy,
                                        )
                                    {
                                        log::error!(
                                            "Failed to roll back GPU {} Fan {} policy: {}",
                                            rollback_uuid,
                                            fan_idx,
                                            rollback_error
                                        );
                                    }
                                    if rollback_policy == FanPolicy::Manual {
                                        if let Err(rollback_error) =
                                            self.monitor.set_fan_speed_by_uuid(
                                                &rollback_uuid,
                                                fan_idx,
                                                rollback_speed,
                                            )
                                        {
                                            log::error!(
                                                "Failed to roll back GPU {} Fan {} speed: {}",
                                                rollback_uuid,
                                                fan_idx,
                                                rollback_error
                                            );
                                        }
                                    }
                                }
                                self.state.set_notification(Notification::error(error));
                                return Task::none();
                            }
                            applied.push((gpu_uuid.clone(), *old_policy, *old_speed));
                        }
                        for (gpu_uuid, _, _) in targets {
                            if let Some(fan_policy) = self
                                .state
                                .gpus
                                .iter_mut()
                                .find(|gpu| gpu.info.uuid == gpu_uuid)
                                .and_then(|gpu| gpu.fan_policies.get_mut(fan_idx as usize))
                            {
                                *fan_policy = Some(policy);
                            }
                        }

                        self.state.set_notification(Notification::success(format!(
                            "Fan {} policy set to {:?}",
                            fan_idx + 1,
                            policy
                        )));
                    }
                    FanControlMessage::SpeedChanged(fan_idx, speed) => {
                        let gpu_indices = self.state.target_gpu_indices();
                        let mut targets = Vec::new();
                        for gpu in self
                            .state
                            .gpus
                            .iter()
                            .filter(|gpu| gpu_indices.contains(&gpu.index))
                        {
                            let Some(old_speed) = gpu.fan_speeds.get(fan_idx as usize) else {
                                self.state.set_notification(Notification::error(format!(
                                    "Fan {} is out of range for GPU {}",
                                    fan_idx + 1,
                                    gpu.info.uuid
                                )));
                                return Task::none();
                            };
                            let Some(old_policy) =
                                gpu.fan_policies.get(fan_idx as usize).copied().flatten()
                            else {
                                self.state.set_notification(Notification::error(format!(
                                    "Fan policy control is unavailable for GPU {} Fan {}",
                                    gpu.info.uuid,
                                    fan_idx + 1
                                )));
                                return Task::none();
                            };
                            targets.push((gpu.info.uuid.clone(), old_policy, *old_speed));
                        }
                        if targets.is_empty() {
                            self.state
                                .set_notification(Notification::error("No GPU selected"));
                            return Task::none();
                        }

                        let mut applied: Vec<(String, FanPolicy, FanSpeed)> = Vec::new();
                        for (gpu_uuid, old_policy, old_speed) in &targets {
                            if let Err(error) =
                                self.monitor.set_fan_speed_by_uuid(gpu_uuid, fan_idx, speed)
                            {
                                for (rollback_uuid, rollback_policy, rollback_speed) in
                                    applied.into_iter().rev()
                                {
                                    if rollback_policy == FanPolicy::Auto {
                                        if let Err(rollback_error) =
                                            self.monitor.set_fan_speed_by_uuid(
                                                &rollback_uuid,
                                                fan_idx,
                                                rollback_speed,
                                            )
                                        {
                                            log::error!(
                                                "Failed to roll back GPU {} Fan {} speed: {}",
                                                rollback_uuid,
                                                fan_idx,
                                                rollback_error
                                            );
                                        }
                                    }
                                    if let Err(rollback_error) =
                                        self.monitor.set_fan_policy_by_uuid(
                                            &rollback_uuid,
                                            fan_idx,
                                            rollback_policy,
                                        )
                                    {
                                        log::error!(
                                            "Failed to roll back GPU {} Fan {} policy: {}",
                                            rollback_uuid,
                                            fan_idx,
                                            rollback_error
                                        );
                                    }
                                    if rollback_policy == FanPolicy::Manual {
                                        if let Err(rollback_error) =
                                            self.monitor.set_fan_speed_by_uuid(
                                                &rollback_uuid,
                                                fan_idx,
                                                rollback_speed,
                                            )
                                        {
                                            log::error!(
                                                "Failed to roll back GPU {} Fan {} speed: {}",
                                                rollback_uuid,
                                                fan_idx,
                                                rollback_error
                                            );
                                        }
                                    }
                                }
                                self.state.set_notification(Notification::error(error));
                                return Task::none();
                            }
                            applied.push((gpu_uuid.clone(), *old_policy, *old_speed));
                        }
                        for (gpu_uuid, _, _) in targets {
                            if let Some(gpu) = self
                                .state
                                .gpus
                                .iter_mut()
                                .find(|gpu| gpu.info.uuid == gpu_uuid)
                            {
                                if let Some(fan_speed) = gpu.fan_speeds.get_mut(fan_idx as usize) {
                                    *fan_speed = speed;
                                }
                                if let Some(fan_policy) = gpu.fan_policies.get_mut(fan_idx as usize)
                                {
                                    *fan_policy = Some(FanPolicy::Manual);
                                }
                            }
                        }
                        self.state.set_notification(Notification::success(format!(
                            "Fan {} speed set to {}%",
                            fan_idx + 1,
                            speed.as_percentage()
                        )));
                    }
                    FanControlMessage::SelectCurveFan(fan_idx) => {
                        if fan_idx < self.state.editing_curves.len() {
                            self.state.selected_curve_fan = fan_idx;
                        } else {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                        }
                    }
                    FanControlMessage::CurvePointMoved { index, temp, speed } => {
                        let fan_idx = self.state.selected_curve_fan;
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

                        if fan_idx >= self.state.editing_curves.len() {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                            return Task::none();
                        }

                        match move_curve_point(&current_curve, index, temp, speed) {
                            Ok(new_curve) => {
                                self.state.editing_curves[fan_idx] = Some(new_curve);
                            }
                            Err(error) => {
                                self.state.set_notification(Notification::error(error));
                            }
                        }
                    }
                    FanControlMessage::CurvePointAdded(temp, speed) => {
                        let fan_idx = self.state.selected_curve_fan;
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

                        if fan_idx >= self.state.editing_curves.len() {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                            return Task::none();
                        }

                        match add_curve_point(&current_curve, temp, speed) {
                            Ok((new_curve, actual_temp, actual_speed)) => {
                                self.state.editing_curves[fan_idx] = Some(new_curve);
                                self.state.set_notification(Notification::success(format!(
                                    "Added point at {actual_temp}°C, {actual_speed}%"
                                )));
                            }
                            Err(error) => {
                                self.state.set_notification(Notification::error(error));
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

                        if fan_idx >= self.state.editing_curves.len() {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                            return Task::none();
                        }

                        if points.len() <= 1 {
                            self.state.set_notification(Notification::error(
                                "A fan curve must contain at least one point",
                            ));
                        } else if index >= points.len() {
                            self.state.set_notification(Notification::error(
                                "Curve point is no longer available",
                            ));
                        } else {
                            points.remove(index);

                            match FanCurve::new(points, current_curve.default_speed()) {
                                Ok(new_curve) => {
                                    self.state.editing_curves[fan_idx] = Some(new_curve);
                                    self.state
                                        .set_notification(Notification::success("Point removed"));
                                }
                                Err(error) => {
                                    self.state
                                        .set_notification(Notification::error(error.to_string()));
                                }
                            }
                        }
                    }
                    FanControlMessage::PresetSelected(preset) => {
                        // Apply preset to selected fan's curve
                        let fan_idx = self.state.selected_curve_fan;
                        if fan_idx < self.state.editing_curves.len() {
                            self.state.editing_curves[fan_idx] = Some(preset.to_curve());
                            self.state.set_notification(Notification::success(format!(
                                "Applied {} preset to Fan {}",
                                preset.name(),
                                fan_idx + 1
                            )));
                        } else {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                        }
                    }
                    FanControlMessage::CurveControlToggled(enabled) => {
                        // Toggle curve control for selected fan
                        let fan_idx = self.state.selected_curve_fan;
                        if fan_idx < self.state.curve_control_enabled.len() {
                            self.state.curve_control_enabled[fan_idx] = enabled;
                        } else {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is not available",
                                fan_idx + 1
                            )));
                            return Task::none();
                        }
                        let status = if enabled { "enabled" } else { "disabled" };
                        self.state.set_notification(Notification::success(format!(
                            "Curve control will be {} for Fan {} when applied",
                            status,
                            fan_idx + 1
                        )));
                    }
                    FanControlMessage::ApplyCurve => {
                        let was_confirmed =
                            self.state.confirmed_action.as_ref() == Some(&PendingAction::FanCurve);
                        self.state.confirmed_action = None;
                        if !was_confirmed && self.state.linked_gpus && self.state.gpus.len() > 1 {
                            self.state.request_confirmation(
                                PendingAction::FanCurve,
                                "Apply fan curve to all GPUs",
                                format!(
                                    "This will save and enable the selected fan curve on all {} linked GPUs. Automatic fan control may begin immediately.",
                                    self.state.gpus.len()
                                ),
                            );
                            return Task::none();
                        }
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
                        let target_gpus: Vec<_> = self
                            .state
                            .gpus
                            .iter()
                            .filter(|gpu| gpu_indices.contains(&gpu.index))
                            .map(|gpu| (gpu.index, gpu.info.uuid.clone(), gpu.fan_speeds.len()))
                            .collect();
                        if target_gpus.is_empty() {
                            self.state
                                .set_notification(Notification::error("No GPU selected"));
                            return Task::none();
                        }
                        if let Some((_, uuid, fan_count)) = target_gpus
                            .iter()
                            .find(|(_, _, fan_count)| fan_idx >= *fan_count)
                        {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is out of range for GPU {} ({} fans)",
                                fan_idx + 1,
                                uuid,
                                fan_count
                            )));
                            return Task::none();
                        }

                        let updates: Vec<CurveUpdate> = target_gpus
                            .into_iter()
                            .map(|(gpu_index, uuid, fan_count)| {
                                (gpu_index, uuid, fan_count, fan_idx, curve.clone(), enabled)
                            })
                            .collect();
                        if let Err(error) = self.apply_curve_updates(&updates) {
                            self.state.set_notification(Notification::error(format!(
                                "Failed to apply fan curve: {}",
                                error
                            )));
                            return Task::none();
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
                        self.state.pending_power_limit = Some(limit);
                    }
                    PowerControlMessage::ResetToDefault => {
                        // Reset to default power limit
                        let default_watts = self
                            .state
                            .current_gpu()
                            .and_then(|g| g.power_constraints.as_ref())
                            .map(|c| c.default);

                        if let Some(default) = default_watts {
                            self.state.pending_power_limit = Some(default);
                            self.state.set_notification(Notification::success(format!(
                                "Default power limit selected ({}W); apply to update the GPU",
                                default.as_watts()
                            )));
                        } else {
                            self.state.set_notification(Notification::error(
                                "The selected GPU does not expose a default power limit",
                            ));
                        }
                    }
                    PowerControlMessage::ApplyLimit => {
                        let was_confirmed = self.state.confirmed_action.as_ref()
                            == Some(&PendingAction::PowerLimit);
                        self.state.confirmed_action = None;
                        if !was_confirmed && self.state.linked_gpus && self.state.gpus.len() > 1 {
                            self.state.request_confirmation(
                                PendingAction::PowerLimit,
                                "Apply power limit to all GPUs",
                                format!(
                                    "This will apply the selected power limit to all {} linked GPUs. Each GPU will be validated before the change is made.",
                                    self.state.gpus.len()
                                ),
                            );
                            return Task::none();
                        }
                        // Get current power limit from state
                        let limit = match (self.state.pending_power_limit, self.state.current_gpu())
                        {
                            (Some(limit), _) => limit,
                            (None, Some(gpu)) => gpu.power_limit,
                            (None, None) => return Task::none(),
                        };

                        // Get GPU indices to apply to
                        let gpu_indices = self.state.target_gpu_indices();
                        let mut old_limits = Vec::new();
                        for gpu in self
                            .state
                            .gpus
                            .iter()
                            .filter(|gpu| gpu_indices.contains(&gpu.index))
                        {
                            let Some(constraints) = gpu.power_constraints else {
                                self.state.set_notification(Notification::error(format!(
                                    "GPU {} does not expose power constraints",
                                    gpu.info.uuid
                                )));
                                return Task::none();
                            };
                            if !constraints.contains(&limit) {
                                self.state.set_notification(Notification::error(format!(
                                    "Power limit {} is outside GPU {} range {}",
                                    limit, gpu.info.uuid, constraints
                                )));
                                return Task::none();
                            }
                            old_limits.push((gpu.info.uuid.clone(), gpu.power_limit));
                        }
                        if old_limits.is_empty() {
                            self.state
                                .set_notification(Notification::error("No GPU selected"));
                            return Task::none();
                        }

                        let mut applied: Vec<(String, nvctl::domain::PowerLimit)> = Vec::new();
                        for (gpu_uuid, old_limit) in &old_limits {
                            match self.monitor.set_power_limit_by_uuid(gpu_uuid, limit) {
                                Ok(()) => {
                                    log::info!(
                                        "Power limit set to {}W for GPU {}",
                                        limit.as_watts(),
                                        gpu_uuid
                                    );
                                    applied.push((gpu_uuid.clone(), *old_limit));
                                }
                                Err(e) => {
                                    for (rollback_uuid, rollback_limit) in applied.into_iter().rev()
                                    {
                                        if let Err(rollback_error) = self
                                            .monitor
                                            .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                                        {
                                            log::error!(
                                                "Failed to roll back GPU {} power limit: {}",
                                                rollback_uuid,
                                                rollback_error
                                            );
                                        }
                                    }
                                    self.state.set_notification(Notification::error(e));
                                    return Task::none();
                                }
                            }
                        }

                        for gpu in &mut self.state.gpus {
                            if gpu_indices.contains(&gpu.index) {
                                gpu.power_limit = limit;
                            }
                        }
                        for gpu_index in gpu_indices {
                            self.state.set_gpu_pending_power(gpu_index, None);
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
                        let action = PendingAction::Profile(name.clone());
                        let was_confirmed = self.state.confirmed_action.as_ref() == Some(&action);
                        self.state.confirmed_action = None;
                        if !was_confirmed && self.state.linked_gpus && self.state.gpus.len() > 1 {
                            self.state.request_confirmation(
                                action,
                                "Apply profile to all GPUs",
                                format!(
                                    "Profile '{name}' may change power limits and fan controls on all {} linked GPUs.",
                                    self.state.gpus.len()
                                ),
                            );
                            return Task::none();
                        }
                        match self.apply_profile(&name) {
                            Ok(()) => self.state.set_notification(Notification::success(format!(
                                "Profile '{}' applied",
                                name
                            ))),
                            Err(error) => self.state.set_notification(Notification::error(
                                format!("Failed to apply profile '{}': {}", name, error),
                            )),
                        }
                    }
                    ProfileMessage::SaveCurrent(name) => {
                        if self.state.current_gpu().is_none() {
                            self.state.set_notification(Notification::error(
                                "No GPU is available to save into a profile",
                            ));
                            return Task::none();
                        }
                        // Create profile from current settings
                        let mut profile = Profile::new(&name);

                        // Get current GPU settings
                        if let Some(gpu) = self.state.current_gpu() {
                            let mut settings = GpuSettings::new();

                            let curves: Vec<_> = (0..gpu.fan_speeds.len())
                                .map(|fan_index| {
                                    self.state
                                        .editing_curves
                                        .get(fan_index)
                                        .and_then(|curve| curve.clone())
                                        .or_else(|| gpu.fan_curves.get(fan_index).cloned())
                                        .unwrap_or_default()
                                })
                                .collect();
                            if !curves.is_empty() {
                                let enabled = (0..curves.len())
                                    .map(|fan_index| self.state.is_curve_enabled(fan_index))
                                    .collect();
                                settings = settings.with_fan_curves(curves, enabled);
                            }

                            settings = settings.with_power_limit(
                                self.state.pending_power_limit.unwrap_or(gpu.power_limit),
                            );

                            match self.monitor.acoustic_limits_by_uuid(&gpu.info.uuid) {
                                Ok(Some(limits))
                                    if limits.min.is_some() && limits.max.is_some() =>
                                {
                                    if let Some(current) = limits.current {
                                        settings = settings.with_acoustic_limit(current);
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    self.state.set_notification(Notification::error(format!(
                                        "Failed to read acoustic settings for profile: {}",
                                        error
                                    )));
                                    return Task::none();
                                }
                            }

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
                    ProfileMessage::Refresh => match self.profile_service.load_all() {
                        Ok(()) => {
                            self.state.profiles =
                                self.profile_service.list().into_iter().cloned().collect();
                            if self
                                .state
                                .active_profile
                                .as_ref()
                                .is_some_and(|name| !self.profile_service.exists(name))
                            {
                                self.state.active_profile = None;
                            }
                        }
                        Err(e) => {
                            self.state.set_notification(Notification::error(format!(
                                "Failed to refresh profiles: {}",
                                e
                            )));
                        }
                    },
                }
                Task::none()
            }

            // Preferences
            Message::Settings(msg) => {
                let mut next_config = self.config.clone();
                let message = match msg {
                    SettingsMessage::PollInterval(interval) => {
                        next_config.preferences.poll_interval_ms = interval;
                        format!("Telemetry update rate set to {} ms", interval)
                    }
                    SettingsMessage::TrayEnabled(enabled) => {
                        next_config.preferences.enable_tray = enabled;
                        format!(
                            "System tray will be {} after restarting nvctl",
                            if enabled { "enabled" } else { "disabled" }
                        )
                    }
                    SettingsMessage::StartMinimized(enabled) => {
                        next_config.preferences.start_minimized = enabled;
                        format!(
                            "Start minimized is {}",
                            if enabled { "enabled" } else { "disabled" }
                        )
                    }
                };
                match self.save_gui_config_candidate(&next_config) {
                    Ok(()) => {
                        self.config = next_config;
                        self.state.set_notification(Notification::success(message));
                    }
                    Err(error) => self.state.set_notification(Notification::error(format!(
                        "Could not save preference: {}",
                        error
                    ))),
                }
                Task::none()
            }

            Message::ConfirmPendingAction => {
                let Some(confirmation) = self.state.take_pending_confirmation() else {
                    return Task::none();
                };
                self.state.confirmed_action = Some(confirmation.action.clone());
                match confirmation.action {
                    PendingAction::PowerLimit => self.update(Message::PowerControl(
                        crate::message::PowerControlMessage::ApplyLimit,
                    )),
                    PendingAction::FanCurve => self.update(Message::FanControl(
                        crate::message::FanControlMessage::ApplyCurve,
                    )),
                    PendingAction::Profile(name) => self.update(Message::Profile(
                        crate::message::ProfileMessage::Selected(name),
                    )),
                }
            }

            Message::CancelPendingAction => {
                self.state.cancel_pending_confirmation();
                Task::none()
            }

            // Actions
            Message::ApplySettings => {
                let gpu_indices = self.state.target_gpu_indices();
                let Some(_) = self.state.current_gpu() else {
                    self.state
                        .set_notification(Notification::error("No GPU selected"));
                    return Task::none();
                };
                let pending_limit = self.state.pending_power_limit;

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

                let target_gpus: Vec<_> = self
                    .state
                    .gpus
                    .iter()
                    .filter(|gpu| gpu_indices.contains(&gpu.index))
                    .map(|gpu| (gpu.index, gpu.info.uuid.clone(), gpu.fan_speeds.len()))
                    .collect();
                if target_gpus.is_empty() {
                    self.state
                        .set_notification(Notification::error("No GPU selected"));
                    return Task::none();
                }

                let mut old_limits = Vec::new();
                if let Some(limit) = pending_limit {
                    for gpu in self
                        .state
                        .gpus
                        .iter()
                        .filter(|gpu| gpu_indices.contains(&gpu.index))
                    {
                        let Some(constraints) = gpu.power_constraints else {
                            self.state.set_notification(Notification::error(format!(
                                "GPU {} does not expose power constraints",
                                gpu.info.uuid
                            )));
                            return Task::none();
                        };
                        if !constraints.contains(&limit) {
                            self.state.set_notification(Notification::error(format!(
                                "Power limit {} is outside GPU {} range {}",
                                limit, gpu.info.uuid, constraints
                            )));
                            return Task::none();
                        }
                        old_limits.push((gpu.info.uuid.clone(), gpu.power_limit));
                    }
                }

                let mut curve_updates = Vec::new();
                for (gpu_index, gpu_uuid, fan_count) in &target_gpus {
                    for (fan_idx, curve, enabled) in &curve_data {
                        if *fan_idx >= *fan_count {
                            self.state.set_notification(Notification::error(format!(
                                "Fan {} is out of range for GPU {} ({} fans)",
                                fan_idx + 1,
                                gpu_uuid,
                                fan_count
                            )));
                            return Task::none();
                        }
                        curve_updates.push((
                            *gpu_index,
                            gpu_uuid.clone(),
                            *fan_count,
                            *fan_idx,
                            curve.clone(),
                            *enabled,
                        ));
                    }
                }

                let mut applied_power: Vec<(String, nvctl::domain::PowerLimit)> = Vec::new();
                for (gpu_uuid, old_limit) in &old_limits {
                    let Some(limit) = pending_limit else {
                        break;
                    };
                    if let Err(error) = self.monitor.set_power_limit_by_uuid(gpu_uuid, limit) {
                        for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                            if let Err(rollback_error) = self
                                .monitor
                                .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                            {
                                log::error!(
                                    "Failed to roll back GPU {} power limit: {}",
                                    rollback_uuid,
                                    rollback_error
                                );
                            }
                        }
                        self.state.set_notification(Notification::error(format!(
                            "Failed to apply power limit: {}",
                            error
                        )));
                        return Task::none();
                    }
                    applied_power.push((gpu_uuid.clone(), *old_limit));
                }

                if let Err(error) = self.apply_curve_updates(&curve_updates) {
                    for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                        if let Err(rollback_error) = self
                            .monitor
                            .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                        {
                            log::error!(
                                "Failed to roll back GPU {} power limit: {}",
                                rollback_uuid,
                                rollback_error
                            );
                        }
                    }
                    self.state.set_notification(Notification::error(format!(
                        "Failed to apply fan curves: {}",
                        error
                    )));
                    return Task::none();
                }

                if let Some(limit) = pending_limit {
                    for gpu in &mut self.state.gpus {
                        if gpu_indices.contains(&gpu.index) {
                            gpu.power_limit = limit;
                        }
                    }
                    for &gpu_index in &gpu_indices {
                        self.state.set_gpu_pending_power(gpu_index, None);
                    }
                }
                self.state
                    .set_notification(Notification::success("All settings applied"));
                Task::none()
            }

            Message::ResetSettings => {
                let gpu_indices = self.state.target_gpu_indices();
                let targets: Vec<_> = self
                    .state
                    .gpus
                    .iter()
                    .filter(|gpu| gpu_indices.contains(&gpu.index))
                    .map(|gpu| {
                        (
                            gpu.index,
                            gpu.info.uuid.clone(),
                            gpu.fan_speeds.len(),
                            gpu.power_limit,
                            gpu.power_constraints.map(|constraints| constraints.default),
                        )
                    })
                    .collect();
                if targets.is_empty() {
                    self.state
                        .set_notification(Notification::error("No GPU selected"));
                    return Task::none();
                }
                let mut errors = Vec::new();

                let power_defaults_available = targets.iter().all(|target| target.4.is_some());
                if !power_defaults_available {
                    for (gpu_index, _, _, _, default_limit) in &targets {
                        if default_limit.is_none() {
                            errors.push(format!("GPU {} has no power constraints", gpu_index));
                        }
                    }
                }

                let mut applied_power: Vec<(String, nvctl::domain::PowerLimit)> = Vec::new();
                let mut power_reset = power_defaults_available;
                if power_defaults_available {
                    for (gpu_index, gpu_uuid, _, old_limit, default_limit) in &targets {
                        let Some(default_limit) = *default_limit else {
                            power_reset = false;
                            break;
                        };
                        if let Err(error) = self
                            .monitor
                            .set_power_limit_by_uuid(gpu_uuid, default_limit)
                        {
                            errors.push(format!("GPU {} power reset: {}", gpu_index, error));
                            power_reset = false;
                            for (rollback_uuid, rollback_limit) in applied_power.drain(..).rev() {
                                if let Err(rollback_error) = self
                                    .monitor
                                    .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                                {
                                    errors.push(format!(
                                        "GPU {} power rollback: {}",
                                        rollback_uuid, rollback_error
                                    ));
                                }
                            }
                            break;
                        }
                        applied_power.push((gpu_uuid.clone(), *old_limit));
                    }
                }

                let default_curve = nvctl::domain::FanCurve::default();
                let mut curve_updates = Vec::new();
                for (gpu_index, gpu_uuid, fan_count, _, _) in &targets {
                    for fan_idx in 0..*fan_count {
                        curve_updates.push((
                            *gpu_index,
                            gpu_uuid.clone(),
                            *fan_count,
                            fan_idx,
                            default_curve.clone(),
                            false,
                        ));
                    }
                }
                if let Err(error) = self.apply_curve_updates(&curve_updates) {
                    if power_reset {
                        for (rollback_uuid, rollback_limit) in applied_power.into_iter().rev() {
                            if let Err(rollback_error) = self
                                .monitor
                                .set_power_limit_by_uuid(&rollback_uuid, rollback_limit)
                            {
                                log::error!(
                                    "Failed to roll back GPU {} power limit: {}",
                                    rollback_uuid,
                                    rollback_error
                                );
                            }
                        }
                    }
                    self.state.set_notification(Notification::error(format!(
                        "Failed to reset fan curves: {}",
                        error
                    )));
                    return Task::none();
                }

                if power_reset {
                    for (gpu_index, _, _, _, default_limit) in &targets {
                        if let Some(gpu) = self
                            .state
                            .gpus
                            .iter_mut()
                            .find(|gpu| gpu.index == *gpu_index)
                        {
                            if let Some(default_limit) = default_limit {
                                gpu.power_limit = *default_limit;
                            }
                        }
                        self.state.set_gpu_pending_power(*gpu_index, None);
                    }
                }

                // Restore selected fans to automatic control immediately. If
                // a write fails, the disabled daemon entry remains safe and
                // later polling can retry without re-enabling the curve.
                for (gpu_index, gpu_uuid, fan_count, _, _) in &targets {
                    for fan_idx in 0..*fan_count {
                        if let Err(error) = self.monitor.set_fan_policy_by_uuid(
                            gpu_uuid,
                            fan_idx as u32,
                            FanPolicy::Auto,
                        ) {
                            errors.push(format!(
                                "GPU {} Fan {} policy reset: {}",
                                gpu_index, fan_idx, error
                            ));
                        } else if let Some(policy) = self
                            .state
                            .gpus
                            .iter_mut()
                            .find(|gpu| gpu.info.uuid == *gpu_uuid)
                            .and_then(|gpu| gpu.fan_policies.get_mut(fan_idx))
                        {
                            *policy = Some(FanPolicy::Auto);
                        }
                    }
                }

                if errors.is_empty() {
                    self.state
                        .set_notification(Notification::success("Settings reset to defaults"));
                } else {
                    self.state.set_notification(Notification::warning(format!(
                        "Some settings could not be reset: {}",
                        errors.join(", ")
                    )));
                }
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

        // Feedback and confirmations are overlays so they never move a page's
        // controls out from under the pointer.
        let with_notification: Element<'_, Message> =
            if let Some(ref notif) = self.state.notification {
                iced::widget::stack![
                    main_layout,
                    container(self.view_notification(notif))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(spacing::LG)
                        .align_x(alignment::Horizontal::Right)
                        .align_y(alignment::Vertical::Top),
                ]
                .into()
            } else {
                main_layout.into()
            };

        let with_confirmation: Element<'_, Message> =
            if let Some(ref confirmation) = self.state.pending_confirmation {
                iced::widget::stack![with_notification, self.view_confirmation(confirmation)].into()
            } else {
                with_notification
            };

        container(with_confirmation)
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
            (View::FanControl, "Fan Control", colors::ACCENT_CYAN),
            (View::PowerControl, "Power", colors::ACCENT_CYAN),
            (View::ThermalControl, "Thermal", colors::ACCENT_CYAN),
            (View::Profiles, "Profiles", colors::ACCENT_CYAN),
        ];

        let nav_buttons: Vec<Element<'_, Message>> = nav_items
            .iter()
            .map(|(view, label, color)| self.view_nav_button(*view, label, *color))
            .collect();

        let nav_column = Column::with_children(nav_buttons).spacing(spacing::SM);

        let sidebar_toggle = button(
            text(if self.state.sidebar_expanded {
                "‹ Collapse"
            } else {
                "›"
            })
            .size(font_size::SM),
        )
        .on_press(Message::SidebarToggled)
        .padding([spacing::XS, spacing::SM])
        .width(Length::Fill);

        let settings_button = self.view_nav_button(View::Settings, "Settings", colors::ACCENT_CYAN);

        let brand: Element<'_, Message> = if self.state.sidebar_expanded {
            column![
                text("NVCTL")
                    .size(font_size::XL)
                    .color(colors::TEXT_PRIMARY),
                text("GPU CONTROL")
                    .size(font_size::XS)
                    .color(colors::TEXT_MUTED),
            ]
            .spacing(spacing::XS)
            .into()
        } else {
            text("N")
                .size(font_size::XL)
                .color(colors::ACCENT_CYAN)
                .into()
        };

        let content = column![
            brand,
            sidebar_toggle,
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

    /// Render a compact navigation button with one consistent active state.
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
                    background: Some(colors::ACCENT_CYAN_DIM.into()),
                    text_color: accent,
                    border: iced::Border {
                        color: colors::with_alpha(accent, 0.55),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
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
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            }
        };

        let label_text: Element<'_, Message> = if self.state.sidebar_expanded {
            text(label).size(font_size::BASE).into()
        } else {
            // Two-letter abbreviations remain distinguishable (Power/Profiles
            // would otherwise both be rendered as "P").
            let abbreviation = match view {
                View::Dashboard => "DB",
                View::FanControl => "FN",
                View::PowerControl => "PW",
                View::ThermalControl => "TH",
                View::Profiles => "PR",
                View::Settings => "ST",
            };
            text(abbreviation).size(font_size::SM).into()
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
            View::Settings => views::view_settings(&self.state, &self.config),
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
            .max_width(460.0)
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

    /// Render an explicit confirmation for actions that change several GPUs.
    fn view_confirmation<'a>(
        &'a self,
        confirmation: &'a crate::state::Confirmation,
    ) -> Element<'a, Message> {
        let dialog = container(
            column![
                text(&confirmation.title)
                    .size(font_size::XL)
                    .color(colors::TEXT_PRIMARY),
                text(&confirmation.description)
                    .size(font_size::BASE)
                    .color(colors::TEXT_SECONDARY),
                row![
                    button(text("Cancel").size(font_size::BASE))
                        .on_press(Message::CancelPendingAction)
                        .padding([spacing::SM, spacing::MD]),
                    horizontal_space(),
                    button(text("Apply to all GPUs").size(font_size::BASE))
                        .on_press(Message::ConfirmPendingAction)
                        .padding([spacing::SM, spacing::MD]),
                ]
                .align_y(Alignment::Center)
                .spacing(spacing::MD),
            ]
            .spacing(spacing::LG),
        )
        .padding(spacing::LG)
        .width(Length::Fill)
        .max_width(480.0)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::with_alpha(colors::ACCENT_ORANGE, 0.7),
                width: 1.5,
                radius: 12.0.into(),
            },
            shadow: iced::Shadow {
                color: colors::with_alpha(colors::BG_BASE, 0.8),
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 28.0,
            },
            ..Default::default()
        });

        container(
            container(dialog)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.55).into()),
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
            // Poll GPU state at the configured interval.
            iced::time::every(Duration::from_millis(
                self.config.preferences.poll_interval_ms,
            ))
            .map(Message::Tick),
            iced::window::open_events().map(Message::WindowOpened),
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
            config_writable: true,
            tray: None,
            window_id: None,
            curve_daemon: Arc::new(RwLock::new(CurveDaemon::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{CurvePreset, FanControlMessage, PowerControlMessage};

    #[test]
    fn moving_curve_point_cannot_cross_neighbours_or_decrease_speed() {
        let curve = FanCurve::default_curve();
        let moved = move_curve_point(&curve, 1, 99, 0).unwrap();

        assert_eq!(moved.points()[1].temperature, 74);
        assert_eq!(moved.points()[1].speed.as_percentage(), 30);
        assert_eq!(moved.points()[2].temperature, 75);
    }

    #[test]
    fn adding_curve_point_rejects_duplicates_and_clamps_speed_to_neighbours() {
        let curve = FanCurve::default_curve();
        assert!(add_curve_point(&curve, 60, 50).is_err());

        let (curve, temperature, speed) = add_curve_point(&curve, 70, 0).unwrap();
        assert_eq!(temperature, 70);
        assert_eq!(speed, 50);
        assert_eq!(curve.speed_for_temperature(70).as_percentage(), 50);
    }

    #[test]
    fn moving_missing_curve_point_returns_an_error() {
        assert!(move_curve_point(&FanCurve::default_curve(), usize::MAX, 50, 50).is_err());
    }

    #[test]
    fn stale_curve_messages_do_not_report_false_success() {
        let mut app = NvctlGui::default();

        let _ = app.update(Message::FanControl(FanControlMessage::SelectCurveFan(3)));
        assert!(app.state.notification.as_ref().is_some_and(|notification| {
            notification.level == crate::state::NotificationLevel::Error
        }));

        let _ = app.update(Message::FanControl(FanControlMessage::PresetSelected(
            CurvePreset::Balanced,
        )));
        assert!(app.state.notification.as_ref().is_some_and(|notification| {
            notification.level == crate::state::NotificationLevel::Error
        }));
    }

    #[test]
    fn resetting_an_unavailable_power_limit_reports_an_error() {
        let mut app = NvctlGui::default();
        let _ = app.update(Message::PowerControl(PowerControlMessage::ResetToDefault));

        assert!(app.state.notification.as_ref().is_some_and(|notification| {
            notification.level == crate::state::NotificationLevel::Error
        }));
    }
}
