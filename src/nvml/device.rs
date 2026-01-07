//! NVML device implementation
//!
//! Real implementation of GpuDevice trait using nvml-wrapper.

use crate::domain::{
    AcousticLimits, ClockSpeed, ClockType, CoolerTarget, DecoderUtilization, EccErrors, EccMode,
    EncoderUtilization, FanPolicy, FanSpeed, GpuInfo, MemoryInfo, PcieGeneration, PcieLinkStatus,
    PcieLinkWidth, PcieMetrics, PcieReplayCounter, PcieThroughput, PerformanceState,
    PowerConstraints, PowerLimit, ProcessList, Temperature, ThermalThresholds, ThrottleReasons,
    Utilization,
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

    fn cooler_target(&self, _fan_idx: u32) -> Result<CoolerTarget, NvmlError> {
        // Use raw FFI to get cooler info
        // Note: nvmlDeviceGetCoolerInfo doesn't take a fan index, it returns info for all coolers
        let handle = unsafe { self.device.handle() };
        get_cooler_target_raw(handle)
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

    fn clock_speed(&self, clock_type: ClockType) -> Result<ClockSpeed, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::Clock;

        let nvml_clock = match clock_type {
            ClockType::Graphics => Clock::Graphics,
            ClockType::SM => Clock::SM,
            ClockType::Memory => Clock::Memory,
            ClockType::Video => Clock::Video,
        };

        let speed = self
            .device
            .clock_info(nvml_clock)
            .map_err(Self::convert_error)?;

        Ok(ClockSpeed::new(speed))
    }

    fn utilization(&self) -> Result<Utilization, NvmlError> {
        let util = self
            .device
            .utilization_rates()
            .map_err(Self::convert_error)?;

        Ok(Utilization::new(util.gpu as u8, util.memory as u8))
    }

    fn memory_info(&self) -> Result<MemoryInfo, NvmlError> {
        let mem = self.device.memory_info().map_err(Self::convert_error)?;

        Ok(MemoryInfo::new(mem.total, mem.used, mem.free))
    }

    fn performance_state(&self) -> Result<PerformanceState, NvmlError> {
        let state = self
            .device
            .performance_state()
            .map_err(Self::convert_error)?;

        // nvml-wrapper returns PerformanceState enum, convert to our type
        Ok(PerformanceState::from_raw(state as u32))
    }

    fn throttle_reasons(&self) -> Result<ThrottleReasons, NvmlError> {
        let reasons = self
            .device
            .current_throttle_reasons()
            .map_err(Self::convert_error)?;

        // nvml-wrapper returns ThrottleReasons bitflags
        Ok(ThrottleReasons {
            idle: reasons.contains(nvml_wrapper::bitmasks::device::ThrottleReasons::GPU_IDLE),
            sw_power_cap: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::SW_POWER_CAP),
            hw_slowdown: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::HW_SLOWDOWN),
            sync_boost: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::SYNC_BOOST),
            sw_thermal: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::SW_THERMAL_SLOWDOWN),
            hw_thermal: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::HW_THERMAL_SLOWDOWN),
            hw_power_brake: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN),
            display_clocks: reasons
                .contains(nvml_wrapper::bitmasks::device::ThrottleReasons::DISPLAY_CLOCK_SETTING),
        })
    }

    fn memory_temperature(&self) -> Result<Option<Temperature>, NvmlError> {
        // Memory temperature sensor (NVML_TEMPERATURE_MEMORY = 1)
        // SAFETY: handle() is safe to call within the lifetime of the Device
        match get_memory_temperature_raw(unsafe { self.device.handle() }) {
            Ok(temp) => Ok(Some(Temperature::new(temp))),
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn encoder_utilization(&self) -> Result<Option<EncoderUtilization>, NvmlError> {
        // SAFETY: handle() is safe to call within the lifetime of the Device
        match get_encoder_utilization_raw(unsafe { self.device.handle() }) {
            Ok((utilization, sampling_period)) => {
                Ok(Some(EncoderUtilization::new(utilization, sampling_period)))
            }
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn decoder_utilization(&self) -> Result<Option<DecoderUtilization>, NvmlError> {
        // SAFETY: handle() is safe to call within the lifetime of the Device
        match get_decoder_utilization_raw(unsafe { self.device.handle() }) {
            Ok((utilization, sampling_period)) => {
                Ok(Some(DecoderUtilization::new(utilization, sampling_period)))
            }
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn ecc_mode(&self) -> Result<Option<EccMode>, NvmlError> {
        // Check if ECC is supported and get mode
        match self.device.is_ecc_enabled() {
            Ok(state) => {
                // state is EccModeState with currently_enabled (bool) field
                if state.currently_enabled {
                    Ok(Some(EccMode::Enabled))
                } else {
                    Ok(Some(EccMode::Disabled))
                }
            }
            Err(nvml_wrapper::error::NvmlError::NotSupported) => Ok(None),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    fn ecc_errors(&self) -> Result<Option<EccErrors>, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::{EccCounter, MemoryError};

        // Check if ECC is supported
        if self.ecc_mode()?.is_none() {
            return Ok(None);
        }

        // Get correctable errors
        let correctable_current = self
            .device
            .total_ecc_errors(MemoryError::Corrected, EccCounter::Volatile)
            .unwrap_or(0);

        let correctable_lifetime = self
            .device
            .total_ecc_errors(MemoryError::Corrected, EccCounter::Aggregate)
            .unwrap_or(0);

        // Get uncorrectable errors
        let uncorrectable_current = self
            .device
            .total_ecc_errors(MemoryError::Uncorrected, EccCounter::Volatile)
            .unwrap_or(0);

        let uncorrectable_lifetime = self
            .device
            .total_ecc_errors(MemoryError::Uncorrected, EccCounter::Aggregate)
            .unwrap_or(0);

        Ok(Some(EccErrors::new(
            correctable_current,
            correctable_lifetime,
            uncorrectable_current,
            uncorrectable_lifetime,
        )))
    }

    fn pcie_metrics(&self) -> Result<PcieMetrics, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::PcieUtilCounter;

        // TODO: Get PCIe generation and link width from NVML
        // nvml-wrapper doesn't expose these directly, need to use raw C API
        // For now, use defaults based on max values

        // Get max generation and width (available in nvml-wrapper)
        let max_gen = match self.device.max_pcie_link_gen() {
            Ok(gen) => match gen {
                1 => PcieGeneration::Gen1,
                2 => PcieGeneration::Gen2,
                3 => PcieGeneration::Gen3,
                4 => PcieGeneration::Gen4,
                5 => PcieGeneration::Gen5,
                6 => PcieGeneration::Gen6,
                _ => PcieGeneration::Gen4, // Default for modern GPUs
            },
            Err(_) => PcieGeneration::Gen4,
        };

        let max_width = match self.device.max_pcie_link_width() {
            Ok(width) => PcieLinkWidth::from_lanes(width as u8).unwrap_or(PcieLinkWidth::X16),
            Err(_) => PcieLinkWidth::X16,
        };

        // Get current generation and width from raw NVML C API
        // SAFETY: handle() is safe to call within the lifetime of the Device
        let current_gen = match get_current_pcie_generation_raw(unsafe { self.device.handle() }) {
            Ok(gen) => match gen {
                1 => PcieGeneration::Gen1,
                2 => PcieGeneration::Gen2,
                3 => PcieGeneration::Gen3,
                4 => PcieGeneration::Gen4,
                5 => PcieGeneration::Gen5,
                6 => PcieGeneration::Gen6,
                _ => max_gen, // Fallback to max
            },
            Err(_) => max_gen, // Fallback to max if unavailable
        };

        let current_width = match get_current_pcie_link_width_raw(unsafe { self.device.handle() }) {
            Ok(width) => PcieLinkWidth::from_lanes(width as u8).unwrap_or(max_width),
            Err(_) => max_width, // Fallback to max if unavailable
        };

        let link_status = PcieLinkStatus::new(current_gen, max_gen, current_width, max_width);

        // Get PCIe throughput
        let tx_bytes = self
            .device
            .pcie_throughput(PcieUtilCounter::Send)
            .unwrap_or(0) as u64
            * 1024; // KB/s to bytes/s

        let rx_bytes = self
            .device
            .pcie_throughput(PcieUtilCounter::Receive)
            .unwrap_or(0) as u64
            * 1024; // KB/s to bytes/s

        let throughput = PcieThroughput::new(tx_bytes, rx_bytes);

        // Get PCIe replay counter
        let replay_count =
            get_pcie_replay_counter_raw(unsafe { self.device.handle() }).unwrap_or(0);
        let replay_counter = PcieReplayCounter::new(replay_count);

        Ok(PcieMetrics::new(link_status, throughput, replay_counter))
    }

    fn running_processes(&self) -> Result<ProcessList, NvmlError> {
        use crate::domain::{GpuProcess, ProcessType};
        use std::collections::HashMap;

        // Get graphics processes
        // SAFETY: handle() is safe to call within the lifetime of the Device
        let graphics_processes = get_graphics_processes_raw(unsafe { self.device.handle() })?;

        // Get compute processes
        let compute_processes = get_compute_processes_raw(unsafe { self.device.handle() })?;

        // Merge processes by PID (some processes may be both graphics and compute)
        let mut process_map: HashMap<u32, GpuProcess> = HashMap::new();

        for process in graphics_processes {
            process_map.insert(process.pid, process);
        }

        for compute_process in compute_processes {
            if let Some(existing) = process_map.get_mut(&compute_process.pid) {
                // Process exists in both - mark as GraphicsCompute and sum memory
                existing.process_type = ProcessType::GraphicsCompute;
                existing.used_memory += compute_process.used_memory;
            } else {
                process_map.insert(compute_process.pid, compute_process);
            }
        }

        let mut processes: Vec<GpuProcess> = process_map.into_values().collect();

        // Try to get process names from /proc on Linux
        #[cfg(target_os = "linux")]
        for process in &mut processes {
            if let Ok(cmdline) = std::fs::read_to_string(format!("/proc/{}/cmdline", process.pid)) {
                // cmdline has null-separated args, take first one
                if let Some(name) = cmdline.split('\0').next() {
                    if !name.is_empty() {
                        // Extract just the executable name
                        let exe_name = std::path::Path::new(name)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(name);
                        process.name = Some(exe_name.to_string());
                    }
                }
            }
        }

        Ok(ProcessList::new(processes))
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

/// Get cooler target information using raw FFI
///
/// Returns what the cooler is designed to cool (GPU, Memory, Power Supply, etc.)
fn get_cooler_target_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<CoolerTarget, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    // nvmlCoolerInfo_t structure
    #[repr(C)]
    struct NvmlCoolerInfo {
        signal_type: c_uint,
        target: c_uint,
    }

    type GetCoolerInfoFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        *mut NvmlCoolerInfo,
    ) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetCoolerInfoFn> =
        unsafe { lib.get(b"nvmlDeviceGetCoolerInfo") }.map_err(|e| {
            NvmlError::NotSupported(format!("nvmlDeviceGetCoolerInfo not available: {}", e))
        })?;

    let mut cooler_info = NvmlCoolerInfo {
        signal_type: 0,
        target: 0,
    };

    let result = unsafe { func(handle, &mut cooler_info) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(CoolerTarget::from_raw(cooler_info.target))
    } else if result == 3 {
        // NVML_ERROR_NOT_SUPPORTED - function exists but not supported on this GPU
        // Return a sensible default
        Ok(CoolerTarget::All)
    } else {
        Err(NvmlError::Unknown(format!(
            "nvmlDeviceGetCoolerInfo error code: {}",
            result
        )))
    }
}

/// Get memory temperature using raw FFI
///
/// NVML_TEMPERATURE_MEMORY = 1
fn get_memory_temperature_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<i32, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetTemperatureFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        c_uint,
        *mut c_uint,
    ) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetTemperatureFn> = unsafe { lib.get(b"nvmlDeviceGetTemperature") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut temp: c_uint = 0;
    let result = unsafe { func(handle, 1, &mut temp) }; // 1 = NVML_TEMPERATURE_MEMORY

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(temp as i32)
    } else if result == 3 {
        // NVML_ERROR_NOT_SUPPORTED
        Err(NvmlError::NotSupported(
            "Memory temperature not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get encoder utilization using raw FFI
///
/// Returns (utilization_percent, sampling_period_us)
fn get_encoder_utilization_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<(u8, u32), NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetEncoderUtilFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        *mut c_uint,
        *mut c_uint,
    ) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetEncoderUtilFn> = unsafe { lib.get(b"nvmlDeviceGetEncoderUtilization") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut utilization: c_uint = 0;
    let mut sampling_period: c_uint = 0;
    let result = unsafe { func(handle, &mut utilization, &mut sampling_period) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok((utilization as u8, sampling_period))
    } else if result == 3 {
        Err(NvmlError::NotSupported(
            "Encoder utilization not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get decoder utilization using raw FFI
///
/// Returns (utilization_percent, sampling_period_us)
fn get_decoder_utilization_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<(u8, u32), NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetDecoderUtilFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        *mut c_uint,
        *mut c_uint,
    ) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetDecoderUtilFn> = unsafe { lib.get(b"nvmlDeviceGetDecoderUtilization") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut utilization: c_uint = 0;
    let mut sampling_period: c_uint = 0;
    let result = unsafe { func(handle, &mut utilization, &mut sampling_period) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok((utilization as u8, sampling_period))
    } else if result == 3 {
        Err(NvmlError::NotSupported(
            "Decoder utilization not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get current PCIe generation using raw FFI
fn get_current_pcie_generation_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<u32, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetPcieGenFn =
        unsafe extern "C" fn(nvml_wrapper_sys::bindings::nvmlDevice_t, *mut c_uint) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetPcieGenFn> = unsafe { lib.get(b"nvmlDeviceGetCurrPcieLinkGeneration") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut generation: c_uint = 0;
    let result = unsafe { func(handle, &mut generation) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(generation)
    } else if result == 3 {
        Err(NvmlError::NotSupported(
            "Current PCIe generation not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get current PCIe link width using raw FFI
fn get_current_pcie_link_width_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<u32, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetPcieLinkWidthFn =
        unsafe extern "C" fn(nvml_wrapper_sys::bindings::nvmlDevice_t, *mut c_uint) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetPcieLinkWidthFn> = unsafe { lib.get(b"nvmlDeviceGetCurrPcieLinkWidth") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut width: c_uint = 0;
    let result = unsafe { func(handle, &mut width) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(width)
    } else if result == 3 {
        Err(NvmlError::NotSupported(
            "Current PCIe link width not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get PCIe replay counter using raw FFI
fn get_pcie_replay_counter_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<u64, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::nvmlReturn_enum_NVML_SUCCESS;
    use std::os::raw::c_uint;

    type GetPcieReplayFn =
        unsafe extern "C" fn(nvml_wrapper_sys::bindings::nvmlDevice_t, *mut c_uint) -> c_uint;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetPcieReplayFn> = unsafe { lib.get(b"nvmlDeviceGetPcieReplayCounter") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    let mut counter: c_uint = 0;
    let result = unsafe { func(handle, &mut counter) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(counter as u64)
    } else if result == 3 {
        Err(NvmlError::NotSupported(
            "PCIe replay counter not supported on this GPU".to_string(),
        ))
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get running graphics processes using raw NVML C API
///
/// # Safety
/// This function uses unsafe FFI calls to the NVML library.
fn get_graphics_processes_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<Vec<crate::domain::GpuProcess>, NvmlError> {
    use crate::domain::{GpuProcess, ProcessType};
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::{nvmlDevice_t, nvmlReturn_enum, nvmlReturn_enum_NVML_SUCCESS};
    use std::mem;
    use std::os::raw::{c_uint, c_ulonglong};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct nvmlProcessInfo_t {
        pid: c_uint,
        used_gpu_memory: c_ulonglong,
    }

    // SAFETY: Loading NVML library
    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .or_else(|_| unsafe { Library::new("libnvidia-ml.so") })
        .map_err(|_e| NvmlError::LibraryNotFound)?;

    type GetGraphicsProcessesFn =
        unsafe extern "C" fn(nvmlDevice_t, *mut c_uint, *mut nvmlProcessInfo_t) -> nvmlReturn_enum;

    // SAFETY: Loading function symbol from library
    let func: Symbol<GetGraphicsProcessesFn> =
        unsafe { lib.get(b"nvmlDeviceGetGraphicsRunningProcesses\0") }
            .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    // First call to get count
    let mut count: c_uint = 0;
    let result = unsafe { func(handle, &mut count, std::ptr::null_mut()) };

    if result == 7 {
        // NVML_ERROR_INSUFFICIENT_SIZE - expected on first call
        // Allocate buffer
        let mut processes: Vec<nvmlProcessInfo_t> = vec![unsafe { mem::zeroed() }; count as usize];

        // Second call to get actual data
        let result = unsafe { func(handle, &mut count, processes.as_mut_ptr()) };

        if result == nvmlReturn_enum_NVML_SUCCESS {
            Ok(processes
                .into_iter()
                .take(count as usize)
                .map(|p| GpuProcess::new(p.pid, p.used_gpu_memory, ProcessType::Graphics))
                .collect())
        } else {
            Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
        }
    } else if result == nvmlReturn_enum_NVML_SUCCESS {
        // No processes
        Ok(Vec::new())
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}

/// Get running compute processes using raw NVML C API
///
/// # Safety
/// This function uses unsafe FFI calls to the NVML library.
fn get_compute_processes_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<Vec<crate::domain::GpuProcess>, NvmlError> {
    use crate::domain::{GpuProcess, ProcessType};
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::{nvmlDevice_t, nvmlReturn_enum, nvmlReturn_enum_NVML_SUCCESS};
    use std::mem;
    use std::os::raw::{c_uint, c_ulonglong};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct nvmlProcessInfo_t {
        pid: c_uint,
        used_gpu_memory: c_ulonglong,
    }

    // SAFETY: Loading NVML library
    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .or_else(|_| unsafe { Library::new("libnvidia-ml.so") })
        .map_err(|_e| NvmlError::LibraryNotFound)?;

    type GetComputeProcessesFn =
        unsafe extern "C" fn(nvmlDevice_t, *mut c_uint, *mut nvmlProcessInfo_t) -> nvmlReturn_enum;

    // SAFETY: Loading function symbol from library
    let func: Symbol<GetComputeProcessesFn> =
        unsafe { lib.get(b"nvmlDeviceGetComputeRunningProcesses\0") }
            .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    // First call to get count
    let mut count: c_uint = 0;
    let result = unsafe { func(handle, &mut count, std::ptr::null_mut()) };

    if result == 7 {
        // NVML_ERROR_INSUFFICIENT_SIZE - expected on first call
        // Allocate buffer
        let mut processes: Vec<nvmlProcessInfo_t> = vec![unsafe { mem::zeroed() }; count as usize];

        // Second call to get actual data
        let result = unsafe { func(handle, &mut count, processes.as_mut_ptr()) };

        if result == nvmlReturn_enum_NVML_SUCCESS {
            Ok(processes
                .into_iter()
                .take(count as usize)
                .map(|p| GpuProcess::new(p.pid, p.used_gpu_memory, ProcessType::Compute))
                .collect())
        } else {
            Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
        }
    } else if result == nvmlReturn_enum_NVML_SUCCESS {
        // No processes
        Ok(Vec::new())
    } else {
        Err(NvmlError::Unknown(format!("NVML error code: {}", result)))
    }
}
