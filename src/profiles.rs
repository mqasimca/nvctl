//! Shared GPU profile model and read-only profile catalog.
//!
//! Profile files are compatible with `nvctl-gui` and live in the same
//! `$XDG_CONFIG_HOME/nvctl/profiles` directory by default.

use crate::domain::{FanCurve, PowerLimit, Temperature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors raised while discovering or loading profile files.
#[derive(Debug, Error)]
pub enum ProfileLoadError {
    #[error("failed to read profile: {0}")]
    Io(#[from] io::Error),
    #[error("failed to parse profile: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid profile: {0}")]
    Invalid(String),
}

/// A saved GPU configuration profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Profile name (unique identifier).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// GPU-specific settings, keyed by GPU UUID or `default`.
    #[serde(default)]
    pub gpu_settings: HashMap<String, GpuSettings>,
    /// Whether this is the default profile.
    #[serde(default)]
    pub is_default: bool,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last modified timestamp (ISO 8601).
    #[serde(default)]
    pub modified_at: Option<String>,
}

impl Profile {
    /// Create a new empty profile.
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

    /// Create a profile with a description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add settings for a GPU UUID or the `default` fallback.
    pub fn with_gpu_settings(mut self, gpu_id: impl Into<String>, settings: GpuSettings) -> Self {
        self.gpu_settings.insert(gpu_id.into(), settings);
        self
    }

    /// Mark this profile as the default.
    #[allow(clippy::wrong_self_convention)]
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }

    /// Update the modified timestamp.
    pub fn touch(&mut self) {
        self.modified_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Resolve settings for a stable GPU UUID, falling back to `default`.
    pub fn settings_for(&self, gpu_uuid: &str) -> Option<&GpuSettings> {
        self.gpu_settings
            .get(gpu_uuid)
            .or_else(|| self.gpu_settings.get("default"))
    }

    /// Validate invariants which serde alone cannot express.
    pub fn validate(&self) -> Result<(), ProfileLoadError> {
        validate_profile_name(&self.name)?;
        for (gpu_id, settings) in &self.gpu_settings {
            if gpu_id.trim().is_empty() {
                return Err(ProfileLoadError::Invalid(
                    "GPU setting key must not be empty".to_string(),
                ));
            }
            settings.validate(gpu_id)?;
        }
        Ok(())
    }
}

/// Settings for a single GPU within a profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuSettings {
    /// Legacy single fan curve configuration (applied to every fan).
    #[serde(default)]
    pub fan_curve: Option<FanCurve>,
    /// Per-fan curve configuration.
    #[serde(default)]
    pub fan_curves: Vec<FanCurve>,
    /// Per-fan enabled state. Missing legacy values mean enabled.
    #[serde(default)]
    pub fan_curve_enabled: Vec<bool>,
    /// Power limit in watts.
    #[serde(default)]
    pub power_limit: Option<PowerLimit>,
    /// Target acoustic temperature limit.
    #[serde(default)]
    pub acoustic_limit: Option<Temperature>,
    /// Whether to apply fan curves on load.
    #[serde(default)]
    pub apply_fan_curve: bool,
    /// Whether to apply the power limit on load.
    #[serde(default)]
    pub apply_power_limit: bool,
}

impl GpuSettings {
    /// Create empty GPU settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set one fan curve for every fan.
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

    /// Set a power limit.
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

    /// Whether any setting is configured.
    pub fn has_settings(&self) -> bool {
        self.fan_curve.is_some()
            || !self.fan_curves.is_empty()
            || self.power_limit.is_some()
            || self.acoustic_limit.is_some()
    }

    fn validate(&self, gpu_id: &str) -> Result<(), ProfileLoadError> {
        if self.apply_fan_curve && self.fan_curve.is_none() && self.fan_curves.is_empty() {
            return Err(ProfileLoadError::Invalid(format!(
                "GPU '{gpu_id}' enables fan-curve control but contains no curve"
            )));
        }
        if self.fan_curve.is_some() && !self.fan_curves.is_empty() {
            return Err(ProfileLoadError::Invalid(format!(
                "GPU '{gpu_id}' contains both legacy and per-fan curves"
            )));
        }
        if self.fan_curve_enabled.len() > self.fan_curves.len() {
            return Err(ProfileLoadError::Invalid(format!(
                "GPU '{gpu_id}' has more fan enabled flags than fan curves"
            )));
        }
        if self.apply_power_limit && self.power_limit.is_none() {
            return Err(ProfileLoadError::Invalid(format!(
                "GPU '{gpu_id}' enables power-limit control but contains no limit"
            )));
        }
        Ok(())
    }
}

/// Loaded profiles from one directory.
#[derive(Debug, Default)]
pub struct ProfileCatalog {
    directory: PathBuf,
    profiles: HashMap<String, Profile>,
}

