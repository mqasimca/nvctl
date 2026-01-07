//! Application message definitions
//!
//! Hierarchical message structure following The Elm Architecture.

#![allow(dead_code)]

use nvctl::domain::{
    ClockSpeed, FanCurve, FanPolicy, FanSpeed, MemoryInfo, PerformanceState, PowerLimit,
    Temperature, Utilization,
};
use std::time::Instant;

/// Top-level application messages
#[derive(Debug, Clone)]
pub enum Message {
    // === Navigation ===
    /// Switch to a different view
    ViewChanged(View),

    /// Toggle sidebar expanded/collapsed
    SidebarToggled,

    /// Keyboard shortcut pressed
    KeyPressed(KeyboardShortcut),

    // === GPU Selection ===
    /// Select a GPU by index
    GpuSelected(usize),

    /// Toggle linked GPU mode (apply to all)
    LinkedGpusToggled(bool),

    /// Refresh GPU list
    RefreshGpus,

    // === Polling & Updates ===
    /// Periodic tick for GPU polling
    Tick(Instant),

    /// GPU state has been updated
    GpuStateUpdated(Box<GpuStateSnapshot>),

    // === Fan Control ===
    /// Fan control messages
    FanControl(FanControlMessage),

    // === Power Control ===
    /// Power control messages
    PowerControl(PowerControlMessage),

    // === Profiles ===
    /// Profile messages
    Profile(ProfileMessage),

    // === Actions ===
    /// Apply current settings to GPU
    ApplySettings,

    /// Reset settings to GPU defaults
    ResetSettings,

    // === Async Results ===
    /// Operation completed (success or failure)
    OperationResult(Result<String, String>),

    /// Error occurred
    Error(String),

    /// Dismiss error/notification
    DismissNotification,
}

/// Available application views
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Main dashboard with overview
    #[default]
    Dashboard,
    /// Fan control and curve editor
    FanControl,
    /// Power limit control
    PowerControl,
    /// Thermal/acoustic limit control
    ThermalControl,
    /// Profile management
    Profiles,
    /// Application settings
    Settings,
}

impl View {
    /// Get the display name for this view
    pub fn name(&self) -> &'static str {
        match self {
            View::Dashboard => "Dashboard",
            View::FanControl => "Fan Control",
            View::PowerControl => "Power",
            View::ThermalControl => "Thermal",
            View::Profiles => "Profiles",
            View::Settings => "Settings",
        }
    }

    /// Get the icon name for this view
    pub fn icon(&self) -> &'static str {
        match self {
            View::Dashboard => "dashboard",
            View::FanControl => "fan",
            View::PowerControl => "power",
            View::ThermalControl => "thermal",
            View::Profiles => "profiles",
            View::Settings => "settings",
        }
    }
}

/// Snapshot of GPU state at a point in time
#[derive(Debug, Clone)]
pub struct GpuStateSnapshot {
    /// GPU index
    pub index: u32,
    /// GPU name
    pub name: String,
    /// Current temperature
    pub temperature: Temperature,
    /// Fan speeds (one per fan)
    pub fan_speeds: Vec<FanSpeed>,
    /// Fan policies (one per fan)
    pub fan_policies: Vec<FanPolicy>,
    /// Current power usage
    pub power_usage: PowerLimit,
    /// Current power limit
    pub power_limit: PowerLimit,
    /// Graphics clock speed
    pub gpu_clock: ClockSpeed,
    /// Memory clock speed
    pub mem_clock: ClockSpeed,
    /// GPU and memory utilization
    pub utilization: Utilization,
    /// VRAM info (total, used, free)
    pub memory_info: MemoryInfo,
    /// Performance state (P-state)
    pub perf_state: PerformanceState,
    /// Memory temperature (GDDR6X)
    pub memory_temperature: Option<Temperature>,
    /// ECC memory errors
    pub ecc_errors: Option<nvctl::domain::memory::EccErrors>,
    /// PCIe metrics
    pub pcie_metrics: Option<nvctl::domain::pcie::PcieMetrics>,
    /// Encoder utilization
    pub encoder_util: Option<nvctl::domain::performance::EncoderUtilization>,
    /// Decoder utilization
    pub decoder_util: Option<nvctl::domain::performance::DecoderUtilization>,
    /// Overall GPU health score
    pub health_score: Option<nvctl::health::HealthScore>,
    /// Timestamp
    pub timestamp: Instant,
}

/// Fan control specific messages
#[derive(Debug, Clone)]
pub enum FanControlMessage {
    /// Change fan policy for a specific fan
    PolicyChanged(u32, FanPolicy),

    /// Change fan speed for a specific fan (manual mode)
    SpeedChanged(u32, FanSpeed),

    /// Select which fan's curve to edit
    SelectCurveFan(usize),

    /// Move a point on the fan curve (operates on selected fan)
    CurvePointMoved { index: usize, temp: i32, speed: u8 },

    /// Add a point to the fan curve (operates on selected fan)
    CurvePointAdded(i32, u8),

    /// Remove a point from the fan curve (operates on selected fan)
    CurvePointRemoved(usize),

    /// Select a curve preset (operates on selected fan)
    PresetSelected(CurvePreset),

    /// Enable/disable curve control for selected fan
    CurveControlToggled(bool),

    /// Apply fan curve for selected fan
    ApplyCurve,
}

/// Preset fan curve options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurvePreset {
    /// Quiet operation, lower speeds
    Silent,
    /// Balance between noise and cooling
    Balanced,
    /// Maximum cooling performance
    Performance,
}

