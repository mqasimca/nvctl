//! Profile management service
//!
//! Handles saving, loading, and managing GPU configuration profiles.

use nvctl::domain::{FanCurve, PowerLimit, Temperature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Profile management errors
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum ProfileError {
    #[error("Failed to read profile: {0}")]
    ReadError(#[from] io::Error),

    #[error("Failed to parse profile: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to serialize profile: {0}")]
    SerializeError(#[from] toml::ser::Error),

    #[error("Profile not found: {0}")]
    NotFound(String),

    #[error("Profile already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid profile name: {0}")]
    InvalidName(String),
}

/// A saved GPU configuration profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Profile name (unique identifier)
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// GPU-specific settings, keyed by GPU UUID or index
    #[serde(default)]
    pub gpu_settings: HashMap<String, GpuSettings>,

    /// Whether this is the default profile to load on startup
    #[serde(default)]
    pub is_default: bool,

    /// Creation timestamp (ISO 8601)
    #[serde(default)]
    pub created_at: Option<String>,

    /// Last modified timestamp (ISO 8601)
    #[serde(default)]
    pub modified_at: Option<String>,
}

#[allow(dead_code)]
impl Profile {
    /// Create a new empty profile
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            name: name.into(),
            description: None,
            gpu_settings: HashMap::new(),
            is_default: false,
            created_at: Some(now.clone()),
            modified_at: Some(now),
        }
    }

    /// Create a profile with a description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add settings for a GPU
    pub fn with_gpu_settings(mut self, gpu_id: impl Into<String>, settings: GpuSettings) -> Self {
        self.gpu_settings.insert(gpu_id.into(), settings);
        self
    }

    /// Mark as default profile
    #[allow(clippy::wrong_self_convention)]
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }

    /// Update the modified timestamp
    pub fn touch(&mut self) {
        self.modified_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

/// Settings for a single GPU within a profile
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuSettings {
    /// Fan curve configuration
    #[serde(default)]
    pub fan_curve: Option<FanCurve>,

    /// Power limit in watts
    #[serde(default)]
    pub power_limit: Option<PowerLimit>,

    /// Target acoustic temperature limit
    #[serde(default)]
    pub acoustic_limit: Option<Temperature>,

    /// Whether to apply fan curve on load
    #[serde(default)]
    pub apply_fan_curve: bool,

    /// Whether to apply power limit on load
    #[serde(default)]
    pub apply_power_limit: bool,
}

#[allow(dead_code)]
impl GpuSettings {
    /// Create new GPU settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set fan curve
    pub fn with_fan_curve(mut self, curve: FanCurve) -> Self {
        self.fan_curve = Some(curve);
        self.apply_fan_curve = true;
        self
    }

    /// Set power limit
    pub fn with_power_limit(mut self, limit: PowerLimit) -> Self {
        self.power_limit = Some(limit);
        self.apply_power_limit = true;
        self
    }

    /// Check if any settings are configured
    pub fn has_settings(&self) -> bool {
        self.fan_curve.is_some() || self.power_limit.is_some() || self.acoustic_limit.is_some()
    }
}

/// Profile service for managing profiles
#[allow(dead_code)]
pub struct ProfileService {
    /// Directory where profiles are stored
    profiles_dir: PathBuf,

    /// Cached profiles
    profiles: HashMap<String, Profile>,

    /// Currently active profile name
    active_profile: Option<String>,
}

#[allow(dead_code)]
impl ProfileService {
    /// Create a new profile service
    pub fn new() -> Self {
        let profiles_dir = Self::default_profiles_dir();

        let mut service = Self {
            profiles_dir,
            profiles: HashMap::new(),
            active_profile: None,
        };

        // Try to load existing profiles
        if let Err(e) = service.load_all() {
            log::warn!("Failed to load profiles: {}", e);
        }

        service
    }

    /// Get the default profiles directory
    fn default_profiles_dir() -> PathBuf {
        directories::ProjectDirs::from("", "", "nvctl")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| {
                std::env::var("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".config")
                    .join("nvctl")
            })
            .join("profiles")
    }

    /// Ensure the profiles directory exists
    fn ensure_dir(&self) -> Result<(), ProfileError> {
        if !self.profiles_dir.exists() {
            fs::create_dir_all(&self.profiles_dir)?;
        }
        Ok(())
    }

    /// Get the path for a profile file
    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{}.toml", name))
    }

    /// Validate a profile name
    fn validate_name(name: &str) -> Result<(), ProfileError> {
        if name.is_empty() {
            return Err(ProfileError::InvalidName("Name cannot be empty".into()));
        }
        if name.len() > 64 {
            return Err(ProfileError::InvalidName(
                "Name too long (max 64 chars)".into(),
            ));
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ')
        {
            return Err(ProfileError::InvalidName(
                "Name can only contain letters, numbers, spaces, hyphens, and underscores".into(),
            ));
        }
        Ok(())
    }

    /// Load all profiles from disk
    pub fn load_all(&mut self) -> Result<(), ProfileError> {
        self.ensure_dir()?;
        self.profiles.clear();

        if let Ok(entries) = fs::read_dir(&self.profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml") {
                    match self.load_profile_from_path(&path) {
                        Ok(profile) => {
                            if profile.is_default {
                                self.active_profile = Some(profile.name.clone());
                            }
                            self.profiles.insert(profile.name.clone(), profile);
                        }
                        Err(e) => {
                            log::warn!("Failed to load profile {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        log::info!("Loaded {} profiles", self.profiles.len());
        Ok(())
    }

    /// Load a profile from a file path
    fn load_profile_from_path(&self, path: &PathBuf) -> Result<Profile, ProfileError> {
        let content = fs::read_to_string(path)?;
        let profile: Profile = toml::from_str(&content)?;
        Ok(profile)
    }

    /// Save a profile to disk
    pub fn save(&mut self, profile: Profile) -> Result<(), ProfileError> {
        Self::validate_name(&profile.name)?;
        self.ensure_dir()?;

        let path = self.profile_path(&profile.name);
        let content = toml::to_string_pretty(&profile)?;
        fs::write(&path, content)?;

        self.profiles.insert(profile.name.clone(), profile);
        log::info!("Saved profile to {:?}", path);
        Ok(())
    }

    /// Delete a profile
    pub fn delete(&mut self, name: &str) -> Result<(), ProfileError> {
        if !self.profiles.contains_key(name) {
            return Err(ProfileError::NotFound(name.to_string()));
        }

        let path = self.profile_path(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }

        self.profiles.remove(name);

        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }

        log::info!("Deleted profile: {}", name);
        Ok(())
    }

    /// Get a profile by name
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// Get all profiles
    pub fn list(&self) -> Vec<&Profile> {
        let mut profiles: Vec<_> = self.profiles.values().collect();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        profiles
    }

    /// Get profile names
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.profiles.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the currently active profile
    pub fn active(&self) -> Option<&Profile> {
        self.active_profile
            .as_ref()
            .and_then(|name| self.profiles.get(name))
    }

    /// Get the active profile name
    pub fn active_name(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }

    /// Set the active profile
    pub fn set_active(&mut self, name: Option<String>) {
        self.active_profile = name;
    }

    /// Get the default profile (marked as is_default)
    pub fn default_profile(&self) -> Option<&Profile> {
        self.profiles.values().find(|p| p.is_default)
    }

    /// Set a profile as the default
    pub fn set_default(&mut self, name: &str) -> Result<(), ProfileError> {
        // First, find profiles that need updating and collect their names and paths
        let profiles_to_update: Vec<(String, PathBuf)> = self
            .profiles
            .values()
            .filter(|p| p.is_default && p.name != name)
            .map(|p| (p.name.clone(), self.profile_path(&p.name)))
            .collect();

        // Clear existing defaults
        for (profile_name, path) in profiles_to_update {
            if let Some(profile) = self.profiles.get_mut(&profile_name) {
                profile.is_default = false;
                let content = toml::to_string_pretty(profile)?;
                fs::write(&path, content)?;
            }
        }

        // Check if target profile exists
        if !self.profiles.contains_key(name) {
            return Err(ProfileError::NotFound(name.to_string()));
        }

        // Get path before mutable borrow
        let path = self.profile_path(name);

        // Set new default
        if let Some(profile) = self.profiles.get_mut(name) {
            profile.is_default = true;
            let content = toml::to_string_pretty(profile)?;
            fs::write(&path, content)?;
        }
        Ok(())
    }

    /// Check if a profile exists
    pub fn exists(&self, name: &str) -> bool {
        self.profiles.contains_key(name)
    }

    /// Rename a profile
    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<(), ProfileError> {
        Self::validate_name(new_name)?;

        if !self.profiles.contains_key(old_name) {
            return Err(ProfileError::NotFound(old_name.to_string()));
        }

        if self.profiles.contains_key(new_name) {
            return Err(ProfileError::AlreadyExists(new_name.to_string()));
        }

        // Remove old file
        let old_path = self.profile_path(old_name);
        if old_path.exists() {
            fs::remove_file(&old_path)?;
        }

        // Update and save with new name
        if let Some(mut profile) = self.profiles.remove(old_name) {
            profile.name = new_name.to_string();
            profile.touch();
            self.save(profile)?;
        }

        // Update active profile if needed
        if self.active_profile.as_deref() == Some(old_name) {
            self.active_profile = Some(new_name.to_string());
        }

        Ok(())
    }

    /// Get the number of profiles
    pub fn count(&self) -> usize {
        self.profiles.len()
    }
}

impl Default for ProfileService {
    fn default() -> Self {
        Self::new()
    }
}

/// Create some default preset profiles
#[allow(dead_code)]
pub fn create_default_profiles() -> Vec<Profile> {
    use crate::message::CurvePreset;

    vec![
        Profile::new("Silent")
            .with_description("Quiet operation with reduced fan speeds")
            .with_gpu_settings(
                "default",
                GpuSettings::new().with_fan_curve(CurvePreset::Silent.to_curve()),
            ),
        Profile::new("Balanced")
            .with_description("Balance between noise and cooling")
            .with_gpu_settings(
                "default",
                GpuSettings::new().with_fan_curve(CurvePreset::Balanced.to_curve()),
            )
            .as_default(),
        Profile::new("Performance")
            .with_description("Maximum cooling for heavy workloads")
            .with_gpu_settings(
                "default",
                GpuSettings::new().with_fan_curve(CurvePreset::Performance.to_curve()),
            ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_creation() {
        let profile = Profile::new("Test Profile")
            .with_description("A test profile")
            .as_default();

        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.description, Some("A test profile".to_string()));
        assert!(profile.is_default);
        assert!(profile.created_at.is_some());
    }

    #[test]
    fn test_gpu_settings() {
        let settings = GpuSettings::new().with_power_limit(PowerLimit::from_watts(250));

        assert!(settings.has_settings());
        assert!(settings.apply_power_limit);
        assert_eq!(settings.power_limit.unwrap().as_watts(), 250);
    }

    #[test]
    fn test_validate_name() {
        assert!(ProfileService::validate_name("Valid-Name_123").is_ok());
        assert!(ProfileService::validate_name("Profile with spaces").is_ok());
        assert!(ProfileService::validate_name("").is_err());
        assert!(ProfileService::validate_name("a".repeat(100).as_str()).is_err());
        assert!(ProfileService::validate_name("invalid/name").is_err());
    }

    #[test]
    fn test_profile_serialization() {
        let profile = Profile::new("Test")
            .with_description("Test profile")
            .with_gpu_settings("gpu-0", GpuSettings::new());

        let serialized = toml::to_string(&profile).unwrap();
        let deserialized: Profile = toml::from_str(&serialized).unwrap();

        assert_eq!(profile.name, deserialized.name);
        assert_eq!(profile.description, deserialized.description);
    }
}
