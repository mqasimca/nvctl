//! Workload-aware profile rule evaluation.

use crate::domain::{GpuProcess, ProcessType};
use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

fn default_interval_seconds() -> u64 {
    2
}

fn default_cooldown_seconds() -> u64 {
    10
}

fn default_release_delay_seconds() -> u64 {
    5
}

fn default_min_duration_seconds() -> u64 {
    3
}

/// Automation configuration embedded in `nvctl.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutomationConfig {
    /// Whether automation is enabled for `nvctl auto run`.
    pub enabled: bool,
    /// Poll interval for GPU processes and fan curves.
    pub interval_seconds: u64,
    /// Minimum time between profile transitions.
    pub cooldown_seconds: u64,
    /// Time a workload-free state must remain stable before leaving a profile.
    pub release_delay_seconds: u64,
    /// Optional profile to apply when no rule matches. If absent, restore baseline.
    pub fallback_profile: Option<String>,
    /// Optional profile directory override.
    pub profiles_dir: Option<PathBuf>,
    /// Ordered workload rules.
    pub rules: Vec<AutomationRule>,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: default_interval_seconds(),
            cooldown_seconds: default_cooldown_seconds(),
            release_delay_seconds: default_release_delay_seconds(),
            fallback_profile: None,
            profiles_dir: None,
            rules: Vec::new(),
        }
    }
}

impl AutomationConfig {
    /// Validate rules, timings, and glob syntax.
    pub fn validate(&self) -> Result<(), AutomationError> {
        if self.interval_seconds == 0 {
            return Err(AutomationError::InvalidConfig(
                "automation.interval_seconds must be greater than zero".to_string(),
            ));
        }
        if self
            .fallback_profile
            .as_ref()
            .is_some_and(|profile| profile.trim().is_empty())
        {
            return Err(AutomationError::InvalidConfig(
                "automation.fallback_profile must not be empty".to_string(),
            ));
        }

        let mut names = std::collections::HashSet::new();
        for rule in &self.rules {
            rule.validate()?;
            if !names.insert(rule.name.as_str()) {
                return Err(AutomationError::InvalidConfig(format!(
                    "duplicate automation rule name '{}'",
                    rule.name
                )));
            }
        }
        Ok(())
    }
}

/// One automatic profile activation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutomationRule {
    /// Human-readable unique rule name.
    pub name: String,
    /// Profile to activate.
    pub profile: String,
    /// Case-insensitive process-name glob patterns.
    pub processes: Vec<String>,
    /// Optional GPU process type filter.
    pub process_type: Option<ProcessMatchType>,
    /// Optional stable GPU UUID filter.
    pub gpu_uuid: Option<String>,
    /// Higher priorities win. Equal priorities keep configuration order.
    pub priority: i32,
    /// Candidate must remain matched for this duration before activation.
    pub min_duration_seconds: u64,
}

impl Default for AutomationRule {
    fn default() -> Self {
        Self {
            name: String::new(),
            profile: String::new(),
            processes: Vec::new(),
            process_type: None,
            gpu_uuid: None,
            priority: 0,
            min_duration_seconds: default_min_duration_seconds(),
        }
    }
}

impl AutomationRule {
    fn validate(&self) -> Result<(), AutomationError> {
        if self.name.trim().is_empty() {
            return Err(AutomationError::InvalidConfig(
                "automation rule name must not be empty".to_string(),
            ));
        }
        if self.profile.trim().is_empty() {
            return Err(AutomationError::InvalidConfig(format!(
                "automation rule '{}' has an empty profile",
                self.name
            )));
        }
        if self.processes.is_empty() && self.process_type.is_none() {
            return Err(AutomationError::InvalidConfig(format!(
                "automation rule '{}' needs processes or process_type",
                self.name
            )));
        }
        if self
            .gpu_uuid
            .as_ref()
            .is_some_and(|uuid| uuid.trim().is_empty())
        {
            return Err(AutomationError::InvalidConfig(format!(
                "automation rule '{}' has an empty gpu_uuid",
                self.name
            )));
        }
        for pattern in &self.processes {
            compile_glob(pattern).map_err(|error| {
                AutomationError::InvalidConfig(format!(
                    "automation rule '{}' has invalid process glob '{}': {}",
                    self.name, pattern, error
                ))
            })?;
        }
        Ok(())
    }
}

/// GPU process type filter used by an automation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessMatchType {
    Graphics,
    Compute,
    Both,
    Any,
}

