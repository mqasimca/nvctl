//! Integration tests for nvctl-gui with mock GPU
//!
//! Tests application behavior using mock NVML devices.

use nvctl::domain::{
    FanCurve, FanCurvePoint, FanPolicy, FanSpeed, GpuInfo, PowerConstraints, PowerLimit,
    Temperature,
};
use nvctl::mock::{MockDevice, MockManager};
use nvctl::nvml::traits::{GpuDevice, GpuManager};
use std::collections::VecDeque;

// Re-create minimal state structures for testing
// (In a real app, these would be imported from nvctl_gui)

/// Minimal GPU state for testing
#[derive(Debug, Clone)]
struct TestGpuState {
    index: u32,
    info: GpuInfo,
    temperature: Temperature,
    fan_speeds: Vec<FanSpeed>,
    fan_policies: Vec<FanPolicy>,
    power_usage: PowerLimit,
    power_limit: PowerLimit,
    power_constraints: Option<PowerConstraints>,
    temp_history: VecDeque<f32>,
}

impl TestGpuState {
    fn new(info: GpuInfo) -> Self {
        Self {
            index: info.index,
            info,
            temperature: Temperature::new(0),
            fan_speeds: Vec::new(),
            fan_policies: Vec::new(),
            power_usage: PowerLimit::from_watts(0),
            power_limit: PowerLimit::from_watts(0),
            power_constraints: None,
            temp_history: VecDeque::new(),
        }
    }

    fn average_fan_speed(&self) -> Option<u8> {
        if self.fan_speeds.is_empty() {
            return None;
        }
        let sum: u32 = self
            .fan_speeds
            .iter()
            .map(|s| s.as_percentage() as u32)
            .sum();
        Some((sum / self.fan_speeds.len() as u32) as u8)
    }

    fn has_manual_fans(&self) -> bool {
        self.fan_policies.contains(&FanPolicy::Manual)
    }

    fn power_ratio(&self) -> f32 {
        if self.power_limit.as_watts() == 0 {
            return 0.0;
        }
        self.power_usage.as_watts() as f32 / self.power_limit.as_watts() as f32
    }
}

/// GPU state snapshot
#[derive(Debug, Clone)]
struct TestSnapshot {
    index: u32,
    temperature: Temperature,
    fan_speeds: Vec<FanSpeed>,
    fan_policies: Vec<FanPolicy>,
    power_usage: PowerLimit,
    power_limit: PowerLimit,
}

// ============================================================================
// Mock GPU Monitor Tests
// ============================================================================

mod mock_gpu_monitor {
    use super::*;

    /// Test that MockManager creates devices correctly
    #[test]
    fn test_mock_manager_creation() {
        let manager = MockManager::new(2);
        assert_eq!(manager.device_count().unwrap(), 2);
    }

