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

// Acoustic thresholds are not exposed by nvml-wrapper's high-level API.
// Re-export the generated values instead of duplicating NVML enum ordinals.
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN: u32 =
    nvml_wrapper_sys::bindings::nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN;
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR: u32 =
    nvml_wrapper_sys::bindings::nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR;
const NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX: u32 =
    nvml_wrapper_sys::bindings::nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX;

const fn nvml_struct_version<T>(version: u32) -> u32 {
    std::mem::size_of::<T>() as u32 | (version << 24)
}

fn raw_nvml_error(code: u32, operation: &str) -> NvmlError {
    use nvml_wrapper_sys::bindings::{
        nvmlReturn_enum_NVML_ERROR_GPU_IS_LOST, nvmlReturn_enum_NVML_ERROR_INVALID_ARGUMENT,
        nvmlReturn_enum_NVML_ERROR_NOT_SUPPORTED, nvmlReturn_enum_NVML_ERROR_NO_PERMISSION,
    };

    match code {
        value if value == nvmlReturn_enum_NVML_ERROR_INVALID_ARGUMENT => {
            NvmlError::InvalidArgument(format!("{} received an invalid argument", operation))
        }
        value if value == nvmlReturn_enum_NVML_ERROR_NOT_SUPPORTED => {
            NvmlError::NotSupported(format!("{} is not supported by this GPU", operation))
        }
        value if value == nvmlReturn_enum_NVML_ERROR_NO_PERMISSION => {
            NvmlError::InsufficientPermissions(format!(
                "Insufficient permissions for {}",
                operation
            ))
        }
        value if value == nvmlReturn_enum_NVML_ERROR_GPU_IS_LOST => NvmlError::GpuLost,
        _ => NvmlError::Unknown(format!(
            "{} failed with NVML error code {}",
            operation, code
        )),
    }
}

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
    pub(crate) fn convert_error(err: nvml_wrapper::error::NvmlError) -> NvmlError {
        use nvml_wrapper::error::NvmlError as NE;
        match err {
            NE::NotSupported => {
                NvmlError::NotSupported("Operation not supported by this GPU".to_string())
            }
            NE::NoPermission => {
                NvmlError::InsufficientPermissions("Insufficient permissions".to_string())
            }
            NE::NotFound => NvmlError::Unknown("NVML object not found".to_string()),
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
        let fan_count = self.fan_count()?;

        let mut info = GpuInfo::new(self.index, name, uuid).with_fan_count(fan_count);

        // Try to get optional info
        match self.device.pci_info().map_err(Self::convert_error) {
            Ok(pci) => info = info.with_pci_bus_id(pci.bus_id),
            Err(NvmlError::NotSupported(_)) => {}
            Err(error) => return Err(error),
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
        Ok(Temperature::from(temp))
    }

    fn thermal_thresholds(&self) -> Result<ThermalThresholds, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::TemperatureThreshold;

        let threshold = |kind| {
            optional_metric(
                self.device
                    .temperature_threshold(kind)
                    .map(Temperature::from)
                    .map_err(Self::convert_error),
            )
        };

        let shutdown = threshold(TemperatureThreshold::Shutdown)?;
        let slowdown = threshold(TemperatureThreshold::Slowdown)?;
        let gpu_max = threshold(TemperatureThreshold::GpuMax)?;

        Ok(ThermalThresholds::new(shutdown, slowdown, gpu_max))
    }

    fn acoustic_limits(&self) -> Result<AcousticLimits, NvmlError> {
        // Use raw FFI to get acoustic thresholds
        // These are not exposed by nvml-wrapper's high-level API
        let handle = unsafe { self.device.handle() };

        let current = optional_metric(get_temperature_threshold_raw(
            handle,
            NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR,
        ))?
        .map(Temperature::new);

        let min = optional_metric(get_temperature_threshold_raw(
            handle,
            NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN,
        ))?
        .map(Temperature::new);

        let max = optional_metric(get_temperature_threshold_raw(
            handle,
            NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX,
        ))?
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
        match self.device.num_fans() {
            Ok(count) => Ok(count),
            Err(nvml_wrapper::error::NvmlError::NotSupported) => Ok(0),
            Err(error) => Err(Self::convert_error(error)),
        }
    }

    fn fan_speed(&self, fan_idx: u32) -> Result<FanSpeed, NvmlError> {
        let speed = self
            .device
            .fan_speed(fan_idx)
            .map_err(Self::convert_error)?;

        // Clamp to valid range (NVML might return > 100 in some edge cases)
        let clamped = speed.min(100) as u8;
        Ok(FanSpeed::from_percentage_clamped(clamped))
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

    fn cooler_target(&self, fan_idx: u32) -> Result<CoolerTarget, NvmlError> {
        // Use raw FFI to get cooler info
        let handle = unsafe { self.device.handle() };
        get_cooler_target_raw(handle, fan_idx)
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

        let gpu = utilization_percentage(util.gpu, "GPU utilization")?;
        let memory = utilization_percentage(util.memory, "memory utilization")?;
        Ok(Utilization::new(gpu, memory))
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
            Ok((utilization, sampling_period)) => Ok(Some(EncoderUtilization::new(
                utilization_percentage(utilization, "encoder utilization")?,
                sampling_period,
            ))),
            Err(NvmlError::NotSupported(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn decoder_utilization(&self) -> Result<Option<DecoderUtilization>, NvmlError> {
        // SAFETY: handle() is safe to call within the lifetime of the Device
        match get_decoder_utilization_raw(unsafe { self.device.handle() }) {
            Ok((utilization, sampling_period)) => Ok(Some(DecoderUtilization::new(
                utilization_percentage(utilization, "decoder utilization")?,
                sampling_period,
            ))),
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

        // Counters are meaningful only when ECC is currently enabled.
        if !matches!(self.ecc_mode()?, Some(EccMode::Enabled)) {
            return Ok(None);
        }

        // Get correctable errors
        let correctable_current = self
            .device
            .total_ecc_errors(MemoryError::Corrected, EccCounter::Volatile)
            .map_err(Self::convert_error)?;

        let correctable_lifetime = self
            .device
            .total_ecc_errors(MemoryError::Corrected, EccCounter::Aggregate)
            .map_err(Self::convert_error)?;

        // Get uncorrectable errors
        let uncorrectable_current = self
            .device
            .total_ecc_errors(MemoryError::Uncorrected, EccCounter::Volatile)
            .map_err(Self::convert_error)?;

        let uncorrectable_lifetime = self
            .device
            .total_ecc_errors(MemoryError::Uncorrected, EccCounter::Aggregate)
            .map_err(Self::convert_error)?;

        Ok(Some(EccErrors::new(
            correctable_current,
            correctable_lifetime,
            uncorrectable_current,
            uncorrectable_lifetime,
        )))
    }

    fn pcie_metrics(&self) -> Result<PcieMetrics, NvmlError> {
        use nvml_wrapper::enum_wrappers::device::PcieUtilCounter;

        let max_gen = pcie_generation(
            self.device
                .max_pcie_link_gen()
                .map_err(Self::convert_error)?,
        )?;
        let max_width = pcie_width(
            self.device
                .max_pcie_link_width()
                .map_err(Self::convert_error)?,
        )?;

        // Get current generation and width from raw NVML C API
        // SAFETY: handle() is safe to call within the lifetime of the Device
        let current_gen = pcie_generation(get_current_pcie_generation_raw(unsafe {
            self.device.handle()
        })?)?;
        let current_width = pcie_width(get_current_pcie_link_width_raw(unsafe {
            self.device.handle()
        })?)?;

        let link_status = PcieLinkStatus::new(current_gen, max_gen, current_width, max_width);

        // Get PCIe throughput
        let tx_bytes = self
            .device
            .pcie_throughput(PcieUtilCounter::Send)
            .map_err(Self::convert_error)? as u64
            * 1024; // KB/s to bytes/s

        let rx_bytes = self
            .device
            .pcie_throughput(PcieUtilCounter::Receive)
            .map_err(Self::convert_error)? as u64
            * 1024; // KB/s to bytes/s

        let throughput = PcieThroughput::new(tx_bytes, rx_bytes);

        // Get PCIe replay counter
        let replay_count = get_pcie_replay_counter_raw(unsafe { self.device.handle() })?;
        let replay_counter = PcieReplayCounter::new(replay_count);

        Ok(PcieMetrics::new(link_status, throughput, replay_counter))
    }

    fn running_processes(&self) -> Result<ProcessList, NvmlError> {
        use crate::domain::{GpuProcess, ProcessType};
        use nvml_wrapper::enums::device::UsedGpuMemory;
        use std::collections::HashMap;

        let convert_process = |process: nvml_wrapper::struct_wrappers::device::ProcessInfo,
                               process_type| {
            let memory = match process.used_gpu_memory {
                UsedGpuMemory::Used(bytes) => Some(bytes),
                UsedGpuMemory::Unavailable => None,
            };
            GpuProcess::new(process.pid, memory, process_type)
        };

        let graphics_processes = self
            .device
            .running_graphics_processes()
            .map_err(Self::convert_error)?
            .into_iter()
            .map(|process| convert_process(process, ProcessType::Graphics));

        let compute_processes = self
            .device
            .running_compute_processes()
            .map_err(Self::convert_error)?
            .into_iter()
            .map(|process| convert_process(process, ProcessType::Compute));

        // Merge processes by PID (some processes may be both graphics and compute)
        let mut process_map: HashMap<u32, GpuProcess> = HashMap::new();

        for process in graphics_processes {
            process_map.insert(process.pid, process);
        }

        for compute_process in compute_processes {
            if let Some(existing) = process_map.get_mut(&compute_process.pid) {
                // Both APIs report the process's total GPU memory, so summing
                // them would double count it. Keep the best available value.
                existing.process_type = ProcessType::GraphicsCompute;
                existing.used_memory = match (existing.used_memory, compute_process.used_memory) {
                    (Some(graphics), Some(compute)) => Some(graphics.max(compute)),
                    (memory @ Some(_), None) | (None, memory @ Some(_)) => memory,
                    (None, None) => None,
                };
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

fn optional_metric<T>(result: Result<T, NvmlError>) -> Result<Option<T>, NvmlError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(NvmlError::NotSupported(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn utilization_percentage(value: u32, metric: &str) -> Result<u8, NvmlError> {
    if value > 100 {
        return Err(NvmlError::InvalidArgument(format!(
            "NVML returned invalid {metric} value {value}%"
        )));
    }
    Ok(value as u8)
}

fn pcie_generation(value: u32) -> Result<PcieGeneration, NvmlError> {
    match value {
        1 => Ok(PcieGeneration::Gen1),
        2 => Ok(PcieGeneration::Gen2),
        3 => Ok(PcieGeneration::Gen3),
        4 => Ok(PcieGeneration::Gen4),
        5 => Ok(PcieGeneration::Gen5),
        6 => Ok(PcieGeneration::Gen6),
        _ => Err(NvmlError::InvalidArgument(format!(
            "NVML returned invalid PCIe generation {value}"
        ))),
    }
}

fn pcie_width(value: u32) -> Result<PcieLinkWidth, NvmlError> {
    let lanes = u8::try_from(value).map_err(|_| {
        NvmlError::InvalidArgument(format!("NVML returned invalid PCIe link width {value}"))
    })?;
    PcieLinkWidth::from_lanes(lanes).map_err(|_| {
        NvmlError::InvalidArgument(format!("NVML returned invalid PCIe link width {value}"))
    })
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
        i32::try_from(temp).map_err(|_| {
            NvmlError::InvalidArgument(format!(
                "NVML returned an invalid temperature threshold {temp}"
            ))
        })
    } else {
        Err(raw_nvml_error(result, "reading a temperature threshold"))
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

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(())
    } else {
        Err(raw_nvml_error(
            result,
            "setting the acoustic temperature limit",
        ))
    }
}

/// Get cooler target information using raw FFI
///
/// Returns what the cooler is designed to cool (GPU, Memory, Power Supply, etc.)
fn get_cooler_target_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
    fan_idx: u32,
) -> Result<CoolerTarget, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::{
        nvmlCoolerInfo_t, nvmlReturn_enum, nvmlReturn_enum_NVML_SUCCESS,
    };

    type GetCoolerInfoFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        *mut nvmlCoolerInfo_t,
    ) -> nvmlReturn_enum;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetCoolerInfoFn> =
        unsafe { lib.get(b"nvmlDeviceGetCoolerInfo") }.map_err(|e| {
            NvmlError::NotSupported(format!("nvmlDeviceGetCoolerInfo not available: {}", e))
        })?;

    let mut cooler_info = nvmlCoolerInfo_t {
        version: nvml_struct_version::<nvmlCoolerInfo_t>(1),
        index: fan_idx,
        signalType: 0,
        target: 0,
    };

    let result = unsafe { func(handle, &mut cooler_info) };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(CoolerTarget::from_raw(cooler_info.target))
    } else {
        Err(raw_nvml_error(result, "reading cooler information"))
    }
}

/// Get memory temperature using raw FFI
fn get_memory_temperature_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<i32, NvmlError> {
    use libloading::{Library, Symbol};
    use nvml_wrapper_sys::bindings::{
        nvmlGpuThermalSettings_t, nvmlReturn_enum, nvmlReturn_enum_NVML_SUCCESS,
        nvmlThermalTarget_t_NVML_THERMAL_TARGET_ALL,
        nvmlThermalTarget_t_NVML_THERMAL_TARGET_MEMORY,
    };

    type GetThermalSettingsFn = unsafe extern "C" fn(
        nvml_wrapper_sys::bindings::nvmlDevice_t,
        u32,
        *mut nvmlGpuThermalSettings_t,
    ) -> nvmlReturn_enum;

    let lib = unsafe { Library::new("libnvidia-ml.so.1") }
        .map_err(|e| NvmlError::Unknown(format!("Failed to load NVML library: {}", e)))?;

    let func: Symbol<GetThermalSettingsFn> = unsafe { lib.get(b"nvmlDeviceGetThermalSettings") }
        .map_err(|e| NvmlError::NotSupported(format!("Function not available: {}", e)))?;

    // NVML returns up to NVML_MAX_THERMAL_SENSORS_PER_GPU entries. Asking for
    // the ALL target lets us identify the entry whose target is memory.
    let mut settings: nvmlGpuThermalSettings_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        func(
            handle,
            nvmlThermalTarget_t_NVML_THERMAL_TARGET_ALL as u32,
            &mut settings,
        )
    };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        settings
            .sensor
            .iter()
            .take(settings.count as usize)
            .find(|sensor| sensor.target == nvmlThermalTarget_t_NVML_THERMAL_TARGET_MEMORY)
            .map(|sensor| sensor.currentTemp)
            .filter(|temperature| *temperature != -1)
            .ok_or_else(|| {
                NvmlError::NotSupported("Memory temperature is not exposed by this GPU".to_string())
            })
    } else {
        Err(raw_nvml_error(result, "reading memory temperature"))
    }
}

/// Get encoder utilization using raw FFI
///
/// Returns (utilization_percent, sampling_period_us)
fn get_encoder_utilization_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<(u32, u32), NvmlError> {
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
        Ok((utilization, sampling_period))
    } else {
        Err(raw_nvml_error(result, "reading encoder utilization"))
    }
}

/// Get decoder utilization using raw FFI
///
/// Returns (utilization_percent, sampling_period_us)
fn get_decoder_utilization_raw(
    handle: nvml_wrapper_sys::bindings::nvmlDevice_t,
) -> Result<(u32, u32), NvmlError> {
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
        Ok((utilization, sampling_period))
    } else {
        Err(raw_nvml_error(result, "reading decoder utilization"))
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
    } else {
        Err(raw_nvml_error(result, "reading current PCIe generation"))
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
    } else {
        Err(raw_nvml_error(result, "reading current PCIe link width"))
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
    } else {
        Err(raw_nvml_error(result, "reading the PCIe replay counter"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvml_wrapper_sys::bindings::{
        nvmlCoolerInfo_t, nvmlReturn_enum_NVML_ERROR_INVALID_ARGUMENT,
        nvmlReturn_enum_NVML_ERROR_NOT_SUPPORTED, nvmlReturn_enum_NVML_ERROR_NO_PERMISSION,
    };

    #[test]
    fn acoustic_threshold_constants_match_nvml() {
        assert_eq!(NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN, 4);
        assert_eq!(NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR, 5);
        assert_eq!(NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX, 6);
    }

    #[test]
    fn cooler_info_uses_versioned_nvml_layout() {
        assert_eq!(std::mem::size_of::<nvmlCoolerInfo_t>(), 16);
        assert_eq!(nvml_struct_version::<nvmlCoolerInfo_t>(1), 0x0100_0010);
    }

    #[test]
    fn raw_nvml_errors_keep_their_meaning() {
        assert!(matches!(
            raw_nvml_error(nvmlReturn_enum_NVML_ERROR_INVALID_ARGUMENT, "test"),
            NvmlError::InvalidArgument(_)
        ));
        assert!(matches!(
            raw_nvml_error(nvmlReturn_enum_NVML_ERROR_NOT_SUPPORTED, "test"),
            NvmlError::NotSupported(_)
        ));
        assert!(matches!(
            raw_nvml_error(nvmlReturn_enum_NVML_ERROR_NO_PERMISSION, "test"),
            NvmlError::InsufficientPermissions(_)
        ));
    }
}
