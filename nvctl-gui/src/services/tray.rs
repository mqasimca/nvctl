//! System tray service using ksni
//!
//! Provides system tray integration for background monitoring.

#![allow(dead_code)] // Some methods are for future use

use ksni::{menu::StandardItem, Icon, MenuItem, Tray, TrayService};
use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

/// System tray state shared between tray and main app
#[derive(Debug, Clone)]
pub struct TrayState {
    /// Current GPU temperature
    temperature: Arc<AtomicI32>,
    /// Current active profile name (if any)
    profile: Arc<std::sync::RwLock<Option<String>>>,
    /// Whether the main window should be shown
    show_window: Arc<AtomicBool>,
    /// Whether app should quit
    quit_requested: Arc<AtomicBool>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayState {
    /// Create a new tray state
    pub fn new() -> Self {
        Self {
            temperature: Arc::new(AtomicI32::new(0)),
            profile: Arc::new(std::sync::RwLock::new(None)),
            show_window: Arc::new(AtomicBool::new(false)),
            quit_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Update the temperature
    pub fn set_temperature(&self, temp: i32) {
        self.temperature.store(temp, Ordering::Relaxed);
    }

    /// Get the current temperature
    pub fn temperature(&self) -> i32 {
        self.temperature.load(Ordering::Relaxed)
    }

    /// Update the active profile
    pub fn set_profile(&self, name: Option<String>) {
        if let Ok(mut profile) = self.profile.write() {
            *profile = name;
        }
    }

    /// Get the current profile name
    pub fn profile(&self) -> Option<String> {
        self.profile.read().ok().and_then(|p| p.clone())
    }

    /// Request to show the main window
    pub fn request_show_window(&self) {
        self.show_window.store(true, Ordering::Relaxed);
    }

    /// Check and clear the show window flag
    pub fn take_show_window(&self) -> bool {
        self.show_window.swap(false, Ordering::Relaxed)
    }

    /// Request to quit the application
    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::Relaxed);
    }

    /// Check if quit was requested
    pub fn quit_requested(&self) -> bool {
        self.quit_requested.load(Ordering::Relaxed)
    }
}

/// nvctl system tray
struct NvctlTray {
    state: TrayState,
}

impl Tray for NvctlTray {
    fn id(&self) -> String {
        "nvctl-gui".into()
    }

    fn title(&self) -> String {
        let temp = self.state.temperature();
        if temp > 0 {
            format!("nvctl - {}°C", temp)
        } else {
            "nvctl".into()
        }
    }

    fn icon_name(&self) -> String {
        let temp = self.state.temperature();
        // Use different icons based on temperature
        // These are standard icon names that should be available on most systems
        if temp == 0 {
            "computer".into() // Default when no temp
        } else if temp < 50 {
            "weather-clear".into() // Cool
        } else if temp < 75 {
            "dialog-warning".into() // Warm
        } else {
            "dialog-error".into() // Hot
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Could provide custom icons here
        vec![]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let temp = self.state.temperature();
        let profile = self.state.profile().unwrap_or_else(|| "None".into());

        ksni::ToolTip {
            title: "nvctl GPU Control".into(),
            description: format!("Temperature: {}°C\nProfile: {}", temp, profile),
            icon_name: String::new(),
            icon_pixmap: vec![],
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let temp = self.state.temperature();
        let profile = self.state.profile().unwrap_or_else(|| "None".into());

        vec![
            // Status display (disabled, just for info)
            MenuItem::Standard(StandardItem {
                label: format!("GPU: {}°C", temp),
                enabled: false,
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: format!("Profile: {}", profile),
                enabled: false,
                ..Default::default()
            }),
            MenuItem::Separator,
            // Open main window
            MenuItem::Standard(StandardItem {
                label: "Open nvctl".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.state.request_show_window();
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            // Quit
            MenuItem::Standard(StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.state.request_quit();
                }),
                ..Default::default()
            }),
        ]
    }
}

/// Handle to a running tray service
pub struct TrayHandle {
    state: TrayState,
    running: Arc<AtomicBool>,
}

impl TrayHandle {
    /// Update the displayed temperature
    pub fn update_temperature(&self, temp: i32) {
        self.state.set_temperature(temp);
    }

    /// Update the active profile
    pub fn update_profile(&self, profile: Option<String>) {
        self.state.set_profile(profile);
    }

    /// Check if show window was requested
    pub fn take_show_window_request(&self) -> bool {
        self.state.take_show_window()
    }

    /// Check if quit was requested
    pub fn quit_requested(&self) -> bool {
        self.state.quit_requested()
    }

    /// Whether the tray service thread is still alive.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

struct TrayRunningGuard(Arc<AtomicBool>);

impl Drop for TrayRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Start the system tray service
///
/// Returns a handle to control the tray, or None if the tray couldn't be started.
pub fn start_tray() -> Option<TrayHandle> {
    let state = TrayState::new();
    let tray_state = state.clone();
    let running = Arc::new(AtomicBool::new(true));
    let service_running = Arc::clone(&running);

    // Start the D-Bus service in a separate thread. Give immediate setup
    // failures a chance to surface so start-minimized never hides the only
    // window when no session bus is available.
    let result = thread::Builder::new()
        .name("nvctl-tray".into())
        .spawn(move || {
            let _running_guard = TrayRunningGuard(service_running);
            let tray = NvctlTray { state: tray_state };
            TrayService::new(tray).run()
        });

    match result {
        Ok(handle) => {
            thread::sleep(Duration::from_millis(100));
            if handle.is_finished() {
                match handle.join() {
                    Ok(Err(error)) => log::warn!("Failed to start system tray: {}", error),
                    Ok(Ok(())) => log::warn!("System tray service stopped during startup"),
                    Err(_) => log::warn!("System tray service panicked during startup"),
                }
                return None;
            }
            // Dropping a JoinHandle detaches the still-running service thread.
            drop(handle);
            log::info!("System tray started");
            Some(TrayHandle { state, running })
        }
        Err(e) => {
            log::warn!("Failed to start system tray: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_state_temperature() {
        let state = TrayState::new();
        assert_eq!(state.temperature(), 0);

        state.set_temperature(65);
        assert_eq!(state.temperature(), 65);
    }

    #[test]
    fn test_tray_state_profile() {
        let state = TrayState::new();
        assert_eq!(state.profile(), None);

        state.set_profile(Some("Gaming".into()));
        assert_eq!(state.profile(), Some("Gaming".into()));
    }

    #[test]
    fn test_tray_state_show_window() {
        let state = TrayState::new();
        assert!(!state.take_show_window());

        state.request_show_window();
        assert!(state.take_show_window());
        // Should be cleared after take
        assert!(!state.take_show_window());
    }

    #[test]
    fn test_tray_state_quit() {
        let state = TrayState::new();
        assert!(!state.quit_requested());

        state.request_quit();
        assert!(state.quit_requested());
    }

    #[test]
    fn tray_handle_reports_service_liveness() {
        let running = Arc::new(AtomicBool::new(true));
        let handle = TrayHandle {
            state: TrayState::new(),
            running: Arc::clone(&running),
        };
        assert!(handle.is_running());
        running.store(false, Ordering::Release);
        assert!(!handle.is_running());
    }
}
