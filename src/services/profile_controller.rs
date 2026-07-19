//! Transactional profile application and active fan-curve maintenance.

use crate::domain::{FanPolicy, FanSpeed, PowerLimit, Temperature};
use crate::error::NvmlError;
use crate::nvml::{GpuDevice, GpuManager};
use crate::profiles::{GpuSettings, Profile};
use serde::Serialize;
use thiserror::Error;

/// Result of applying or restoring a profile.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileApplyReport {
    pub profile: Option<String>,
    pub dry_run: bool,
    pub actions: Vec<String>,
}

/// Errors from profile validation, application, or rollback.
#[derive(Debug, Error)]
pub enum ProfileApplyError {
    #[error("NVML operation failed: {0}")]
    Nvml(#[from] NvmlError),
    #[error("profile '{profile}' is invalid for GPU {gpu_uuid}: {message}")]
    InvalidProfile {
        profile: String,
        gpu_uuid: String,
        message: String,
    },
    #[error("failed to roll back profile transition after '{original}': {rollback}")]
    Rollback { original: String, rollback: String },
    #[error("profile controller state error: {0}")]
    State(String),
}

#[derive(Debug, Clone)]
struct FanState {
    speed: FanSpeed,
    policy: Option<FanPolicy>,
}

#[derive(Debug, Clone)]
struct DeviceState {
    uuid: String,
    power_limit: Option<PowerLimit>,
    acoustic_limit: Option<Temperature>,
    fans: Vec<FanState>,
}

/// Applies profiles to selected GPUs and restores the original session state.
pub struct ProfileController<'manager, M: GpuManager> {
    manager: &'manager M,
    baseline: Vec<DeviceState>,
    dry_run: bool,
    active_profile: Option<String>,
}

impl<M: GpuManager> Drop for ProfileController<'_, M> {
    fn drop(&mut self) {
        if self.active_profile.is_some() {
            if let Err(error) = self.restore() {
                log::error!("Failed to restore the pre-profile GPU state: {}", error);
            }
        }
    }
}

impl<'manager, M: GpuManager> ProfileController<'manager, M> {
    /// Capture a baseline for every selected GPU before any mutation happens.
    pub fn new(
        manager: &'manager M,
        target_indices: &[u32],
        dry_run: bool,
    ) -> Result<Self, ProfileApplyError> {
        let target_uuids = target_indices
            .iter()
            .map(|index| manager.device_by_index(*index)?.uuid())
            .collect::<Result<Vec<_>, _>>()?;
        Self::new_by_uuids(manager, &target_uuids, dry_run)
    }

    /// Capture baselines by stable UUID after the caller establishes ownership.
    pub fn new_by_uuids(
        manager: &'manager M,
        target_uuids: &[String],
        dry_run: bool,
    ) -> Result<Self, ProfileApplyError> {
        let mut baseline = Vec::with_capacity(target_uuids.len());
        for uuid in target_uuids {
            let device = manager.device_by_uuid(uuid)?;
            baseline.push(capture_state(&device)?);
        }
        Ok(Self {
            manager,
            baseline,
            dry_run,
            active_profile: None,
        })
    }

    /// Stable UUIDs of GPUs controlled by this session.
    pub fn target_uuids(&self) -> Vec<&str> {
        self.baseline
            .iter()
            .map(|state| state.uuid.as_str())
            .collect()
    }

    /// Currently active profile name, if any.
    pub fn active_profile(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }

    /// Validate all targets, then atomically reconcile them with a profile.
    pub fn apply_profile(
        &mut self,
        profile: &Profile,
    ) -> Result<ProfileApplyReport, ProfileApplyError> {
        let targets = self.baseline.clone();
        for baseline in &targets {
            let device = self.manager.device_by_uuid(&baseline.uuid)?;
            validate_settings(&device, baseline, profile)?;
        }

        let before = self.capture_current_states()?;
        let mut actions = Vec::new();
        for baseline in &targets {
            let mut device = match self.manager.device_by_uuid(&baseline.uuid) {
                Ok(device) => device,
                Err(error) => {
                    return self.rollback_transition(error.into(), &before);
                }
            };
            let Some(current) = before.iter().find(|state| state.uuid == baseline.uuid) else {
                return self.rollback_transition(
                    ProfileApplyError::State(format!(
                        "no transition snapshot exists for GPU {}",
                        baseline.uuid
                    )),
                    &before,
                );
            };
            let settings = profile.settings_for(&baseline.uuid);
            if let Err(error) = reconcile_device(
                &mut device,
                baseline,
                current,
                settings,
                self.dry_run,
                &mut actions,
            ) {
                return self.rollback_transition(error, &before);
            }
        }

        self.active_profile = Some(profile.name.clone());
        Ok(ProfileApplyReport {
            profile: Some(profile.name.clone()),
            dry_run: self.dry_run,
            actions,
        })
    }

