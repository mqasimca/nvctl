//! Custom canvas widgets for nvctl-gui
//!
//! All visual components are custom canvas widgets for maximum flexibility.

mod ecc_gauge;
mod fan_curve;
mod fan_gauge;
mod health_gauge;
mod mem_temp_gauge;
mod multi_series_graph;
mod pcie_gauge;
mod power_bar;
mod temp_gauge;
mod time_series;
mod util_gauge;
mod video_gauge;
mod vram_bar;

pub use ecc_gauge::EccGauge;
pub use fan_curve::FanCurveEditor;
pub use fan_gauge::FanGauge;
#[allow(unused_imports)]
pub use health_gauge::HealthGauge;
pub use mem_temp_gauge::MemTempGauge;
pub use multi_series_graph::{DataSeries, MultiSeriesGraph};
pub use pcie_gauge::PcieGauge;
pub use power_bar::PowerBar;
pub use temp_gauge::TempGauge;
pub use util_gauge::UtilGauge;
pub use video_gauge::VideoGauge;
pub use vram_bar::VramBar;

// Re-export single-series graphs for potential use in other views
#[allow(unused_imports)]
pub use time_series::{power_graph, temp_graph, TimeSeriesGraph};
