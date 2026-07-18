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
#[serde(default, deny_unknown_fields)]
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

impl Config {
    /// Validate cross-field and range constraints after deserialization/merging.
    pub fn validate(&self) -> Result<(), crate::error::ConfigError> {
        if self.general.interval_seconds == 0 || self.general.retry_interval_seconds == 0 {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "general.interval_seconds".to_string(),
                message: "control and retry intervals must be greater than zero".to_string(),
            });
        }
        let selector_count = [
            self.gpu.index.is_some(),
            self.gpu.name.is_some(),
            self.gpu.uuid.is_some(),
        ]
        .into_iter()
        .filter(|configured| *configured)
        .count();
        if selector_count > 1 {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "gpu".to_string(),
                message: "configure only one of index, name, or uuid".to_string(),
            });
        }
        if self
            .gpu
            .name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "gpu.name".to_string(),
                message: "must not be empty".to_string(),
            });
        }
        if self
            .gpu
            .uuid
            .as_ref()
            .is_some_and(|uuid| uuid.trim().is_empty())
        {
            return Err(crate::error::ConfigError::InvalidValue {
                key: "gpu.uuid".to_string(),
                message: "must not be empty".to_string(),
            });
        }
        self.fan
            .to_fan_curve()
            .map_err(|error| crate::error::ConfigError::InvalidValue {
                key: "fan".to_string(),
                message: error.to_string(),
            })?;
        self.power
            .to_power_limit()
            .map_err(|error| crate::error::ConfigError::InvalidValue {
                key: "power.limit_watts".to_string(),
                message: error.to_string(),
            })?;
        Ok(())
    }
}

/// General configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeneralConfig {
    /// Enable verbose logging
    pub verbose: bool,
    /// Dry run mode
    pub dry_run: bool,
    /// Control loop interval in seconds
    #[serde(alias = "interval")]
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
            retry: false,
            retry_interval_seconds: 10,
        }
    }
}

/// GPU selection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
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
#[serde(default, deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct FanCurvePointConfig {
    /// Temperature threshold in Celsius
    pub temperature: i32,
    /// Fan speed percentage at this temperature
    pub speed: u8,
}

/// Power control configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PowerConfig {
    /// Power limit in watts
    pub limit_watts: Option<u32>,
}

impl PowerConfig {
    /// Convert to PowerLimit domain object
    pub fn to_power_limit(&self) -> Result<Option<PowerLimit>, crate::error::DomainError> {
        self.limit_watts.map(PowerLimit::try_from_watts).transpose()
    }
}

/// Thermal configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ThermalConfig {
    /// Acoustic limit in Celsius
    #[serde(alias = "acoustic_limit")]
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

    #[test]
    fn documented_config_aliases_are_supported() {
        let config: Config =
            toml::from_str("[general]\ninterval = 7\n[thermal]\nacoustic_limit = 82\n").unwrap();

        assert_eq!(config.general.interval_seconds, 7);
        assert_eq!(config.thermal.acoustic_limit_celsius, Some(82));
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        assert!(toml::from_str::<Config>("[general]\nintervl = 7\n").is_err());
    }

    #[test]
    fn zero_intervals_are_rejected() {
        let mut config = Config::default();
        config.general.interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn ambiguous_and_blank_gpu_selectors_are_rejected() {
        let mut config = Config::default();
        config.gpu.index = Some(0);
        config.gpu.name = Some("RTX".to_string());
        assert!(config.validate().is_err());

        config.gpu.index = None;
        config.gpu.name = Some("  ".to_string());
        assert!(config.validate().is_err());

        config.gpu.name = None;
        config.gpu.uuid = Some(String::new());
        assert!(config.validate().is_err());
    }
}