impl CurvePreset {
    /// Get the display name
    pub fn name(&self) -> &'static str {
        match self {
            CurvePreset::Silent => "Silent",
            CurvePreset::Balanced => "Balanced",
            CurvePreset::Performance => "Performance",
        }
    }

    /// Create a fan curve from this preset
    #[allow(clippy::wrong_self_convention)]
    pub fn to_curve(&self) -> FanCurve {
        use nvctl::domain::FanCurvePoint;

        let (points, default) = match self {
            CurvePreset::Silent => (
                vec![
                    FanCurvePoint::new(30, FanSpeed::new(20).unwrap_or(FanSpeed::new(20).unwrap())),
                    FanCurvePoint::new(60, FanSpeed::new(30).unwrap_or(FanSpeed::new(30).unwrap())),
                    FanCurvePoint::new(80, FanSpeed::new(60).unwrap_or(FanSpeed::new(60).unwrap())),
                    FanCurvePoint::new(
                        90,
                        FanSpeed::new(100).unwrap_or(FanSpeed::new(100).unwrap()),
                    ),
                ],
                FanSpeed::new(20).unwrap_or(FanSpeed::new(20).unwrap()),
            ),
            CurvePreset::Balanced => (
                vec![
                    FanCurvePoint::new(40, FanSpeed::new(30).unwrap_or(FanSpeed::new(30).unwrap())),
                    FanCurvePoint::new(60, FanSpeed::new(50).unwrap_or(FanSpeed::new(50).unwrap())),
                    FanCurvePoint::new(75, FanSpeed::new(80).unwrap_or(FanSpeed::new(80).unwrap())),
                    FanCurvePoint::new(
                        85,
                        FanSpeed::new(100).unwrap_or(FanSpeed::new(100).unwrap()),
                    ),
                ],
                FanSpeed::new(30).unwrap_or(FanSpeed::new(30).unwrap()),
            ),
            CurvePreset::Performance => (
                vec![
                    FanCurvePoint::new(30, FanSpeed::new(50).unwrap_or(FanSpeed::new(50).unwrap())),
                    FanCurvePoint::new(50, FanSpeed::new(70).unwrap_or(FanSpeed::new(70).unwrap())),
                    FanCurvePoint::new(70, FanSpeed::new(90).unwrap_or(FanSpeed::new(90).unwrap())),
                    FanCurvePoint::new(
                        80,
                        FanSpeed::new(100).unwrap_or(FanSpeed::new(100).unwrap()),
                    ),
                ],
                FanSpeed::new(50).unwrap_or(FanSpeed::new(50).unwrap()),
            ),
        };

        FanCurve::new(points, default).unwrap_or_default()
    }
}

/// Power control specific messages
#[derive(Debug, Clone)]
pub enum PowerControlMessage {
    /// Change power limit
    LimitChanged(PowerLimit),

    /// Reset to default power limit
    ResetToDefault,

    /// Apply power limit
    ApplyLimit,
}

/// Profile specific messages
#[derive(Debug, Clone)]
pub enum ProfileMessage {
    /// Select a profile
    Selected(String),

    /// Save current settings as profile
    SaveCurrent(String),

    /// Request delete confirmation for a profile
    RequestDelete(String),

    /// Confirm delete of pending profile
    ConfirmDelete,

    /// Cancel delete
    CancelDelete,

    /// Apply selected profile
    Apply,

    /// Profile name input changed
    NameInputChanged(String),

    /// Set profile as default
    SetDefault(String),

    /// Start editing a profile (name)
    StartEdit(String),

    /// Edit name input changed
    EditNameChanged(String),

    /// Confirm edit (save renamed profile)
    ConfirmEdit,

    /// Cancel editing
    CancelEdit,

    /// Refresh profiles from disk
    Refresh,
}

impl From<FanControlMessage> for Message {
    fn from(msg: FanControlMessage) -> Self {
        Message::FanControl(msg)
    }
}

impl From<PowerControlMessage> for Message {
    fn from(msg: PowerControlMessage) -> Self {
        Message::PowerControl(msg)
    }
}

impl From<ProfileMessage> for Message {
    fn from(msg: ProfileMessage) -> Self {
        Message::Profile(msg)
    }
}

/// Keyboard shortcuts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardShortcut {
    /// Navigate to Dashboard (Ctrl+1)
    GotoDashboard,
    /// Navigate to Fan Control (Ctrl+2)
    GotoFanControl,
    /// Navigate to Power Control (Ctrl+3)
    GotoPower,
    /// Navigate to Thermal (Ctrl+4)
    GotoThermal,
    /// Navigate to Profiles (Ctrl+5)
    GotoProfiles,
    /// Navigate to Settings (Ctrl+,)
    GotoSettings,
    /// Refresh GPU data (F5)
    Refresh,
    /// Toggle sidebar (Ctrl+B)
    ToggleSidebar,
}

impl KeyboardShortcut {
    /// Get the view associated with this shortcut, if any
    pub fn to_view(self) -> Option<View> {
        match self {
            KeyboardShortcut::GotoDashboard => Some(View::Dashboard),
            KeyboardShortcut::GotoFanControl => Some(View::FanControl),
            KeyboardShortcut::GotoPower => Some(View::PowerControl),
            KeyboardShortcut::GotoThermal => Some(View::ThermalControl),
            KeyboardShortcut::GotoProfiles => Some(View::Profiles),
            KeyboardShortcut::GotoSettings => Some(View::Settings),
            _ => None,
        }
    }
}
