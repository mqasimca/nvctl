//! Profile management service
//!
//! Handles saving, loading, and managing GPU configuration profiles.

use nvctl::domain::{FanCurve, PowerLimit, Temperature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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

    #[error("Invalid profile: {0}")]
    InvalidProfile(String),
}

/// A saved GPU configuration profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct GpuSettings {
    /// Legacy single fan curve configuration (applied to every fan)
    #[serde(default)]
    pub fan_curve: Option<FanCurve>,

    /// Per-fan curve configuration
    #[serde(default)]
    pub fan_curves: Vec<FanCurve>,

    /// Per-fan enabled state. Missing legacy values mean enabled.
    #[serde(default)]
    pub fan_curve_enabled: Vec<bool>,

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
        self.fan_curves.clear();
        self.fan_curve_enabled.clear();
        self.apply_fan_curve = true;
        self
    }

    /// Set distinct curves and enabled state for each fan.
    pub fn with_fan_curves(mut self, curves: Vec<FanCurve>, enabled: Vec<bool>) -> Self {
        self.fan_curve = None;
        self.fan_curves = curves;
        self.fan_curve_enabled = enabled;
        self.apply_fan_curve = true;
        self
    }

    /// Resolve per-fan settings, expanding legacy single-curve profiles.
    pub fn resolved_fan_curves(&self, fan_count: usize) -> Vec<(FanCurve, bool)> {
        if !self.fan_curves.is_empty() {
            return self
                .fan_curves
                .iter()
                .take(fan_count)
                .enumerate()
                .map(|(index, curve)| {
                    (
                        curve.clone(),
                        self.fan_curve_enabled.get(index).copied().unwrap_or(true),
                    )
                })
                .collect();
        }

        self.fan_curve
            .as_ref()
            .map(|curve| vec![(curve.clone(), true); fan_count])
            .unwrap_or_default()
    }

    /// Set power limit
    pub fn with_power_limit(mut self, limit: PowerLimit) -> Self {
        self.power_limit = Some(limit);
        self.apply_power_limit = true;
        self
    }

    /// Set an acoustic temperature target.
    pub fn with_acoustic_limit(mut self, limit: Temperature) -> Self {
        self.acoustic_limit = Some(limit);
        self
    }

    /// Check if any settings are configured
    pub fn has_settings(&self) -> bool {
        self.fan_curve.is_some()
            || !self.fan_curves.is_empty()
            || self.power_limit.is_some()
            || self.acoustic_limit.is_some()
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
        if name.trim().is_empty() {
            return Err(ProfileError::InvalidName("Name cannot be empty".into()));
        }
        if name.chars().count() > 64 {
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

    fn validate_profile(profile: &Profile) -> Result<(), ProfileError> {
        Self::validate_name(&profile.name)?;
        for (gpu_id, settings) in &profile.gpu_settings {
            if gpu_id.trim().is_empty() {
                return Err(ProfileError::InvalidProfile(
                    "GPU setting key must not be empty".to_string(),
                ));
            }
            if settings.apply_fan_curve
                && settings.fan_curve.is_none()
                && settings.fan_curves.is_empty()
            {
                return Err(ProfileError::InvalidProfile(format!(
                    "GPU '{gpu_id}' enables fan-curve control but contains no curve"
                )));
            }
            if settings.fan_curve.is_some() && !settings.fan_curves.is_empty() {
                return Err(ProfileError::InvalidProfile(format!(
                    "GPU '{gpu_id}' contains both legacy and per-fan curves"
                )));
            }
            if settings.fan_curve_enabled.len() > settings.fan_curves.len() {
                return Err(ProfileError::InvalidProfile(format!(
                    "GPU '{gpu_id}' has more fan enabled flags than fan curves"
                )));
            }
            if settings.apply_power_limit && settings.power_limit.is_none() {
                return Err(ProfileError::InvalidProfile(format!(
                    "GPU '{gpu_id}' enables power-limit control but contains no limit"
                )));
            }
        }
        Ok(())
    }

    /// Load all profiles from disk
    pub fn load_all(&mut self) -> Result<(), ProfileError> {
        self.ensure_dir()?;
        let previous_active = self.active_profile.clone();

        let mut paths = fs::read_dir(&self.profiles_dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();

        let mut loaded_profiles = HashMap::new();
        let mut loaded_default: Option<String> = None;

        for path in paths {
            if path.extension().is_some_and(|ext| ext == "toml") {
                let profile = self.load_profile_from_path(&path).map_err(|error| {
                    ProfileError::InvalidProfile(format!("{}: {}", path.display(), error))
                })?;
                if profile.is_default {
                    if let Some(existing) = &loaded_default {
                        return Err(ProfileError::InvalidProfile(format!(
                            "profiles '{}' and '{}' are both marked as default",
                            existing, profile.name
                        )));
                    } else {
                        loaded_default = Some(profile.name.clone());
                    }
                }
                loaded_profiles.insert(profile.name.clone(), profile);
            }
        }

        self.profiles = loaded_profiles;
        self.active_profile = previous_active.filter(|name| self.profiles.contains_key(name));

        log::info!("Loaded {} profiles", self.profiles.len());
        Ok(())
    }

    /// Load a profile from a file path
    fn load_profile_from_path(&self, path: &Path) -> Result<Profile, ProfileError> {
        let content = fs::read_to_string(path)?;
        let profile: Profile = toml::from_str(&content)?;
        Self::validate_profile(&profile)?;
        let filename = path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                ProfileError::InvalidProfile(format!(
                    "profile path '{}' has no valid filename",
                    path.display()
                ))
            })?;
        if filename != profile.name {
            return Err(ProfileError::InvalidProfile(format!(
                "profile name '{}' does not match filename '{}'",
                profile.name, filename
            )));
        }
        Ok(profile)
    }

    /// Save a profile to disk
    pub fn save(&mut self, profile: Profile) -> Result<(), ProfileError> {
        Self::validate_profile(&profile)?;
        self.ensure_dir()?;

        let wants_default = profile.is_default;
        // Persist a non-default copy first. `set_default` then updates every
        // affected file transactionally, avoiding two defaults if that update fails.
        let mut stored_profile = profile;
        if wants_default {
            stored_profile.is_default = false;
        }
        let profile_name = stored_profile.name.clone();
        let path = self.profile_path(&profile_name);
        if self.profiles.contains_key(&profile_name) || path.exists() {
            return Err(ProfileError::AlreadyExists(profile_name));
        }
        let content = toml::to_string_pretty(&stored_profile)?;
        Self::write_atomic(&path, &content)?;

        self.profiles.insert(profile_name.clone(), stored_profile);
        if wants_default {
            if let Err(error) = self.set_default(&profile_name) {
                self.profiles.remove(&profile_name);
                if let Err(remove_error) = fs::remove_file(&path) {
                    log::error!(
                        "Failed to remove profile '{}' after default update failed: {}",
                        profile_name,
                        remove_error
                    );
                }
                return Err(error);
            }
        }
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
        self.profiles
            .values()
            .filter(|profile| profile.is_default)
            .min_by(|left, right| left.name.cmp(&right.name))
    }

    /// Set a profile as the default
    pub fn set_default(&mut self, name: &str) -> Result<(), ProfileError> {
        if !self.profiles.contains_key(name) {
            return Err(ProfileError::NotFound(name.to_string()));
        }

        let mut updates: Vec<_> = self
            .profiles
            .values()
            .filter(|profile| profile.is_default || profile.name == name)
            .map(|profile| {
                let old_profile = profile.clone();
                let mut new_profile = profile.clone();
                new_profile.is_default = profile.name == name;
                Ok((
                    profile.name.clone(),
                    self.profile_path(&profile.name),
                    toml::to_string_pretty(&old_profile)?,
                    toml::to_string_pretty(&new_profile)?,
                    new_profile,
                ))
            })
            .collect::<Result<Vec<_>, ProfileError>>()?;
        // Write the new default first so a mid-operation filesystem failure
        // never intentionally leaves the on-disk set without any default.
        updates.sort_by_key(|(profile_name, ..)| profile_name != name);

        let mut completed = Vec::new();
        for (profile_name, path, _, new_content, _) in &updates {
            if let Err(error) = Self::write_atomic(path, new_content) {
                for completed_name in completed.iter().rev() {
                    if let Some((_, rollback_path, old_content, _, _)) = updates
                        .iter()
                        .find(|(candidate, ..)| candidate == completed_name)
                    {
                        if let Err(rollback_error) = Self::write_atomic(rollback_path, old_content)
                        {
                            log::error!(
                                "Failed to roll back profile '{}': {}",
                                completed_name,
                                rollback_error
                            );
                        }
                    }
                }
                return Err(error);
            }
            completed.push(profile_name.clone());
        }

        for (profile_name, _, _, _, new_profile) in updates {
            self.profiles.insert(profile_name, new_profile);
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

        let old_path = self.profile_path(old_name);
        let new_path = self.profile_path(new_name);
        if new_path.exists() {
            return Err(ProfileError::AlreadyExists(new_name.to_string()));
        }
        let mut profile = self
            .profiles
            .get(old_name)
            .cloned()
            .ok_or_else(|| ProfileError::NotFound(old_name.to_string()))?;
        profile.name = new_name.to_string();
        profile.touch();
        let content = toml::to_string_pretty(&profile)?;

        // Write the replacement first. If removing the old file fails, remove
        // the replacement so the cache and disk still describe the old name.
        Self::write_atomic(&new_path, &content)?;
        if old_path.exists() {
            if let Err(error) = fs::remove_file(&old_path) {
                let _ = fs::remove_file(&new_path);
                return Err(error.into());
            }
        }
        self.profiles.remove(old_name);
        self.profiles.insert(new_name.to_string(), profile);

        // Update active profile if needed
        if self.active_profile.as_deref() == Some(old_name) {
            self.active_profile = Some(new_name.to_string());
        }

        Ok(())
    }

    fn write_atomic(path: &Path, content: &str) -> Result<(), ProfileError> {
        let temporary = path.with_extension(format!("toml.tmp-{}", std::process::id()));
        fs::write(&temporary, content)?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
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
    use nvctl::domain::{FanCurvePoint, FanSpeed};

    fn service_in(path: &Path) -> ProfileService {
        ProfileService {
            profiles_dir: path.to_path_buf(),
            profiles: HashMap::new(),
            active_profile: None,
        }
    }

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
        let settings = GpuSettings::new()
            .with_power_limit(PowerLimit::from_watts(250))
            .with_acoustic_limit(Temperature::new(78));

        assert!(settings.has_settings());
        assert!(settings.apply_power_limit);
        assert_eq!(settings.power_limit.unwrap().as_watts(), 250);
        assert_eq!(settings.acoustic_limit, Some(Temperature::new(78)));
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

    #[test]
    fn distinct_fan_curves_and_enabled_flags_round_trip() {
        let first = FanCurve::new(
            vec![FanCurvePoint::new(50, FanSpeed::new(50).unwrap())],
            FanSpeed::new(30).unwrap(),
        )
        .unwrap();
        let second = FanCurve::new(
            vec![FanCurvePoint::new(60, FanSpeed::new(75).unwrap())],
            FanSpeed::new(35).unwrap(),
        )
        .unwrap();
        let settings = GpuSettings::new()
            .with_fan_curves(vec![first.clone(), second.clone()], vec![true, false]);

        assert_eq!(
            settings.resolved_fan_curves(2),
            vec![(first, true), (second, false)]
        );
        assert!(ProfileService::validate_profile(
            &Profile::new("Valid").with_gpu_settings("default", settings)
        )
        .is_ok());
    }

    #[test]
    fn default_profile_updates_are_persistent_and_unique() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service_in(directory.path());
        service.save(Profile::new("First").as_default()).unwrap();
        service.save(Profile::new("Second").as_default()).unwrap();

        assert!(!service.get("First").unwrap().is_default);
        assert!(service.get("Second").unwrap().is_default);

        let mut reloaded = service_in(directory.path());
        reloaded.load_all().unwrap();
        assert_eq!(reloaded.default_profile().unwrap().name, "Second");
        assert_eq!(
            reloaded
                .list()
                .into_iter()
                .filter(|profile| profile.is_default)
                .count(),
            1
        );
    }

    #[test]
    fn malformed_profile_refresh_preserves_the_previous_cache() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service_in(directory.path());
        service.save(Profile::new("Valid")).unwrap();
        fs::write(directory.path().join("Broken.toml"), "not = [valid").unwrap();

        assert!(service.load_all().is_err());
        assert!(service.exists("Valid"));
        assert_eq!(service.count(), 1);
    }

    #[test]
    fn malformed_profile_settings_are_rejected() {
        let profile = Profile::new("Invalid").with_gpu_settings(
            "default",
            GpuSettings {
                apply_power_limit: true,
                ..GpuSettings::default()
            },
        );
        assert!(ProfileService::validate_profile(&profile).is_err());

        let ambiguous = Profile::new("Ambiguous").with_gpu_settings(
            "default",
            GpuSettings {
                fan_curve: Some(FanCurve::default()),
                fan_curves: vec![FanCurve::default()],
                apply_fan_curve: true,
                ..GpuSettings::default()
            },
        );
        assert!(ProfileService::validate_profile(&ambiguous).is_err());

        let unknown = r#"
name = "Unknown"
surprise = true
"#;
        assert!(toml::from_str::<Profile>(unknown).is_err());
    }

    #[test]
    fn saving_never_overwrites_an_existing_profile() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = service_in(directory.path());
        service.save(Profile::new("Existing")).unwrap();

        assert!(matches!(
            service.save(Profile::new("Existing")),
            Err(ProfileError::AlreadyExists(name)) if name == "Existing"
        ));
    }
}