impl ProcessMatchType {
    fn matches(self, process_type: ProcessType) -> bool {
        match self {
            Self::Graphics => process_type.is_graphics(),
            Self::Compute => process_type.is_compute(),
            Self::Both => process_type == ProcessType::GraphicsCompute,
            Self::Any => true,
        }
    }
}

/// A process plus the stable identity of the GPU on which NVML reported it.
#[derive(Debug, Clone)]
pub struct ProcessObservation {
    pub gpu_uuid: String,
    pub gpu_index: u32,
    pub process: GpuProcess,
}

/// Why a rule was selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleMatch {
    pub rule: String,
    pub profile: String,
    pub priority: i32,
    pub gpu_uuid: String,
    pub gpu_index: u32,
    pub pid: u32,
    pub process: String,
    pub process_type: String,
    pub min_duration_seconds: u64,
}

/// Compiled deterministic rule evaluator.
pub struct AutomationEngine {
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    config: AutomationRule,
    process_matchers: Vec<GlobMatcher>,
}

impl AutomationEngine {
    /// Compile all process globs once at startup.
    pub fn new(config: &AutomationConfig) -> Result<Self, AutomationError> {
        config.validate()?;
        let rules = config
            .rules
            .iter()
            .map(|rule| {
                let process_matchers = rule
                    .processes
                    .iter()
                    .map(|pattern| compile_glob(pattern))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(CompiledRule {
                    config: rule.clone(),
                    process_matchers,
                })
            })
            .collect::<Result<Vec<_>, globset::Error>>()?;
        Ok(Self { rules })
    }

    /// Return the highest-priority matching rule. Configuration order breaks ties.
    pub fn evaluate(&self, observations: &[ProcessObservation]) -> Option<RuleMatch> {
        let mut winner: Option<RuleMatch> = None;
        for rule in &self.rules {
            let matched_process = observations
                .iter()
                .find(|observation| rule.matches(observation));
            let Some(observation) = matched_process else {
                continue;
            };
            let candidate = RuleMatch {
                rule: rule.config.name.clone(),
                profile: rule.config.profile.clone(),
                priority: rule.config.priority,
                gpu_uuid: observation.gpu_uuid.clone(),
                gpu_index: observation.gpu_index,
                pid: observation.process.pid,
                process: observation.process.display_name(),
                process_type: observation.process.process_type.to_string(),
                min_duration_seconds: rule.config.min_duration_seconds,
            };
            if winner
                .as_ref()
                .is_none_or(|current| candidate.priority > current.priority)
            {
                winner = Some(candidate);
            }
        }
        winner
    }
}

impl CompiledRule {
    fn matches(&self, observation: &ProcessObservation) -> bool {
        if self
            .config
            .gpu_uuid
            .as_ref()
            .is_some_and(|uuid| uuid != &observation.gpu_uuid)
        {
            return false;
        }
        if self
            .config
            .process_type
            .is_some_and(|filter| !filter.matches(observation.process.process_type))
        {
            return false;
        }
        if self.process_matchers.is_empty() {
            return true;
        }
        observation.process.name.as_ref().is_some_and(|name| {
            self.process_matchers
                .iter()
                .any(|matcher| matcher.is_match(name))
        })
    }
}

fn compile_glob(pattern: &str) -> Result<GlobMatcher, globset::Error> {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(false)
        .build()
        .map(|glob| glob.compile_matcher())
}

/// Desired controller state, including the unmodified session baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesiredProfile {
    Baseline,
    Named(String),
}

/// Debounces candidates and enforces transition cooldowns.
#[derive(Debug)]
pub struct ProfileSwitchGate {
    active: DesiredProfile,
    pending: Option<(DesiredProfile, Duration)>,
    last_switch: Option<Duration>,
}

impl Default for ProfileSwitchGate {
    fn default() -> Self {
        Self {
            active: DesiredProfile::Baseline,
            pending: None,
            last_switch: None,
        }
    }
}

impl ProfileSwitchGate {
    /// Current applied state according to the gate.
    pub fn active(&self) -> &DesiredProfile {
        &self.active
    }

