//! Configuration builder
//!
//! Merges configuration from files and CLI arguments.

use crate::config::{Config, ConfigFile};

/// Builder for merging configuration sources
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Load configuration from a file
    pub fn with_file(mut self, path: Option<&str>) -> Self {
        let file_config = if let Some(path) = path {
            ConfigFile::load(path).ok()
        } else {
            ConfigFile::load_default()
        };

        if let Some(cfg) = file_config {
            self.config = cfg;
        }

        self
    }

    /// Override with CLI verbose flag
    pub fn with_verbose(mut self, verbose: Option<bool>) -> Self {
        if let Some(v) = verbose {
            self.config.general.verbose = v;
        }
        self
    }

    /// Override with CLI dry-run flag
    pub fn with_dry_run(mut self, dry_run: Option<bool>) -> Self {
        if let Some(d) = dry_run {
            self.config.general.dry_run = d;
        }
        self
    }

    /// Override with CLI interval
    pub fn with_interval(mut self, interval: Option<u64>) -> Self {
        if let Some(i) = interval {
            self.config.general.interval_seconds = i;
        }
        self
    }

    /// Override with CLI GPU index
    pub fn with_gpu_index(mut self, index: Option<u32>) -> Self {
        if let Some(i) = index {
            self.config.gpu.index = Some(i);
        }
        self
    }

    /// Override with CLI GPU name
    pub fn with_gpu_name(mut self, name: Option<String>) -> Self {
        if let Some(n) = name {
            self.config.gpu.name = Some(n);
        }
        self
    }

    /// Override with CLI GPU UUID
    pub fn with_gpu_uuid(mut self, uuid: Option<String>) -> Self {
        if let Some(u) = uuid {
            self.config.gpu.uuid = Some(u);
        }
        self
    }

    /// Override with CLI power limit
    pub fn with_power_limit(mut self, limit: Option<u32>) -> Self {
        if let Some(l) = limit {
            self.config.power.limit_watts = Some(l);
        }
        self
    }

    /// Build the final configuration
    pub fn build(self) -> Config {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let config = ConfigBuilder::new().build();
        assert!(!config.general.verbose);
        assert!(!config.general.dry_run);
    }

    #[test]
    fn test_builder_overrides() {
        let config = ConfigBuilder::new()
            .with_verbose(Some(true))
            .with_dry_run(Some(true))
            .with_interval(Some(10))
            .with_gpu_index(Some(1))
            .build();

        assert!(config.general.verbose);
        assert!(config.general.dry_run);
        assert_eq!(config.general.interval_seconds, 10);
        assert_eq!(config.gpu.index, Some(1));
    }
}
