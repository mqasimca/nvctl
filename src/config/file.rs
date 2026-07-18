//! Configuration file loading
//!
//! Handles loading configuration from TOML files.

use crate::config::Config;
use crate::error::ConfigError;

use std::path::{Path, PathBuf};

/// Configuration file handler
pub struct ConfigFile;

impl ConfigFile {
    /// Load configuration from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();

        let content = std::fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ConfigError::FileNotFound(path.display().to_string())
            } else {
                ConfigError::ReadError {
                    path: path.display().to_string(),
                    message: error.to_string(),
                }
            }
        })?;

        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from default locations
    pub fn load_default() -> Result<Option<Config>, ConfigError> {
        for path in Self::default_paths() {
            if path.exists() {
                let config = Self::load(&path)?;
                log::info!("Loaded config from {}", path.display());
                return Ok(Some(config));
            }
        }
        Ok(None)
    }

    /// Get default configuration file paths
    pub fn default_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Project-local configuration has highest implicit precedence.
        paths.push(PathBuf::from(".nvctl.toml"));
        paths.push(PathBuf::from("nvctl.toml"));

        // User configuration overrides the system default.
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("nvctl/config.toml"));
        }

        paths.push(PathBuf::from("/etc/nvctl/config.toml"));

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_paths_not_empty() {
        let paths = ConfigFile::default_paths();
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_load_missing_file() {
        let result = ConfigFile::load("/nonexistent/path/config.toml");
        assert!(matches!(result, Err(ConfigError::FileNotFound(_))));
    }

    #[test]
    fn unreadable_path_is_not_reported_as_missing() {
        let directory = tempfile::tempdir().unwrap();
        let result = ConfigFile::load(directory.path());
        assert!(matches!(result, Err(ConfigError::ReadError { .. })));
    }
}
