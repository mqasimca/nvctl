//! Custom canvas widgets for nvctl-gui
//!
//! All visual components are custom canvas widgets for maximum flexibility.

mod fan_curve;
mod fan_gauge;
mod multi_series_graph;
mod power_bar;
mod temp_gauge;
mod time_series;
mod util_gauge;
mod vram_bar;

pub use fan_curve::FanCurveEditor;
pub use fan_gauge::FanGauge;
pub use multi_series_graph::{DataSeries, MultiSeriesGraph};
pub use power_bar::PowerBar;
pub use temp_gauge::TempGauge;
pub use util_gauge::UtilGauge;
pub use vram_bar::VramBar;

// Re-export single-series graphs for potential use in other views
#[allow(unused_imports)]
pub use time_series::{power_graph, temp_graph, TimeSeriesGraph};
