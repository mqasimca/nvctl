//! NVML device implementation
//!
//! Real implementation of GpuDevice trait using nvml-wrapper.

use crate::domain::{
    AcousticLimits, FanPolicy, FanSpeed, GpuInfo, PowerConstraints, PowerLimit, Temperature,
    ThermalThresholds,
};
use crate::error::NvmlError;
use crate::nvml::traits::GpuDevice;

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Device;

// FFI constants for acoustic temperature thresholds
// These are not exposed by nvml-wrapper's high-level API
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN: u32 = 5;
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR: u32 = 6;
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX: u32 = 7;

/// NVML device wrapper implementing GpuDevice trait
pub struct NvmlDevice<'a> {
    device: Device<'a>,
    index: u32,
}

impl<'a> NvmlDevice<'a> {
    /// Create a new NVML device wrapper
    pub fn new(device: Device<'a>, index: u32) -> Self {
        Self { device, index }
    }

    /// Convert NVML error to our error type
    fn convert_error(err: nvml_wrapper::error::NvmlError) -> NvmlError {
        use nvml_wrapper::error::NvmlError as NE;
        match err {
            NE::NotSupported => {
                NvmlError::NotSupported("Operation not supported by this GPU".to_string())
            }
            NE::NoPermission => {
                NvmlError::InsufficientPermissions("Insufficient permissions".to_string())
            }
            NE::NotFound => NvmlError::DeviceNotFound(0),
            NE::GpuLost => NvmlError::GpuLost,
            NE::InvalidArg => NvmlError::InvalidArgument("Invalid argument".to_string()),
            _ => NvmlError::Unknown(err.to_string()),
        }
    }
}

