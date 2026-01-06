//! Custom canvas widgets for nvctl-gui
//!
//! All visual components are custom canvas widgets for maximum flexibility.

mod fan_curve;
mod fan_gauge;
mod power_bar;
mod temp_gauge;
mod time_series;

pub use fan_curve::FanCurveEditor;
pub use fan_gauge::FanGauge;
pub use power_bar::PowerBar;
pub use temp_gauge::TempGauge;
pub use time_series::temp_graph;

// Re-export for potential future use
#[allow(unused_imports)]
pub use time_series::{power_graph, TimeSeriesGraph};
