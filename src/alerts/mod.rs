//! Alert and notification system
//!
//! Provides threshold-based alerting with multiple notification channels.

mod config;
mod manager;
mod notifier;
mod types;

pub use config::{AlertConfig, AlertRuleConfig, AlertSettings, ConditionConfig};
pub use manager::{AlertManager, AlertManagerConfig};
pub use notifier::{NotificationManager, Notifier, TerminalNotifier};
pub use types::{Alert, AlertRule, AlertSeverity, AlertState, Condition, GpuFilter, MetricType};