impl GpuDevice for NvmlDevice<'_> {
    fn info(&self) -> Result<GpuInfo, NvmlError> {
        let name = self.name()?;
        let uuid = self.uuid()?;
        let fan_count = self.fan_count().unwrap_or(0);

        let mut info = GpuInfo::new(self.index, name, uuid).with_fan_count(fan_count);

        // Try to get optional info
        if let Ok(pci) = self.device.pci_info() {
            info = info.with_pci_bus_id(pci.bus_id);
        }

        Ok(info)
    }

    fn name(&self) -> Result<String, NvmlError> {
        self.device.name().map_err(Self::convert_error)
    }

    fn uuid(&self) -> Result<String, NvmlError> {
        self.device.uuid().map_err(Self::convert_error)
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn temperature(&self) -> Result<Temperature, NvmlError> {
        let temp = self
            .device
            .temperature(TemperatureSensor::Gpu)
            .map_err(Self::convert_error)?;
        Ok(Temperature::new(temp as i32))
    }

    fn thermal_thresholds(&self) -> Result<ThermalThresholds, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::TemperatureThreshold;

        let shutdown = self
            .device
            .temperature_threshold(TemperatureThreshold::Shutdown)
            .ok()
            .map(|t| Temperature::new(t as i32));

        let slowdown = self
            .device
            .temperature_threshold(TemperatureThreshold::Slowdown)
            .ok()
            .map(|t| Temperature::new(t as i32));

        let gpu_max = self
            .device
            .temperature_threshold(TemperatureThreshold::GpuMax)
            .ok()
            .map(|t| Temperature::new(t as i32));

        Ok(ThermalThresholds::new(shutdown, slowdown, gpu_max))
    }

    fn acoustic_limits(&self) -> Result<AcousticLimits, NvmlError> {
        // Use raw FFI to get acoustic thresholds
        // These are not exposed by nvml-wrapper's high-level API
        let handle = unsafe { self.device.handle() };

        let current =
            get_temperature_threshold_raw(handle, NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR)
                .ok()
                .map(Temperature::new);

        let min = get_temperature_threshold_raw(handle, NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN)
            .ok()
            .map(Temperature::new);

        let max = get_temperature_threshold_raw(handle, NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX)
            .ok()
            .map(Temperature::new);

        Ok(AcousticLimits::new(current, min, max))
    }

    fn set_acoustic_limit(&mut self, temp: Temperature) -> Result<(), NvmlError> {
        let handle = unsafe { self.device.handle() };
        set_temperature_threshold_raw(
            handle,
            NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR,
            temp.as_celsius(),
        )
    }

    fn fan_count(&self) -> Result<u32, NvmlError> {
        self.device.num_fans().map_err(Self::convert_error)
    }

    fn fan_speed(&self, fan_idx: u32) -> Result<FanSpeed, NvmlError> {
        let speed = self
            .device
            .fan_speed(fan_idx)
            .map_err(Self::convert_error)?;

        // Clamp to valid range (NVML might return > 100 in some edge cases)
        let clamped = speed.min(100) as u8;
        Ok(FanSpeed::new(clamped).expect("clamped value is always valid"))
    }

    fn set_fan_speed(&mut self, fan_idx: u32, speed: FanSpeed) -> Result<(), NvmlError> {
        self.device
            .set_fan_speed(fan_idx, speed.as_percentage() as u32)
            .map_err(Self::convert_error)
    }

    fn fan_policy(&self, fan_idx: u32) -> Result<FanPolicy, NvmlError> {
        use nvml_wrapper::enums::device::FanControlPolicy;

        let policy = self
            .device
            .fan_control_policy(fan_idx)
            .map_err(Self::convert_error)?;

        Ok(match policy {
            FanControlPolicy::TemperatureContinousSw => FanPolicy::Auto,
            FanControlPolicy::Manual => FanPolicy::Manual,
        })
    }

    fn set_fan_policy(&mut self, fan_idx: u32, policy: FanPolicy) -> Result<(), NvmlError> {
        use nvml_wrapper::enums::device::FanControlPolicy;

        let nvml_policy = match policy {
            FanPolicy::Auto => FanControlPolicy::TemperatureContinousSw,
            FanPolicy::Manual => FanControlPolicy::Manual,
        };

        self.device
            .set_fan_control_policy(fan_idx, nvml_policy)
            .map_err(Self::convert_error)
    }

    fn power_limit(&self) -> Result<PowerLimit, NvmlError> {
        let limit_mw = self
            .device
            .power_management_limit()
            .map_err(Self::convert_error)?;
        Ok(PowerLimit::from_milliwatts(limit_mw))
    }

    fn power_constraints(&self) -> Result<PowerConstraints, NvmlError> {
        let constraints = self
            .device
            .power_management_limit_constraints()
            .map_err(Self::convert_error)?;

        let default = self
            .device
            .power_management_limit_default()
            .map_err(Self::convert_error)?;

        Ok(PowerConstraints::new(
            PowerLimit::from_milliwatts(constraints.min_limit),
            PowerLimit::from_milliwatts(constraints.max_limit),
            PowerLimit::from_milliwatts(default),
        ))
    }

    fn set_power_limit(&mut self, limit: PowerLimit) -> Result<(), NvmlError> {
        self.device
            .set_power_management_limit(limit.as_milliwatts())
            .map_err(Self::convert_error)
    }

    fn power_usage(&self) -> Result<PowerLimit, NvmlError> {
        let usage_mw = self.device.power_usage().map_err(Self::convert_error)?;
        Ok(PowerLimit::from_milliwatts(usage_mw))
    }
}

/// Get temperature threshold using raw FFI
///
/// This bypasses nvml-wrapper to access thresholds not exposed by the high-level API.
fn get_temperature_threshold_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
    threshold_type: u32,
) -> Result<i32, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetThresholdFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        c_uint,
        *mut c_uint,
    ) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetThresholdFn> = unsafe { lib.get(b"nvmlDeviceGetTemperatureThreshold") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut temp: c_uint = 0;
    let result = unsafe { func(handle, threshold_type, &mut temp) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(temp as i32)
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Set temperature threshold using raw FFI
///
/// This bypasses nvml-wrapper to access thresholds not exposed by the high-level API.
fn set_temperature_threshold_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
    threshold_type: u32,
    temp: i32,
) -> Result<(), NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_int;

    type SetThresholdFn =
        unsafe extern "C" fn(nvml_wrapper_sys::bindings::nvmlDevice_t, u32, *mut c_int) -> u32;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<SetThresholdFn> = unsafe { lib.get(b"nvmlDeviceSetTemperatureThreshold") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut temp_val = temp as c_int;
    let result = unsafe { func(handle, threshold_type, &mut temp_val) };

    match result {
        x if x == nvmlReturn_enum_NVML_SUCCESS => Ok(()),
        2 => Err(NvmlError::NotSupported(
            "Acoustic temperature limit not supported on this GPU".to_string(),
        )),
        3 => Err(NvmlError::InsufficientPermissions(
            "Root privileges required to set temperature threshold".to_string(),
        )),
        code => Err(NvmlError::Unknown(format!("NVML error code: {}", code))),
    }
}
