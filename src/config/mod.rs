//! Configuration system
//!
//! Handles TOML config file parsing and CLI argument merging.

pub mod builder;
pub mod file;

pub use builder::ConfigBuilder;
pub use file::ConfigFile;

use crate::domain::{FanCurve, FanCurvePoint, FanSpeed, PowerLimit};
use serde::{Deserialize, Serialize};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,
    /// GPU selection settings
    pub gpu: GpuConfig,
    /// Fan control settings
    pub fan: FanConfig,
    /// Power control settings
    pub power: PowerConfig,
    /// Thermal settings
    pub thermal: ThermalConfig,
}

/// General configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Enable verbose logging
    pub verbose: bool,
    /// Dry run mode
    pub dry_run: bool,
    /// Control loop interval in seconds
    pub interval_seconds: u64,
    /// Enable retry on errors
    pub retry: bool,
    /// Retry interval in seconds
    pub retry_interval_seconds: u64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            dry_run: false,
            interval_seconds: 5,
            retry: true,
            retry_interval_seconds: 10,
        }
    }
}

/// GPU selection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GpuConfig {
    /// Target GPU by index
    pub index: Option<u32>,
    /// Target GPU by name (partial match)
    pub name: Option<String>,
    /// Target GPU by UUID
    pub uuid: Option<String>,
}

/// Fan control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FanConfig {
    /// Default fan speed percentage
    pub default_speed: u8,
    /// Fan curve points
    pub curve: Vec<FanCurvePointConfig>,
}

impl Default for FanConfig {
    fn default() -> Self {
        Self {
            default_speed: 30,
            curve: vec![
                FanCurvePointConfig {
                    temperature: 40,
                    speed: 30,
                },
                FanCurvePointConfig {
                    temperature: 60,
                    speed: 50,
                },
                FanCurvePointConfig {
                    temperature: 75,
                    speed: 80,
                },
                FanCurvePointConfig {
                    temperature: 85,
                    speed: 100,
                },
            ],
        }
    }
}

impl FanConfig {
    /// Convert to a FanCurve domain object
    pub fn to_fan_curve(&self) -> Result<FanCurve, crate::error::DomainError> {
        let default_speed = FanSpeed::new(self.default_speed)?;

        let points: Result<Vec<_>, _> = self
            .curve
            .iter()
            .map(|p| {
                let speed = FanSpeed::new(p.speed)?;
                Ok(FanCurvePoint::new(p.temperature, speed))
            })
            .collect();

        FanCurve::new(points?, default_speed)
    }
}

/// Fan curve point configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurvePointConfig {
    /// Temperature threshold in Celsius
    pub temperature: i32,
    /// Fan speed percentage at this temperature
    pub speed: u8,
}

/// Power control configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PowerConfig {
    /// Power limit in watts
    pub limit_watts: Option<u32>,
}

impl PowerConfig {
    /// Convert to PowerLimit domain object
    pub fn to_power_limit(&self) -> Option<PowerLimit> {
        self.limit_watts.map(PowerLimit::from_watts)
    }
}

/// Thermal configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ThermalConfig {
    /// Acoustic limit in Celsius
    pub acoustic_limit_celsius: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.interval_seconds, 5);
        assert_eq!(config.fan.default_speed, 30);
        assert!(!config.fan.curve.is_empty());
    }

    #[test]
    fn test_fan_config_to_curve() {
        let config = FanConfig::default();
        let curve = config.to_fan_curve().unwrap();
        assert_eq!(curve.points().len(), 4);
    }
}
