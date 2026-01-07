//! nvctl - NVML-based GPU control library
//!
//! This library provides the core functionality for GPU fan control,
//! power management, and thermal monitoring via NVIDIA's NVML library.
//!
//! # Modules
//!
//! - [`alerts`]: Alerting and notification system
//! - [`cli`]: Command-line interface definitions
//! - [`commands`]: Command handlers
//! - [`config`]: Configuration system
//! - [`domain`]: Domain models with validation
//! - [`error`]: Error types
//! - [`health`]: GPU health scoring and monitoring
//! - [`nvml`]: NVML abstraction layer
//! - [`services`]: Business logic services

pub mod alerts;
pub mod cli;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod health;
pub mod nvml;
pub mod services;

/// Mock implementations for testing
/// Available when the "mock" feature is enabled or during tests
#[cfg(any(test, feature = "mock"))]
pub mod mock;

pub use error::{AppError, Result};
