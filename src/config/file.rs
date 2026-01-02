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

        let content = std::fs::read_to_string(path)
            .map_err(|_| ConfigError::FileNotFound(path.display().to_string()))?;

        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from default locations
    pub fn load_default() -> Option<Config> {
        for path in Self::default_paths() {
            if path.exists() {
                if let Ok(config) = Self::load(&path) {
                    log::info!("Loaded config from {}", path.display());
                    return Some(config);
                }
            }
        }
        None
    }

    /// Get default configuration file paths
    pub fn default_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // System-wide config
        paths.push(PathBuf::from("/etc/nvctl/config.toml"));

        // User config
        if let Some(home) = dirs_path_home() {
            paths.push(home.join(".config/nvctl/config.toml"));
        }

        // Current directory
        paths.push(PathBuf::from("nvctl.toml"));
        paths.push(PathBuf::from(".nvctl.toml"));

        paths
    }
}

fn dirs_path_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
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
}