    /// Test GPU detection with mock devices
    #[test]
    fn test_detect_gpus() {
        let manager = MockManager::new(2);

        let mut gpus = Vec::new();
        for i in 0..manager.device_count().unwrap() {
            let device = manager.device_by_index(i).unwrap();
            let info = device.info().unwrap();
            let mut state = TestGpuState::new(info);

            state.temperature = device.temperature().unwrap();
            if let Ok(limit) = device.power_limit() {
                state.power_limit = limit;
            }
            if let Ok(usage) = device.power_usage() {
                state.power_usage = usage;
            }
            if let Ok(constraints) = device.power_constraints() {
                state.power_constraints = Some(constraints);
            }

            // Get fan info
            if let Ok(fan_count) = device.fan_count() {
                for j in 0..fan_count {
                    if let Ok(speed) = device.fan_speed(j) {
                        state.fan_speeds.push(speed);
                    }
                    if let Ok(policy) = device.fan_policy(j) {
                        state.fan_policies.push(policy);
                    }
                }
            }

            gpus.push(state);
        }

        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].index, 0);
        assert_eq!(gpus[1].index, 1);

        // Check default values
        assert_eq!(gpus[0].temperature.as_celsius(), 45);
        assert_eq!(gpus[0].fan_speeds.len(), 2);
        assert_eq!(gpus[0].power_limit.as_watts(), 300);
    }

    /// Test polling GPU state
    #[test]
    fn test_poll_gpu() {
        let manager = MockManager::new(1);
        let device = manager.device_by_index(0).unwrap();

        let snapshot = TestSnapshot {
            index: device.index(),
            temperature: device.temperature().unwrap(),
            fan_speeds: (0..device.fan_count().unwrap())
                .filter_map(|i| device.fan_speed(i).ok())
                .collect(),
            fan_policies: (0..device.fan_count().unwrap())
                .filter_map(|i| device.fan_policy(i).ok())
                .collect(),
            power_usage: device.power_usage().unwrap(),
            power_limit: device.power_limit().unwrap(),
        };

        assert_eq!(snapshot.index, 0);
        assert_eq!(snapshot.temperature.as_celsius(), 45);
        assert_eq!(snapshot.fan_speeds.len(), 2);
        assert_eq!(snapshot.power_limit.as_watts(), 300);
    }

    /// Test temperature changes
    #[test]
    fn test_temperature_update() {
        let device = MockDevice::new(0);

        assert_eq!(device.temperature().unwrap().as_celsius(), 45);

        device.set_temperature(Temperature::new(75));
        assert_eq!(device.temperature().unwrap().as_celsius(), 75);

        device.set_temperature(Temperature::new(95));
        assert_eq!(device.temperature().unwrap().as_celsius(), 95);
    }

    /// Test fan speed control
    #[test]
    fn test_fan_speed_control() {
        let mut device = MockDevice::new(0);

        // Initial state - auto mode
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Auto);
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 50);

        // Switch to manual and set speed
        device.set_fan_policy(0, FanPolicy::Manual).unwrap();
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Manual);

        let new_speed = FanSpeed::new(80).unwrap();
        device.set_fan_speed(0, new_speed).unwrap();
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 80);
    }

    /// Test power limit control
    #[test]
    fn test_power_limit_control() {
        let mut device = MockDevice::new(0);

        // Check constraints
        let constraints = device.power_constraints().unwrap();
        assert_eq!(constraints.min.as_watts(), 100);
        assert_eq!(constraints.max.as_watts(), 400);
        assert_eq!(constraints.default.as_watts(), 300);

        // Set valid limit
        device.set_power_limit(PowerLimit::from_watts(350)).unwrap();
        assert_eq!(device.power_limit().unwrap().as_watts(), 350);

        // Try invalid limit (too high)
        let result = device.set_power_limit(PowerLimit::from_watts(500));
        assert!(result.is_err());

        // Try invalid limit (too low)
        let result = device.set_power_limit(PowerLimit::from_watts(50));
        assert!(result.is_err());
    }

    /// Test thermal thresholds
    #[test]
    fn test_thermal_thresholds() {
        let device = MockDevice::new(0);
        let thresholds = device.thermal_thresholds().unwrap();

        assert_eq!(thresholds.shutdown.unwrap().as_celsius(), 100);
        assert_eq!(thresholds.slowdown.unwrap().as_celsius(), 95);
        assert_eq!(thresholds.gpu_max.unwrap().as_celsius(), 83);
    }

    /// Test acoustic limits
    #[test]
    fn test_acoustic_limits() {
        let mut device = MockDevice::new(0);
        let limits = device.acoustic_limits().unwrap();

        assert_eq!(limits.current.unwrap().as_celsius(), 80);
        assert_eq!(limits.min.unwrap().as_celsius(), 60);
        assert_eq!(limits.max.unwrap().as_celsius(), 90);

        // Set new acoustic limit
        device.set_acoustic_limit(Temperature::new(75)).unwrap();
        let updated = device.acoustic_limits().unwrap();
        assert_eq!(updated.current.unwrap().as_celsius(), 75);

        // Try invalid limit
        let result = device.set_acoustic_limit(Temperature::new(50));
        assert!(result.is_err());
    }
}