    /// Observe one candidate and return a profile transition when it becomes eligible.
    pub fn observe(
        &mut self,
        desired: DesiredProfile,
        now: Duration,
        stable_for: Duration,
        cooldown: Duration,
    ) -> Option<DesiredProfile> {
        if desired == self.active {
            self.pending = None;
            return None;
        }

        match &mut self.pending {
            Some((pending, since)) if pending == &desired => {
                let stable = now.saturating_sub(*since) >= stable_for;
                let cooled_down = self
                    .last_switch
                    .is_none_or(|last| now.saturating_sub(last) >= cooldown);
                if stable && cooled_down {
                    self.active = desired.clone();
                    self.pending = None;
                    self.last_switch = Some(now);
                    Some(desired)
                } else {
                    None
                }
            }
            _ => {
                self.pending = Some((desired.clone(), now));
                if stable_for.is_zero()
                    && self
                        .last_switch
                        .is_none_or(|last| now.saturating_sub(last) >= cooldown)
                {
                    self.active = desired.clone();
                    self.pending = None;
                    self.last_switch = Some(now);
                    Some(desired)
                } else {
                    None
                }
            }
        }
    }
}

/// Automation configuration or glob compilation error.
#[derive(Debug, Error)]
pub enum AutomationError {
    #[error("invalid automation configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid process pattern: {0}")]
    Glob(#[from] globset::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(name: &str, process_type: ProcessType) -> ProcessObservation {
        ProcessObservation {
            gpu_uuid: "GPU-1".to_string(),
            gpu_index: 0,
            process: GpuProcess::with_name(42, name.to_string(), 100, process_type),
        }
    }

    fn rule(name: &str, profile: &str, pattern: &str, priority: i32) -> AutomationRule {
        AutomationRule {
            name: name.to_string(),
            profile: profile.to_string(),
            processes: vec![pattern.to_string()],
            priority,
            min_duration_seconds: 0,
            ..AutomationRule::default()
        }
    }

    #[test]
    fn engine_matches_case_insensitive_globs_and_priority() {
        let config = AutomationConfig {
            rules: vec![
                rule("generic", "Balanced", "steam*", 10),
                rule("game", "Performance", "STEAM_APP_*", 100),
            ],
            ..AutomationConfig::default()
        };
        let engine = AutomationEngine::new(&config).unwrap();
        let matched = engine
            .evaluate(&[observation("steam_app_123", ProcessType::Graphics)])
            .unwrap();
        assert_eq!(matched.rule, "game");
        assert_eq!(matched.profile, "Performance");
    }

    #[test]
    fn equal_priority_preserves_config_order() {
        let config = AutomationConfig {
            rules: vec![
                rule("first", "First", "blender", 5),
                rule("second", "Second", "blender", 5),
            ],
            ..AutomationConfig::default()
        };
        let engine = AutomationEngine::new(&config).unwrap();
        assert_eq!(
            engine
                .evaluate(&[observation("blender", ProcessType::Compute)])
                .unwrap()
                .profile,
            "First"
        );
    }

    #[test]
    fn filters_by_process_type_and_gpu_uuid() {
        let config = AutomationConfig {
            rules: vec![AutomationRule {
                name: "render".to_string(),
                profile: "Render".to_string(),
                process_type: Some(ProcessMatchType::Compute),
                gpu_uuid: Some("GPU-1".to_string()),
                min_duration_seconds: 0,
                ..AutomationRule::default()
            }],
            ..AutomationConfig::default()
        };
        let engine = AutomationEngine::new(&config).unwrap();
        assert!(engine
            .evaluate(&[observation("python", ProcessType::Compute)])
            .is_some());
        assert!(engine
            .evaluate(&[observation("game", ProcessType::Graphics)])
            .is_none());
    }

    #[test]
    fn invalid_rules_are_rejected() {
        let config = AutomationConfig {
            rules: vec![rule("bad", "Performance", "[", 0)],
            ..AutomationConfig::default()
        };
        assert!(AutomationEngine::new(&config).is_err());
    }

    #[test]
    fn switch_gate_debounces_and_enforces_cooldown() {
        let mut gate = ProfileSwitchGate::default();
        let performance = DesiredProfile::Named("Performance".to_string());
        assert_eq!(
            gate.observe(
                performance.clone(),
                Duration::from_secs(0),
                Duration::from_secs(3),
                Duration::from_secs(10)
            ),
            None
        );
        assert_eq!(
            gate.observe(
                performance.clone(),
                Duration::from_secs(3),
                Duration::from_secs(3),
                Duration::from_secs(10)
            ),
            Some(performance.clone())
        );
        assert_eq!(
            gate.observe(
                DesiredProfile::Baseline,
                Duration::from_secs(8),
                Duration::ZERO,
                Duration::from_secs(10)
            ),
            None
        );
        assert_eq!(
            gate.observe(
                DesiredProfile::Baseline,
                Duration::from_secs(13),
                Duration::ZERO,
                Duration::from_secs(10)
            ),
            Some(DesiredProfile::Baseline)
        );
    }
}
