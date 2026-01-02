//! Command handlers
//!
//! Each command handler orchestrates the execution of a CLI command.

pub mod control;
pub mod fan;
pub mod info;
pub mod list;
pub mod power;
pub mod thermal;

pub use control::run_control;
pub use fan::run_fan;
pub use info::run_info;
pub use list::run_list;
pub use power::run_power;
pub use thermal::run_thermal;