    /// Re-evaluate active fan curves using current temperatures.
    pub fn tick_fan_curves(
        &mut self,
        profile: &Profile,
    ) -> Result<ProfileApplyReport, ProfileApplyError> {
        let mut actions = Vec::new();
        for baseline in &self.baseline {
            let Some(settings) = profile.settings_for(&baseline.uuid) else {
                continue;
            };
            if !settings.apply_fan_curve {
                continue;
            }
            let mut device = self.manager.device_by_uuid(&baseline.uuid)?;
            let temperature = device.temperature()?;
            let curves = settings.resolved_fan_curves(baseline.fans.len());
            for (fan_index, (curve, enabled)) in curves.into_iter().enumerate() {
                if !enabled {
                    continue;
                }
                let target = curve.speed_for_temperature(temperature.as_celsius());
                let current = device.fan_speed(fan_index as u32)?;
                if current == target {
                    continue;
                }
                actions.push(format!(
                    "GPU {} fan {}: {} -> {} at {}",
                    baseline.uuid, fan_index, current, target, temperature
                ));
                if !self.dry_run {
                    device.set_fan_policy(fan_index as u32, FanPolicy::Manual)?;
                    device.set_fan_speed(fan_index as u32, target)?;
                }
            }
        }
        Ok(ProfileApplyReport {
            profile: Some(profile.name.clone()),
            dry_run: self.dry_run,
            actions,
        })
    }

    /// Restore the state captured at session startup.
    pub fn restore(&mut self) -> Result<ProfileApplyReport, ProfileApplyError> {
        let before = self.capture_current_states()?;
        let mut actions = Vec::new();
        for baseline in &self.baseline {
            let mut device = match self.manager.device_by_uuid(&baseline.uuid) {
                Ok(device) => device,
                Err(error) => {
                    return self.rollback_transition(error.into(), &before);
                }
            };
            let Some(current) = before.iter().find(|state| state.uuid == baseline.uuid) else {
                return self.rollback_transition(
                    ProfileApplyError::State(format!(
                        "no restoration snapshot exists for GPU {}",
                        baseline.uuid
                    )),
                    &before,
                );
            };
            if let Err(error) =
                restore_device(&mut device, baseline, current, self.dry_run, &mut actions)
            {
                return self.rollback_transition(error, &before);
            }
        }
        self.active_profile = None;
        Ok(ProfileApplyReport {
            profile: None,
            dry_run: self.dry_run,
            actions,
        })
    }

    fn capture_current_states(&self) -> Result<Vec<DeviceState>, ProfileApplyError> {
        self.baseline
            .iter()
            .map(|baseline| {
                let device = self.manager.device_by_uuid(&baseline.uuid)?;
                capture_state(&device).map_err(ProfileApplyError::from)
            })
            .collect()
    }

    fn rollback_transition<T>(
        &self,
        original: ProfileApplyError,
        before: &[DeviceState],
    ) -> Result<T, ProfileApplyError> {
        if self.dry_run {
            return Err(original);
        }
        let mut failures = Vec::new();
        for state in before.iter().rev() {
            match self.manager.device_by_uuid(&state.uuid) {
                Ok(mut device) => {
                    let current = match capture_state(&device) {
                        Ok(current) => current,
                        Err(error) => {
                            failures.push(format!("{}: {}", state.uuid, error));
                            continue;
                        }
                    };
                    if let Err(error) =
                        restore_device(&mut device, state, &current, false, &mut Vec::new())
                    {
                        failures.push(format!("{}: {}", state.uuid, error));
                    }
                }
                Err(error) => failures.push(format!("{}: {}", state.uuid, error)),
            }
        }
        if failures.is_empty() {
            Err(original)
        } else {
            Err(ProfileApplyError::Rollback {
                original: original.to_string(),
                rollback: failures.join("; "),
            })
        }
    }
}