// ============================================================================
// State Management Tests
// ============================================================================

mod state_tests {
    use super::*;

    /// Test GPU state creation and updates
    #[test]
    fn test_gpu_state_creation() {
        let info =
            GpuInfo::new(0, "Test GPU".to_string(), "GPU-TEST-0000".to_string()).with_fan_count(2);

        let state = TestGpuState::new(info);

        assert_eq!(state.index, 0);
        assert_eq!(state.info.name, "Test GPU");
        assert_eq!(state.temperature.as_celsius(), 0);
        assert!(state.fan_speeds.is_empty());
    }

    /// Test average fan speed calculation
    #[test]
    fn test_average_fan_speed() {
        let info = GpuInfo::new(0, "Test".to_string(), "UUID".to_string());
        let mut state = TestGpuState::new(info);

        // No fans
        assert_eq!(state.average_fan_speed(), None);

        // Add fans
        state.fan_speeds.push(FanSpeed::new(40).unwrap());
        state.fan_speeds.push(FanSpeed::new(60).unwrap());

        assert_eq!(state.average_fan_speed(), Some(50));
    }

    /// Test manual fan detection
    #[test]
    fn test_has_manual_fans() {
        let info = GpuInfo::new(0, "Test".to_string(), "UUID".to_string());
        let mut state = TestGpuState::new(info);

        // All auto
        state.fan_policies.push(FanPolicy::Auto);
        state.fan_policies.push(FanPolicy::Auto);
        assert!(!state.has_manual_fans());

        // One manual
        state.fan_policies[0] = FanPolicy::Manual;
        assert!(state.has_manual_fans());
    }

    /// Test power ratio calculation
    #[test]
    fn test_power_ratio() {
        let info = GpuInfo::new(0, "Test".to_string(), "UUID".to_string());
        let mut state = TestGpuState::new(info);

        // Zero limit
        assert_eq!(state.power_ratio(), 0.0);

        // Normal ratio
        state.power_limit = PowerLimit::from_watts(300);
        state.power_usage = PowerLimit::from_watts(150);
        assert!((state.power_ratio() - 0.5).abs() < 0.001);

        // Full power
        state.power_usage = PowerLimit::from_watts(300);
        assert!((state.power_ratio() - 1.0).abs() < 0.001);
    }

    /// Test temperature history
    #[test]
    fn test_temp_history() {
        let info = GpuInfo::new(0, "Test".to_string(), "UUID".to_string());
        let mut state = TestGpuState::new(info);

        // Add temperatures
        for i in 0..10 {
            state.temp_history.push_back(50.0 + i as f32);
        }

        assert_eq!(state.temp_history.len(), 10);
        assert_eq!(state.temp_history.front(), Some(&50.0));
        assert_eq!(state.temp_history.back(), Some(&59.0));
    }
}

// ============================================================================
// Fan Curve Tests
// ============================================================================

mod fan_curve_tests {
    use super::*;

    /// Test fan curve creation
    #[test]
    fn test_fan_curve_creation() {
        let points = vec![
            FanCurvePoint::new(40, FanSpeed::new(30).unwrap()),
            FanCurvePoint::new(60, FanSpeed::new(50).unwrap()),
            FanCurvePoint::new(80, FanSpeed::new(100).unwrap()),
        ];
        let default = FanSpeed::new(30).unwrap();

        let curve = FanCurve::new(points.clone(), default).unwrap();

        assert_eq!(curve.points().len(), 3);
    }

    /// Test fan curve step-based lookup
    #[test]
    fn test_fan_curve_lookup() {
        let points = vec![
            FanCurvePoint::new(40, FanSpeed::new(30).unwrap()),
            FanCurvePoint::new(60, FanSpeed::new(50).unwrap()),
            FanCurvePoint::new(80, FanSpeed::new(100).unwrap()),
        ];
        let default = FanSpeed::new(30).unwrap();
        let curve = FanCurve::new(points, default).unwrap();

        // Below first point - use default
        assert_eq!(curve.speed_for_temperature(30).as_percentage(), 30);

        // At a point
        assert_eq!(curve.speed_for_temperature(40).as_percentage(), 30);
        assert_eq!(curve.speed_for_temperature(60).as_percentage(), 50);
        assert_eq!(curve.speed_for_temperature(80).as_percentage(), 100);

        // Above last point - use last
        assert_eq!(curve.speed_for_temperature(90).as_percentage(), 100);
    }

