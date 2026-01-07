//! Service layer for GPU control operations
//!
//! Services encapsulate the business logic for fan control, power management,
//! thermal monitoring, and alerting.

pub mod alert_service;
pub mod fan_service;
pub mod monitor;
pub mod power_service;

pub use alert_service::AlertService;
pub use fan_service::FanService;
pub use monitor::Monitor;
pub use power_service::PowerService;
