//! Application views
//!
//! Each view corresponds to a screen in the application.

pub mod dashboard;
pub mod fan_control;
pub mod power_control;
pub mod profiles;
pub mod settings;
pub mod thermal;

pub use dashboard::view_dashboard;
pub use fan_control::view_fan_control;
pub use power_control::view_power_control;
pub use profiles::view_profiles;
pub use settings::view_settings;
pub use thermal::view_thermal_control;