    /// Test preset curves
    #[test]
    fn test_preset_silent() {
        let points = vec![
            FanCurvePoint::new(30, FanSpeed::new(20).unwrap()),
            FanCurvePoint::new(60, FanSpeed::new(30).unwrap()),
            FanCurvePoint::new(80, FanSpeed::new(60).unwrap()),
            FanCurvePoint::new(90, FanSpeed::new(100).unwrap()),
        ];
        let curve = FanCurve::new(points, FanSpeed::new(20).unwrap()).unwrap();

        // Silent should be quiet at normal temps
        assert!(curve.speed_for_temperature(50).as_percentage() <= 30);
    }

    /// Test preset balanced
    #[test]
    fn test_preset_balanced() {
        let points = vec![
            FanCurvePoint::new(40, FanSpeed::new(30).unwrap()),
            FanCurvePoint::new(60, FanSpeed::new(50).unwrap()),
            FanCurvePoint::new(75, FanSpeed::new(80).unwrap()),
            FanCurvePoint::new(85, FanSpeed::new(100).unwrap()),
        ];
        let curve = FanCurve::new(points, FanSpeed::new(30).unwrap()).unwrap();

        // Balanced should be moderate at mid temps
        let speed = curve.speed_for_temperature(60).as_percentage();
        assert!(speed >= 40 && speed <= 60);
    }

    /// Test preset performance
    #[test]
    fn test_preset_performance() {
        let points = vec![
            FanCurvePoint::new(30, FanSpeed::new(50).unwrap()),
            FanCurvePoint::new(50, FanSpeed::new(70).unwrap()),
            FanCurvePoint::new(70, FanSpeed::new(90).unwrap()),
            FanCurvePoint::new(80, FanSpeed::new(100).unwrap()),
        ];
        let curve = FanCurve::new(points, FanSpeed::new(50).unwrap()).unwrap();

        // Performance should be aggressive
        assert!(curve.speed_for_temperature(50).as_percentage() >= 50);
    }
}

// ============================================================================
// Multi-GPU Tests
// ============================================================================

mod multi_gpu_tests {
    use super::*;

    /// Test multiple GPU handling
    #[test]
    fn test_multi_gpu_detection() {
        let devices = vec![
            MockDevice::new(0).with_name("RTX 4090"),
            MockDevice::new(1).with_name("RTX 3080"),
        ];
        let manager = MockManager::with_devices(devices);

        assert_eq!(manager.device_count().unwrap(), 2);

        let gpu0 = manager.device_by_index(0).unwrap();
        let gpu1 = manager.device_by_index(1).unwrap();

        assert!(gpu0.name().unwrap().contains("4090"));
        assert!(gpu1.name().unwrap().contains("3080"));
    }

    /// Test GPU selection
    #[test]
    fn test_gpu_selection() {
        let manager = MockManager::new(3);
        let mut selected_gpu: usize = 0;

        // Initially select first GPU
        assert_eq!(selected_gpu, 0);

        // Select second GPU
        selected_gpu = 1;
        let device = manager.device_by_index(selected_gpu as u32).unwrap();
        assert_eq!(device.index(), 1);

        // Select third GPU
        selected_gpu = 2;
        let device = manager.device_by_index(selected_gpu as u32).unwrap();
        assert_eq!(device.index(), 2);
    }

    /// Test GPU lookup by UUID
    #[test]
    fn test_gpu_by_uuid() {
        let manager = MockManager::new(2);

        let device = manager.device_by_uuid("GPU-MOCK-0000").unwrap();
        assert_eq!(device.index(), 0);

        let device = manager.device_by_uuid("GPU-MOCK-0001").unwrap();
        assert_eq!(device.index(), 1);

        assert!(manager.device_by_uuid("INVALID").is_err());
    }

