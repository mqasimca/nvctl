//! Command handlers
//!
//! Each command handler orchestrates the execution of a CLI command.

pub mod alerts;
pub mod control;
pub mod fan;
pub mod health;
pub mod info;
pub mod list;
pub mod power;
pub mod processes;
pub mod thermal;

pub use alerts::run_alerts;
pub use control::run_control;
pub use fan::run_fan;
pub use health::run_health;
pub use info::run_info;
pub use list::run_list;
pub use power::run_power;
pub use processes::run_processes;
pub use thermal::run_thermal;
