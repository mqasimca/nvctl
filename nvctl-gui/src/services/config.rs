//! GUI configuration service
//!
//! Handles persistent configuration including fan labels and user preferences.

#![allow(dead_code)]

use nvctl::domain::CoolerTarget;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// GUI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GuiConfig {
    /// Fan labels per GPU (by UUID)
    pub fan_labels: HashMap<String, GpuFanConfig>,

    /// UI preferences
    pub preferences: Preferences,
}

/// Fan configuration for a specific GPU
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GpuFanConfig {
    /// Custom labels for each fan (by index)
    pub labels: HashMap<u32, String>,

    /// Auto-detected targets (stored for reference)
    pub detected_targets: HashMap<u32, String>,
}

impl GpuFanConfig {
    /// Get the display name for a fan
    pub fn get_fan_name(&self, index: u32, fallback_target: Option<CoolerTarget>) -> String {
        // First check for custom label
        if let Some(label) = self.labels.get(&index) {
            return label.clone();
        }

        // Use detected target or provided fallback
        if let Some(target_str) = self.detected_targets.get(&index) {
            return format!("Fan {} ({})", index + 1, target_str);
        }

        if let Some(target) = fallback_target {
            return format!("Fan {} ({})", index + 1, target.suggested_position());
        }

        format!("Fan {}", index + 1)
    }

    /// Set a custom label for a fan
    pub fn set_label(&mut self, index: u32, label: String) {
        if label.trim().is_empty() {
            self.labels.remove(&index);
        } else {
            self.labels.insert(index, label);
        }
    }

    /// Store detected target for a fan
    pub fn set_detected_target(&mut self, index: u32, target: CoolerTarget, total_fans: u32) {
        // Generate meaningful default name based on position
        let name = Self::suggest_fan_name(index, total_fans, &target);
        self.detected_targets.insert(index, name);
    }

    /// Suggest a meaningful fan name based on index and total count
    fn suggest_fan_name(index: u32, total_fans: u32, target: &CoolerTarget) -> String {
        // Use target hint if available and meaningful
        match target {
            CoolerTarget::Gpu => return "Center Fan".to_string(),
            CoolerTarget::Memory => return "Memory Fan".to_string(),
            CoolerTarget::PowerSupply => return "Rear Fan".to_string(),
            _ => {}
        }

        // Otherwise, suggest based on typical GPU fan layouts
        match (index, total_fans) {
            // Single fan
            (0, 1) => "Main Fan".to_string(),
            // Dual fan (most common): front + rear
            (0, 2) => "Front Fans".to_string(),
            (1, 2) => "Rear Fan".to_string(),
            // Triple fan: left, center, right or front x2 + rear
            (0, 3) => "Left Fan".to_string(),
            (1, 3) => "Center Fan".to_string(),
            (2, 3) => "Right Fan".to_string(),
            // Quad fan
            (0, 4) => "Front Left".to_string(),
            (1, 4) => "Front Right".to_string(),
            (2, 4) => "Rear Left".to_string(),
            (3, 4) => "Rear Right".to_string(),
            // Fallback
            _ => format!("Fan {}", index + 1),
        }
    }
}

/// UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Sidebar expanded by default
    pub sidebar_expanded: bool,

    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    /// Enable system tray
    pub enable_tray: bool,

    /// Start minimized to tray
    pub start_minimized: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar_expanded: true,
            poll_interval_ms: 1000,
            enable_tray: true,
            start_minimized: false,
        }
    }
}

impl GuiConfig {
    /// Get the config file path
    pub fn config_path() -> PathBuf {
        let config_dir = directories::ProjectDirs::from("", "", "nvctl")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".config/nvctl"));

        config_dir.join("gui.yaml")
    }

    /// Load config from file
    pub fn load() -> Self {
        let path = Self::config_path();
        Self::load_from_path(&path)
    }

    /// Load config from a specific path
    pub fn load_from(path: &str) -> Result<Self, String> {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(format!("Config file not found: {:?}", path));
        }
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
    }

    /// Load config from a path (internal helper)
    fn load_from_path(path: &PathBuf) -> Self {
        if !path.exists() {
            log::info!("No config file found at {:?}, using defaults", path);
            return Self::default();
        }

        match fs::read_to_string(path) {
            Ok(content) => match serde_yaml::from_str(&content) {
                Ok(config) => {
                    log::info!("Loaded config from {:?}", path);
                    config
                }
                Err(e) => {
                    log::warn!("Failed to parse config: {}, using defaults", e);
                    Self::default()
                }
            },
            Err(e) => {
                log::warn!("Failed to read config: {}, using defaults", e);
                Self::default()
            }
        }
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let content = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))?;

        log::info!("Saved config to {:?}", path);
        Ok(())
    }

    /// Get or create fan config for a GPU
    pub fn get_gpu_fan_config(&mut self, gpu_uuid: &str) -> &mut GpuFanConfig {
        self.fan_labels.entry(gpu_uuid.to_string()).or_default()
    }

    /// Get fan config for a GPU (read-only)
    pub fn gpu_fan_config(&self, gpu_uuid: &str) -> Option<&GpuFanConfig> {
        self.fan_labels.get(gpu_uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GuiConfig::default();
        assert!(config.fan_labels.is_empty());
        assert!(config.preferences.sidebar_expanded);
    }

    #[test]
    fn test_fan_name_with_label() {
        let mut gpu_config = GpuFanConfig::default();
        gpu_config.set_label(0, "Front Left".to_string());

        assert_eq!(gpu_config.get_fan_name(0, None), "Front Left");
        assert_eq!(gpu_config.get_fan_name(1, None), "Fan 2");
    }

    #[test]
    fn test_fan_name_with_target() {
        let mut gpu_config = GpuFanConfig::default();
        gpu_config.set_detected_target(0, CoolerTarget::Gpu, 2);

        // CoolerTarget::Gpu maps to "Center Fan"
        assert_eq!(gpu_config.get_fan_name(0, None), "Fan 1 (Center Fan)");
    }

    #[test]
    fn test_fan_name_dual_fan_layout() {
        let mut gpu_config = GpuFanConfig::default();
        // Simulate dual fan with CoolerTarget::All (common case)
        gpu_config.set_detected_target(0, CoolerTarget::All, 2);
        gpu_config.set_detected_target(1, CoolerTarget::All, 2);

        assert_eq!(gpu_config.get_fan_name(0, None), "Fan 1 (Front Fans)");
        assert_eq!(gpu_config.get_fan_name(1, None), "Fan 2 (Rear Fan)");
    }

    #[test]
    fn test_empty_label_removes() {
        let mut gpu_config = GpuFanConfig::default();
        gpu_config.set_label(0, "Test".to_string());
        assert!(gpu_config.labels.contains_key(&0));

        gpu_config.set_label(0, "".to_string());
        assert!(!gpu_config.labels.contains_key(&0));
    }

    #[test]
    fn test_config_serialization() {
        let mut config = GuiConfig::default();
        config
            .get_gpu_fan_config("GPU-TEST-123")
            .set_label(0, "My Fan".to_string());

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: GuiConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(
            parsed
                .gpu_fan_config("GPU-TEST-123")
                .unwrap()
                .labels
                .get(&0),
            Some(&"My Fan".to_string())
        );
    }
}