impl ProfileCatalog {
    /// Load profiles from the standard nvctl profile directory.
    pub fn load_default() -> Result<Self, ProfileLoadError> {
        Self::load_from(Self::default_directory())
    }

    /// Load profiles from an explicit directory.
    pub fn load_from(directory: impl Into<PathBuf>) -> Result<Self, ProfileLoadError> {
        let directory = directory.into();
        if !directory.exists() {
            return Ok(Self {
                directory,
                profiles: HashMap::new(),
            });
        }

        let mut paths = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();

        let mut profiles = HashMap::new();
        let mut default_name: Option<String> = None;
        for path in paths {
            if !path
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                continue;
            }
            let profile = load_profile(&path)?;
            if profile.is_default {
                if let Some(existing) = &default_name {
                    return Err(ProfileLoadError::Invalid(format!(
                        "profiles '{existing}' and '{}' are both marked as default",
                        profile.name
                    )));
                }
                default_name = Some(profile.name.clone());
            }
            if profiles.insert(profile.name.clone(), profile).is_some() {
                return Err(ProfileLoadError::Invalid(
                    "duplicate profile name".to_string(),
                ));
            }
        }

        Ok(Self {
            directory,
            profiles,
        })
    }

    /// Standard directory shared with nvctl-gui.
    pub fn default_directory() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nvctl")
            .join("profiles")
    }

    /// Directory represented by this catalog.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Find a profile by exact name.
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// List profiles in deterministic name order.
    pub fn list(&self) -> Vec<&Profile> {
        let mut profiles: Vec<_> = self.profiles.values().collect();
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
        profiles
    }

    /// Number of loaded profiles.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Whether no profiles were loaded.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

fn load_profile(path: &Path) -> Result<Profile, ProfileLoadError> {
    let content = fs::read_to_string(path)?;
    let profile: Profile = toml::from_str(&content)?;
    profile.validate()?;
    let filename = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ProfileLoadError::Invalid(format!("invalid path '{}'", path.display())))?;
    if filename != profile.name {
        return Err(ProfileLoadError::Invalid(format!(
            "profile name '{}' does not match filename '{}'",
            profile.name, filename
        )));
    }
    Ok(profile)
}

fn validate_profile_name(name: &str) -> Result<(), ProfileLoadError> {
    if name.trim().is_empty() {
        return Err(ProfileLoadError::Invalid(
            "name cannot be empty".to_string(),
        ));
    }
    if name.chars().count() > 64 {
        return Err(ProfileLoadError::Invalid(
            "name too long (max 64 chars)".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | ' '))
    {
        return Err(ProfileLoadError::Invalid(
            "name contains unsupported characters".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FanCurvePoint, FanSpeed};

    fn curve() -> FanCurve {
        FanCurve::new(
            vec![FanCurvePoint::new(60, FanSpeed::new(50).unwrap())],
            FanSpeed::new(30).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn settings_resolve_uuid_before_default() {
        let default = GpuSettings::new().with_power_limit(PowerLimit::from_watts(200));
        let exact = GpuSettings::new().with_power_limit(PowerLimit::from_watts(300));
        let profile = Profile::new("Performance")
            .with_gpu_settings("default", default)
            .with_gpu_settings("GPU-1", exact);

        assert_eq!(
            profile
                .settings_for("GPU-1")
                .unwrap()
                .power_limit
                .unwrap()
                .as_watts(),
            300
        );
        assert_eq!(
            profile
                .settings_for("GPU-2")
                .unwrap()
                .power_limit
                .unwrap()
                .as_watts(),
            200
        );
    }

    #[test]
    fn catalog_loads_gui_compatible_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let profile = Profile::new("Gaming")
            .with_gpu_settings("default", GpuSettings::new().with_fan_curve(curve()));
        fs::write(
            directory.path().join("Gaming.toml"),
            toml::to_string_pretty(&profile).unwrap(),
        )
        .unwrap();

        let catalog = ProfileCatalog::load_from(directory.path()).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(catalog.get("Gaming").is_some());
    }

    #[test]
    fn catalog_rejects_filename_mismatch_and_invalid_settings() {
        let directory = tempfile::tempdir().unwrap();
        let mut profile = Profile::new("Actual");
        profile.gpu_settings.insert(
            "default".to_string(),
            GpuSettings {
                apply_power_limit: true,
                ..GpuSettings::default()
            },
        );
        fs::write(
            directory.path().join("Wrong.toml"),
            toml::to_string_pretty(&profile).unwrap(),
        )
        .unwrap();

        assert!(ProfileCatalog::load_from(directory.path()).is_err());
    }

    #[test]
    fn checked_in_automation_profiles_stay_compatible() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/automation/profiles");
        let catalog = ProfileCatalog::load_from(directory).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(
            catalog
                .get("Performance")
                .and_then(|profile| profile.settings_for("GPU-unknown"))
                .and_then(|settings| settings.power_limit)
                .map(|limit| limit.as_watts()),
            Some(400)
        );
    }
}
