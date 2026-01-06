//! Backend services for nvctl-gui
//!
//! Services handle all GPU interactions and data management.

mod config;
pub mod curve_daemon;
mod gpu_monitor;
mod profiles;
mod tray;

#[allow(unused_imports)]
pub use config::{GpuFanConfig, GuiConfig, Preferences};
pub use curve_daemon::CurveDaemon;
pub use gpu_monitor::GpuMonitor;
pub use profiles::{GpuSettings, Profile, ProfileService};
pub use tray::{start_tray, TrayHandle};
