//! Unified error types for nvctl
//!
//! This module defines all error types used throughout the application.
//! Uses thiserror for ergonomic error definitions.

use thiserror::Error;

/// Top-level application error type
#[derive(Error, Debug)]
pub enum AppError {
    /// Error from NVML operations
    #[error("NVML error: {0}")]
    Nvml(#[from] NvmlError),

    /// Error from configuration parsing/validation
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Error from domain type validation
    #[error("Domain validation error: {0}")]
    Domain(#[from] DomainError),

    /// GPU not found by name or UUID
    #[error("GPU not found: {0}")]
    GpuNotFound(String),

    /// No GPUs detected in the system
    #[error("No NVIDIA GPUs detected")]
    NoGpusFound,

    /// Unsupported driver version
    #[error("Unsupported driver version: {current} (minimum required: {minimum})")]
    UnsupportedDriver { current: String, minimum: String },

    /// IO error (file operations)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from NVML wrapper operations
#[derive(Error, Debug)]
pub enum NvmlError {
    /// Failed to initialize NVML library
    #[error("Failed to initialize NVML: {0}")]
    InitializationFailed(String),

    /// NVML library not found
    #[error("NVML library not found. Is the NVIDIA driver installed?")]
    LibraryNotFound,

    /// Device not found at index
    #[error("GPU device not found at index {0}")]
    DeviceNotFound(u32),

    /// Device not found by UUID
    #[error("GPU device not found with UUID: {0}")]
    DeviceNotFoundByUuid(String),

    /// Operation not supported by this GPU
    #[error("Operation not supported: {0}")]
    NotSupported(String),

    /// Insufficient permissions
    #[error("Insufficient permissions: {0}. Try running with sudo.")]
    InsufficientPermissions(String),

    /// Unknown NVML error
    #[error("NVML error: {0}")]
    Unknown(String),

    /// GPU is lost (fallen off bus, etc.)
    #[error("GPU is lost or has become inaccessible")]
    GpuLost,

    /// Invalid argument passed to NVML
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Fan control is not available
    #[error("Fan control not available for this GPU")]
    FanControlNotAvailable,
}

/// Errors from domain type validation
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Invalid fan speed value (must be 0-100)
    #[error("Invalid fan speed: {0}% (must be 0-100)")]
    InvalidFanSpeed(u8),

    /// Invalid power limit value
    #[error("Invalid power limit: {value}W (valid range: {min}-{max}W)")]
    InvalidPowerLimit { value: u32, min: u32, max: u32 },

    /// Invalid temperature value
    #[error("Invalid temperature: {0}°C")]
    InvalidTemperature(i32),

    /// Invalid value provided
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    /// Invalid fan curve (not enough points, not sorted, etc.)
    #[error("Invalid fan curve: {0}")]
    InvalidFanCurve(String),

    /// Fan curve points must be sorted by temperature
    #[error("Fan curve points must be sorted by ascending temperature")]
    UnsortedFanCurve,

    /// Fan curve must have at least one point
    #[error("Fan curve must have at least one point")]
    EmptyFanCurve,
}

/// Errors from configuration parsing and validation
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Config file not found
    #[error("Configuration file not found: {0}")]
    FileNotFound(String),

    /// Failed to parse config file
    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    /// Invalid config value
    #[error("Invalid configuration value for '{key}': {message}")]
    InvalidValue { key: String, message: String },

    /// Missing required config field
    #[error("Missing required configuration field: {0}")]
    MissingField(String),

    /// TOML parsing error
    #[error("TOML parse error: {0}")]
    TomlError(#[from] toml::de::Error),

    /// JSON serialization error
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Errors from service operations
#[derive(Error, Debug)]
pub enum ServiceError {
    /// NVML operation failed
    #[error("NVML operation failed: {0}")]
    Nvml(#[from] NvmlError),

    /// Domain validation failed
    #[error("Validation failed: {0}")]
    Domain(#[from] DomainError),

    /// Service is in dry-run mode
    #[error("Operation skipped (dry-run mode)")]
    DryRun,
}

impl From<ServiceError> for AppError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Nvml(e) => AppError::Nvml(e),
            ServiceError::Domain(e) => AppError::Domain(e),
            ServiceError::DryRun => AppError::Domain(DomainError::InvalidFanCurve(
                "Operation skipped (dry-run mode)".to_string(),
            )),
        }
    }
}

/// Result type alias using AppError
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_error_display() {
        let err = DomainError::InvalidFanSpeed(150);
        assert_eq!(err.to_string(), "Invalid fan speed: 150% (must be 0-100)");
    }

    #[test]
    fn test_nvml_error_display() {
        let err = NvmlError::LibraryNotFound;
        assert!(err.to_string().contains("NVIDIA driver"));
    }

    #[test]
    fn test_power_limit_error_display() {
        let err = DomainError::InvalidPowerLimit {
            value: 500,
            min: 100,
            max: 400,
        };
        assert!(err.to_string().contains("500W"));
        assert!(err.to_string().contains("100-400W"));
    }

    #[test]
    fn test_error_conversion() {
        let domain_err = DomainError::InvalidFanSpeed(120);
        let app_err: AppError = domain_err.into();
        assert!(matches!(app_err, AppError::Domain(_)));
    }
}