fn capture_state<D: GpuDevice>(device: &D) -> Result<DeviceState, NvmlError> {
    let power_limit = optional_metric(device.power_limit())?;
    let acoustic_limit =
        optional_metric(device.acoustic_limits())?.and_then(|limits| limits.current);
    let fan_count = device.fan_count()?;
    let mut fans = Vec::with_capacity(fan_count as usize);
    for fan_index in 0..fan_count {
        fans.push(FanState {
            speed: device.fan_speed(fan_index)?,
            policy: optional_metric(device.fan_policy(fan_index))?,
        });
    }
    Ok(DeviceState {
        uuid: device.uuid()?,
        power_limit,
        acoustic_limit,
        fans,
    })
}

fn optional_metric<T>(result: Result<T, NvmlError>) -> Result<Option<T>, NvmlError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(NvmlError::NotSupported(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn validate_settings<D: GpuDevice>(
    device: &D,
    baseline: &DeviceState,
    profile: &Profile,
) -> Result<(), ProfileApplyError> {
    let Some(settings) = profile.settings_for(&baseline.uuid) else {
        return Ok(());
    };
    let invalid = |message: String| ProfileApplyError::InvalidProfile {
        profile: profile.name.clone(),
        gpu_uuid: baseline.uuid.clone(),
        message,
    };

    if settings.apply_power_limit {
        let limit = settings
            .power_limit
            .ok_or_else(|| invalid("power-limit control is enabled without a limit".to_string()))?;
        if baseline.power_limit.is_none() {
            return Err(invalid(
                "power-limit control is unsupported or has no rollback value".to_string(),
            ));
        }
        let constraints = device.power_constraints()?;
        if !constraints.contains(&limit) {
            return Err(invalid(format!(
                "power limit {} is outside device range {}",
                limit, constraints
            )));
        }
    }

    if let Some(target) = settings.acoustic_limit {
        let limits = optional_metric(device.acoustic_limits())?
            .ok_or_else(|| invalid("acoustic limits are not supported".to_string()))?;
        let (Some(minimum), Some(maximum), Some(_)) = (limits.min, limits.max, limits.current)
        else {
            return Err(invalid(
                "device does not expose complete acoustic limits and rollback state".to_string(),
            ));
        };
        if target < minimum || target > maximum {
            return Err(invalid(format!(
                "acoustic limit {} is outside device range {}-{}",
                target, minimum, maximum
            )));
        }
    }

    if settings.apply_fan_curve {
        if baseline.fans.is_empty() {
            return Err(invalid("device exposes no controllable fans".to_string()));
        }
        if settings.fan_curve.is_none() && settings.fan_curves.is_empty() {
            return Err(invalid(
                "fan-curve control is enabled without a curve".to_string(),
            ));
        }
        if !settings.fan_curves.is_empty() && settings.fan_curves.len() != baseline.fans.len() {
            return Err(invalid(format!(
                "profile has {} fan curves but device exposes {} fans",
                settings.fan_curves.len(),
                baseline.fans.len()
            )));
        }
        let curves = settings.resolved_fan_curves(baseline.fans.len());
        for (fan_index, (_, enabled)) in curves.iter().enumerate() {
            if *enabled && baseline.fans[fan_index].policy.is_none() {
                return Err(invalid(format!(
                    "fan {fan_index} control policy cannot be read for safe restoration"
                )));
            }
        }
    }
    Ok(())
}

fn reconcile_device<D: GpuDevice>(
    device: &mut D,
    baseline: &DeviceState,
    current: &DeviceState,
    settings: Option<&GpuSettings>,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    ensure_fan_topology(baseline, current)?;

    let desired_power = settings
        .filter(|settings| settings.apply_power_limit)
        .and_then(|settings| settings.power_limit)
        .or(baseline.power_limit);
    reconcile_power(
        device,
        &baseline.uuid,
        current.power_limit,
        desired_power,
        dry_run,
        actions,
    )?;

    let desired_acoustic = settings
        .and_then(|settings| settings.acoustic_limit)
        .or(baseline.acoustic_limit);
    reconcile_acoustic(
        device,
        &baseline.uuid,
        current.acoustic_limit,
        desired_acoustic,
        dry_run,
        actions,
    )?;

    let curves = settings
        .filter(|settings| settings.apply_fan_curve)
        .map(|settings| settings.resolved_fan_curves(baseline.fans.len()))
        .unwrap_or_default();
    let temperature = if curves.iter().any(|(_, enabled)| *enabled) {
        Some(device.temperature()?)
    } else {
        None
    };
    for fan_index in 0..baseline.fans.len() {
        let desired = curves.get(fan_index).filter(|(_, enabled)| *enabled);
        if let Some((curve, _)) = desired {
            let current_temperature = temperature.ok_or_else(|| {
                ProfileApplyError::State(format!(
                    "no temperature was captured for enabled GPU {} fan curve",
                    baseline.uuid
                ))
            })?;
            let target = curve.speed_for_temperature(current_temperature.as_celsius());
            reconcile_fan_manual(
                device,
                &baseline.uuid,
                fan_index,
                &current.fans[fan_index],
                target,
                dry_run,
                actions,
            )?;
        } else {
            reconcile_fan_baseline(
                device,
                &baseline.uuid,
                fan_index,
                &current.fans[fan_index],
                &baseline.fans[fan_index],
                dry_run,
                actions,
            )?;
        }
    }
    Ok(())
}

fn restore_device<D: GpuDevice>(
    device: &mut D,
    desired: &DeviceState,
    current: &DeviceState,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    ensure_fan_topology(desired, current)?;

    reconcile_power(
        device,
        &desired.uuid,
        current.power_limit,
        desired.power_limit,
        dry_run,
        actions,
    )?;
    reconcile_acoustic(
        device,
        &desired.uuid,
        current.acoustic_limit,
        desired.acoustic_limit,
        dry_run,
        actions,
    )?;
    for fan_index in 0..desired.fans.len() {
        reconcile_fan_baseline(
            device,
            &desired.uuid,
            fan_index,
            &current.fans[fan_index],
            &desired.fans[fan_index],
            dry_run,
            actions,
        )?;
    }
    Ok(())
}

fn ensure_fan_topology(
    desired: &DeviceState,
    current: &DeviceState,
) -> Result<(), ProfileApplyError> {
    if desired.fans.len() != current.fans.len() {
        return Err(ProfileApplyError::State(format!(
            "GPU {} fan topology changed from {} to {} fan(s)",
            desired.uuid,
            desired.fans.len(),
            current.fans.len()
        )));
    }
    Ok(())
}

fn reconcile_power<D: GpuDevice>(
    device: &mut D,
    uuid: &str,
    current: Option<PowerLimit>,
    desired: Option<PowerLimit>,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    if let Some(desired) = desired.filter(|desired| current != Some(*desired)) {
        actions.push(format!(
            "GPU {uuid} power: {} -> {}",
            current.map_or_else(|| "unsupported".to_string(), |value| value.to_string()),
            desired
        ));
        if !dry_run {
            device.set_power_limit(desired)?;
        }
    }
    Ok(())
}

fn reconcile_acoustic<D: GpuDevice>(
    device: &mut D,
    uuid: &str,
    current: Option<Temperature>,
    desired: Option<Temperature>,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    if let Some(desired) = desired.filter(|desired| current != Some(*desired)) {
        actions.push(format!(
            "GPU {uuid} acoustic limit: {} -> {}",
            current.map_or_else(|| "unsupported".to_string(), |value| value.to_string()),
            desired
        ));
        if !dry_run {
            device.set_acoustic_limit(desired)?;
        }
    }
    Ok(())
}

fn reconcile_fan_manual<D: GpuDevice>(
    device: &mut D,
    uuid: &str,
    fan_index: usize,
    current: &FanState,
    desired_speed: FanSpeed,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    if current.policy != Some(FanPolicy::Manual) || current.speed != desired_speed {
        actions.push(format!(
            "GPU {uuid} fan {fan_index}: {:?}/{} -> Manual/{}",
            current.policy, current.speed, desired_speed
        ));
        if !dry_run {
            device.set_fan_policy(fan_index as u32, FanPolicy::Manual)?;
            device.set_fan_speed(fan_index as u32, desired_speed)?;
        }
    }
    Ok(())
}

fn reconcile_fan_baseline<D: GpuDevice>(
    device: &mut D,
    uuid: &str,
    fan_index: usize,
    current: &FanState,
    desired: &FanState,
    dry_run: bool,
    actions: &mut Vec<String>,
) -> Result<(), ProfileApplyError> {
    if current.policy == desired.policy
        && (desired.policy != Some(FanPolicy::Manual) || current.speed == desired.speed)
    {
        return Ok(());
    }
    actions.push(format!(
        "GPU {uuid} fan {fan_index}: {:?}/{} -> {:?}/{}",
        current.policy, current.speed, desired.policy, desired.speed
    ));
    if dry_run {
        return Ok(());
    }
    match desired.policy {
        Some(FanPolicy::Manual) => {
            device.set_fan_policy(fan_index as u32, FanPolicy::Manual)?;
            device.set_fan_speed(fan_index as u32, desired.speed)?;
        }
        Some(FanPolicy::Auto) => {
            device.set_fan_policy(fan_index as u32, FanPolicy::Auto)?;
        }
        None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FanCurve, FanCurvePoint};
    use crate::mock::MockManager;

    fn curve(speed: u8) -> crate::domain::FanCurve {
        FanCurve::new(
            vec![FanCurvePoint::new(40, FanSpeed::new(speed).unwrap())],
            FanSpeed::new(speed).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn applies_profile_and_restores_session_baseline() {
        let manager = MockManager::new(1);
        let profile = Profile::new("Performance").with_gpu_settings(
            "default",
            GpuSettings::new()
                .with_power_limit(PowerLimit::from_watts(350))
                .with_acoustic_limit(Temperature::new(75))
                .with_fan_curve(curve(80)),
        );
        let mut controller = ProfileController::new(&manager, &[0], false).unwrap();
        controller.apply_profile(&profile).unwrap();

        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.power_limit().unwrap().as_watts(), 350);
        assert_eq!(
            device.acoustic_limits().unwrap().current,
            Some(Temperature::new(75))
        );
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 80);
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Manual);

        controller.restore().unwrap();
        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.power_limit().unwrap().as_watts(), 300);
        assert_eq!(
            device.acoustic_limits().unwrap().current,
            Some(Temperature::new(80))
        );
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Auto);
    }

    #[test]
    fn switching_profiles_restores_omitted_settings() {
        let manager = MockManager::new(1);
        let performance = Profile::new("Performance").with_gpu_settings(
            "default",
            GpuSettings::new().with_power_limit(PowerLimit::from_watts(350)),
        );
        let quiet = Profile::new("Quiet").with_gpu_settings(
            "default",
            GpuSettings::new().with_acoustic_limit(Temperature::new(70)),
        );
        let mut controller = ProfileController::new(&manager, &[0], false).unwrap();
        controller.apply_profile(&performance).unwrap();
        controller.apply_profile(&quiet).unwrap();

        let device = manager.device_by_index(0).unwrap();
        assert_eq!(device.power_limit().unwrap().as_watts(), 300);
        assert_eq!(
            device.acoustic_limits().unwrap().current,
            Some(Temperature::new(70))
        );
    }

    #[test]
    fn dry_run_reports_without_mutating() {
        let manager = MockManager::new(1);
        let profile = Profile::new("Performance").with_gpu_settings(
            "default",
            GpuSettings::new().with_power_limit(PowerLimit::from_watts(350)),
        );
        let mut controller = ProfileController::new(&manager, &[0], true).unwrap();
        let report = controller.apply_profile(&profile).unwrap();
        assert!(!report.actions.is_empty());
        assert_eq!(
            manager
                .device_by_index(0)
                .unwrap()
                .power_limit()
                .unwrap()
                .as_watts(),
            300
        );
    }

    #[test]
    fn invalid_multi_gpu_profile_does_not_partially_apply() {
        let manager = MockManager::new(2);
        let profile = Profile::new("Bad")
            .with_gpu_settings(
                "GPU-MOCK-0000",
                GpuSettings::new().with_power_limit(PowerLimit::from_watts(350)),
            )
            .with_gpu_settings(
                "GPU-MOCK-0001",
                GpuSettings::new().with_power_limit(PowerLimit::from_watts(500)),
            );
        let mut controller = ProfileController::new(&manager, &[0, 1], false).unwrap();
        assert!(controller.apply_profile(&profile).is_err());
        assert_eq!(
            manager
                .device_by_index(0)
                .unwrap()
                .power_limit()
                .unwrap()
                .as_watts(),
            300
        );
    }

    #[test]
    fn fan_curve_tick_tracks_temperature_changes() {
        let manager = MockManager::new(1);
        let curve = FanCurve::new(
            vec![
                FanCurvePoint::new(40, FanSpeed::new(40).unwrap()),
                FanCurvePoint::new(70, FanSpeed::new(90).unwrap()),
            ],
            FanSpeed::new(30).unwrap(),
        )
        .unwrap();
        let profile = Profile::new("Dynamic")
            .with_gpu_settings("default", GpuSettings::new().with_fan_curve(curve));
        let mut controller = ProfileController::new(&manager, &[0], false).unwrap();
        controller.apply_profile(&profile).unwrap();
        let device = manager.device_by_index(0).unwrap();
        device.set_temperature(Temperature::new(75));
        controller.tick_fan_curves(&profile).unwrap();
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 90);
    }

    #[test]
    fn dropping_an_active_controller_restores_the_session_baseline() {
        let manager = MockManager::new(1);
        let profile = Profile::new("Performance").with_gpu_settings(
            "default",
            GpuSettings::new().with_power_limit(PowerLimit::from_watts(350)),
        );
        {
            let mut controller = ProfileController::new(&manager, &[0], false).unwrap();
            controller.apply_profile(&profile).unwrap();
            assert_eq!(
                manager
                    .device_by_index(0)
                    .unwrap()
                    .power_limit()
                    .unwrap()
                    .as_watts(),
                350
            );
        }
        assert_eq!(
            manager
                .device_by_index(0)
                .unwrap()
                .power_limit()
                .unwrap()
                .as_watts(),
            300
        );
    }

    #[test]
    fn topology_change_is_rejected_before_profile_mutation() {
        let manager = MockManager::new(1);
        let mut device = manager.device_by_index(0).unwrap();
        let baseline = capture_state(&device).unwrap();
        let mut current = baseline.clone();
        current.fans.pop();
        let settings = GpuSettings::new().with_power_limit(PowerLimit::from_watts(350));

        let error = reconcile_device(
            &mut device,
            &baseline,
            &current,
            Some(&settings),
            false,
            &mut Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(error, ProfileApplyError::State(_)));
        assert_eq!(device.power_limit().unwrap().as_watts(), 300);
    }

    #[test]
    fn topology_change_is_rejected_before_restore_mutation() {
        let manager = MockManager::new(1);
        let mut device = manager.device_by_index(0).unwrap();
        let mut desired = capture_state(&device).unwrap();
        desired.power_limit = Some(PowerLimit::from_watts(350));
        let mut current = capture_state(&device).unwrap();
        current.fans.pop();

        let error =
            restore_device(&mut device, &desired, &current, false, &mut Vec::new()).unwrap_err();

        assert!(matches!(error, ProfileApplyError::State(_)));
        assert_eq!(device.power_limit().unwrap().as_watts(), 300);
    }

    #[test]
    fn controller_can_capture_a_nonstandard_index_by_stable_uuid() {
        let manager = MockManager::with_devices(vec![crate::mock::MockDevice::new(7)]);
        let uuids = vec!["GPU-MOCK-0007".to_string()];
        let controller = ProfileController::new_by_uuids(&manager, &uuids, false).unwrap();

        assert_eq!(controller.target_uuids(), vec!["GPU-MOCK-0007"]);
    }
}
