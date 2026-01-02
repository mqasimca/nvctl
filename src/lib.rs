//! nvctl - NVML-based GPU control library
//!
//! This library provides the core functionality for GPU fan control,
//! power management, and thermal monitoring via NVIDIA's NVML library.
//!
//! # Modules
//!
//! - [`cli`]: Command-line interface definitions
//! - [`commands`]: Command handlers
//! - [`config`]: Configuration system
//! - [`domain`]: Domain models with validation
//! - [`error`]: Error types
//! - [`nvml`]: NVML abstraction layer
//! - [`services`]: Business logic services

pub mod cli;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod nvml;
pub mod services;

#[cfg(test)]
pub mod mock;

pub use error::{AppError, Result};