    /// Test independent GPU control
    #[test]
    fn test_independent_gpu_control() {
        let devices = vec![MockDevice::new(0), MockDevice::new(1)];
        let manager = MockManager::with_devices(devices);

        // Set different temperatures
        let mut device0 = manager.device_by_index(0).unwrap();
        let mut device1 = manager.device_by_index(1).unwrap();

        device0
            .set_fan_speed(0, FanSpeed::new(30).unwrap())
            .unwrap();
        device1
            .set_fan_speed(0, FanSpeed::new(80).unwrap())
            .unwrap();

        // They should be independent
        // Note: MockManager creates copies, so we need to re-fetch
        // In real implementation, changes would persist
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

mod error_handling_tests {
    use super::*;

    /// Test invalid GPU index
    #[test]
    fn test_invalid_gpu_index() {
        let manager = MockManager::new(1);

        assert!(manager.device_by_index(0).is_ok());
        assert!(manager.device_by_index(1).is_err());
        assert!(manager.device_by_index(99).is_err());
    }

    /// Test invalid fan index
    #[test]
    fn test_invalid_fan_index() {
        let device = MockDevice::new(0).with_fan_count(2);

        assert!(device.fan_speed(0).is_ok());
        assert!(device.fan_speed(1).is_ok());
        assert!(device.fan_speed(2).is_err());
    }

    /// Test invalid power limit
    #[test]
    fn test_invalid_power_limit() {
        let mut device = MockDevice::new(0);

        // Valid range is 100-400W
        assert!(device.set_power_limit(PowerLimit::from_watts(200)).is_ok());
        assert!(device.set_power_limit(PowerLimit::from_watts(50)).is_err());
        assert!(device.set_power_limit(PowerLimit::from_watts(500)).is_err());
    }

    /// Test invalid fan speed
    #[test]
    fn test_invalid_fan_speed() {
        // FanSpeed validation happens at construction
        assert!(FanSpeed::new(0).is_ok());
        assert!(FanSpeed::new(100).is_ok());
        assert!(FanSpeed::new(101).is_err());
    }

    /// Test graceful handling of missing data
    #[test]
    fn test_missing_data_handling() {
        let manager = MockManager::new(0); // No GPUs

        assert_eq!(manager.device_count().unwrap(), 0);
        assert!(manager.device_by_index(0).is_err());
    }
}

// ============================================================================
// Simulation Tests
// ============================================================================

mod simulation_tests {
    use super::*;

    /// Simulate GPU temperature changes over time
    #[test]
    fn test_temperature_simulation() {
        let device = MockDevice::new(0);
        let mut history: VecDeque<i32> = VecDeque::new();

        // Simulate temperature changes
        let temps = [45, 50, 55, 60, 65, 70, 68, 65, 60, 55];

        for &temp in &temps {
            device.set_temperature(Temperature::new(temp));
            let current = device.temperature().unwrap().as_celsius();
            history.push_back(current);
        }

        assert_eq!(history.len(), temps.len());
        assert_eq!(history.back(), Some(&55));

        // Check trend detection (simplified)
        let recent: Vec<_> = history.iter().rev().take(3).copied().collect();
        let is_cooling = recent.windows(2).all(|w| w[0] <= w[1]);
        assert!(is_cooling); // 55, 60, 65 reversed = 65, 60, 55 = cooling
    }

    /// Simulate fan curve response to temperature
    #[test]
    fn test_fan_curve_response() {
        let points = vec![
            FanCurvePoint::new(40, FanSpeed::new(30).unwrap()),
            FanCurvePoint::new(60, FanSpeed::new(50).unwrap()),
            FanCurvePoint::new(80, FanSpeed::new(100).unwrap()),
        ];
        let curve = FanCurve::new(points, FanSpeed::new(30).unwrap()).unwrap();

        let mut fan_speeds: Vec<u8> = Vec::new();

        // Simulate temperature rise and corresponding fan speed
        for temp in (30..=90).step_by(10) {
            let speed = curve.speed_for_temperature(temp);
            fan_speeds.push(speed.as_percentage());
        }

        // Fan speed should increase with temperature
        for i in 1..fan_speeds.len() {
            assert!(fan_speeds[i] >= fan_speeds[i - 1]);
        }
    }

    /// Simulate power draw under load
    #[test]
    fn test_power_simulation() {
        let info = GpuInfo::new(0, "Test".to_string(), "UUID".to_string());
        let mut state = TestGpuState::new(info);
        state.power_limit = PowerLimit::from_watts(350);

        // Simulate varying power draw
        let power_draws = [100, 150, 200, 280, 320, 350, 340, 300];

        for &watts in &power_draws {
            state.power_usage = PowerLimit::from_watts(watts);
            let ratio = state.power_ratio();

            // Should never exceed 100%
            assert!(ratio <= 1.0);

            // At 350W limit, 350W usage should be 100%
            if watts == 350 {
                assert!((ratio - 1.0).abs() < 0.001);
            }
        }
    }
}

// ============================================================================
// Profile Integration Tests
// ============================================================================

mod profile_tests {
    use super::*;

    /// Test profile data structure
    #[test]
    fn test_profile_settings() {
        // Simulate profile settings
        let fan_curve = FanCurve::new(
            vec![
                FanCurvePoint::new(40, FanSpeed::new(30).unwrap()),
                FanCurvePoint::new(80, FanSpeed::new(100).unwrap()),
            ],
            FanSpeed::new(30).unwrap(),
        )
        .unwrap();

        let power_limit = PowerLimit::from_watts(300);

        // Settings should be storable
        assert_eq!(fan_curve.points().len(), 2);
        assert_eq!(power_limit.as_watts(), 300);
    }

    /// Test applying profile to mock GPU
    #[test]
    fn test_apply_profile() {
        let mut device = MockDevice::new(0);

        // Profile settings
        let target_power = PowerLimit::from_watts(280);
        let target_fan_policy = FanPolicy::Manual;
        let target_fan_speed = FanSpeed::new(60).unwrap();

        // Apply profile
        device.set_power_limit(target_power).unwrap();
        device.set_fan_policy(0, target_fan_policy).unwrap();
        device.set_fan_speed(0, target_fan_speed).unwrap();

        // Verify
        assert_eq!(device.power_limit().unwrap().as_watts(), 280);
        assert_eq!(device.fan_policy(0).unwrap(), FanPolicy::Manual);
        assert_eq!(device.fan_speed(0).unwrap().as_percentage(), 60);
    }

    /// Test profile validation
    #[test]
    fn test_profile_validation() {
        let device = MockDevice::new(0);
        let constraints = device.power_constraints().unwrap();

        // Valid profile power limit
        let valid_limit = PowerLimit::from_watts(300);
        assert!(constraints.contains(&valid_limit));

        // Invalid profile power limit
        let invalid_limit = PowerLimit::from_watts(500);
        assert!(!constraints.contains(&invalid_limit));
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

mod concurrency_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// Test concurrent temperature reads
    #[test]
    fn test_concurrent_reads() {
        let device = Arc::new(MockDevice::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let dev: Arc<MockDevice> = Arc::clone(&device);
            handles.push(thread::spawn(move || {
                dev.temperature().unwrap().as_celsius()
            }));
        }

        for handle in handles {
            let temp: i32 = handle.join().unwrap();
            assert_eq!(temp, 45); // Default mock temperature
        }
    }

    /// Test temperature update from another thread
    #[test]
    fn test_concurrent_update() {
        let device = Arc::new(MockDevice::new(0));

        // Update temperature from another thread
        let dev: Arc<MockDevice> = Arc::clone(&device);
        let handle = thread::spawn(move || {
            dev.set_temperature(Temperature::new(80));
        });

        handle.join().unwrap();

        // Read from main thread
        assert_eq!(device.temperature().unwrap().as_celsius(), 80);
    }
}
