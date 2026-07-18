//! Output formatting utilities
//!
//! Provides table and JSON output formatting for CLI commands.

use crate::cli::args::OutputFormat;
use crate::domain::GpuInfo;
use serde::Serialize;
use std::io::{self, Write};

/// Format and print output based on the selected format
pub fn print_output<T: Serialize + TableDisplay>(data: &T, format: OutputFormat) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    match format {
        OutputFormat::Table => {
            writeln!(handle, "{}", data.to_table())?;
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut handle, data).map_err(json_io_error)?;
            writeln!(handle)?;
        }
        OutputFormat::Compact => {
            writeln!(handle, "{}", data.to_compact())?;
        }
    }

    Ok(())
}

/// Collects heterogeneous command records so JSON mode emits one valid document.
pub struct OutputWriter {
    format: OutputFormat,
    json_records: Vec<serde_json::Value>,
}

impl OutputWriter {
    /// Create an output writer for a command invocation.
    pub fn new(format: OutputFormat) -> Self {
        Self {
            format,
            json_records: Vec::new(),
        }
    }

    /// Write a record immediately for human formats or collect it for JSON.
    pub fn write<T: Serialize + TableDisplay>(&mut self, data: &T) -> io::Result<()> {
        match self.format {
            OutputFormat::Json => {
                self.json_records
                    .push(serde_json::to_value(data).map_err(json_io_error)?);
                Ok(())
            }
            format => print_output(data, format),
        }
    }

    /// Finish output, emitting exactly one JSON value in JSON mode.
    pub fn finish(self) -> io::Result<()> {
        if !matches!(self.format, OutputFormat::Json) {
            return Ok(());
        }

        let value = collected_json(self.json_records);
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        serde_json::to_writer_pretty(&mut handle, &value).map_err(json_io_error)?;
        writeln!(handle)
    }
}

fn collected_json(records: Vec<serde_json::Value>) -> serde_json::Value {
    if records.len() == 1 {
        records
            .into_iter()
            .next()
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Array(records)
    }
}

fn json_io_error(error: serde_json::Error) -> io::Error {
    io::Error::other(error)
}

/// Trait for types that can be displayed as a table
pub trait TableDisplay {
    /// Format as a table string
    fn to_table(&self) -> String;

    /// Format as a compact single line
    fn to_compact(&self) -> String {
        self.to_table().trim_end_matches('\n').replace('\n', " | ")
    }
}

/// GPU list entry for display
#[derive(Debug, Clone, Serialize)]
pub struct GpuListEntry {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    pub fans: u32,
}

impl From<&GpuInfo> for GpuListEntry {
    fn from(info: &GpuInfo) -> Self {
        Self {
            index: info.index,
            name: info.name.clone(),
            uuid: info.uuid.clone(),
            fans: info.fan_count,
        }
    }
}

impl TableDisplay for GpuListEntry {
    fn to_table(&self) -> String {
        format!(
            "[{}] {} (Fans: {}, UUID: {})",
            self.index, self.name, self.fans, self.uuid
        )
    }

    fn to_compact(&self) -> String {
        format!("{}:{}", self.index, self.name)
    }
}

/// GPU list for display
#[derive(Debug, Clone, Serialize)]
pub struct GpuList {
    pub gpus: Vec<GpuListEntry>,
    pub driver_version: String,
}

impl TableDisplay for GpuList {
    fn to_table(&self) -> String {
        let mut output = format!("Driver Version: {}\n", self.driver_version);
        output.push_str(&format!("GPUs Found: {}\n\n", self.gpus.len()));

        for gpu in &self.gpus {
            output.push_str(&gpu.to_table());
            output.push('\n');
        }

        output
    }

    fn to_compact(&self) -> String {
        self.gpus
            .iter()
            .map(|g| g.to_compact())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Fan status display
#[derive(Debug, Clone, Serialize)]
pub struct FanStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub fans: Vec<FanInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FanInfo {
    pub index: u32,
    pub speed: u8,
    pub policy: String,
}

impl TableDisplay for FanStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);

        for fan in &self.fans {
            output.push_str(&format!(
                "  Fan {}: {}% ({})\n",
                fan.index, fan.speed, fan.policy
            ));
        }

        output
    }
}

/// Power status display
#[derive(Debug, Clone, Serialize)]
pub struct PowerStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub current_usage_watts: u32,
    pub limit_watts: u32,
    pub min_limit_watts: u32,
    pub max_limit_watts: u32,
    pub default_limit_watts: u32,
}

impl TableDisplay for PowerStatus {
    fn to_table(&self) -> String {
        format!(
            "[{}] {}\n  Current Usage: {}W\n  Power Limit: {}W\n  Range: {}W - {}W\n  Default: {}W",
            self.gpu_index,
            self.gpu_name,
            self.current_usage_watts,
            self.limit_watts,
            self.min_limit_watts,
            self.max_limit_watts,
            self.default_limit_watts
        )
    }
}

/// Thermal status display
#[derive(Debug, Clone, Serialize)]
pub struct ThermalStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub current_temp: i32,
    pub shutdown_threshold: Option<i32>,
    pub slowdown_threshold: Option<i32>,
    pub max_threshold: Option<i32>,
}

impl TableDisplay for ThermalStatus {
    fn to_table(&self) -> String {
        let mut output = format!(
            "[{}] {}\n  Current Temperature: {}°C\n",
            self.gpu_index, self.gpu_name, self.current_temp
        );

        if let Some(t) = self.shutdown_threshold {
            output.push_str(&format!("  Shutdown Threshold: {}°C\n", t));
        }
        if let Some(t) = self.slowdown_threshold {
            output.push_str(&format!("  Slowdown Threshold: {}°C\n", t));
        }
        if let Some(t) = self.max_threshold {
            output.push_str(&format!("  Max Operating: {}°C\n", t));
        }

        output
    }
}

/// Acoustic limit status display
#[derive(Debug, Clone, Serialize)]
pub struct AcousticStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub current_temp: i32,
    pub acoustic_current: Option<i32>,
    pub acoustic_min: Option<i32>,
    pub acoustic_max: Option<i32>,
}

impl TableDisplay for AcousticStatus {
    fn to_table(&self) -> String {
        let mut output = format!(
            "[{}] {}\n  Current Temperature: {}°C\n",
            self.gpu_index, self.gpu_name, self.current_temp
        );

        output.push_str("  Acoustic Temperature Limit:\n");

        if let Some(t) = self.acoustic_current {
            output.push_str(&format!("    Current: {}°C\n", t));
        } else {
            output.push_str("    Current: Not supported\n");
        }

        if let (Some(min), Some(max)) = (self.acoustic_min, self.acoustic_max) {
            output.push_str(&format!("    Range: {}°C - {}°C\n", min, max));
        }

        output
    }
}

/// ECC memory error status display
#[derive(Debug, Clone, Serialize)]
pub struct EccStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub ecc_enabled: bool,
    pub correctable_current: Option<u64>,
    pub correctable_aggregate: Option<u64>,
    pub uncorrectable_current: Option<u64>,
    pub uncorrectable_aggregate: Option<u64>,
    pub health_status: Option<String>,
}

impl TableDisplay for EccStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);

        if !self.ecc_enabled {
            output.push_str("  ECC: Not Supported/Disabled\n");
            return output;
        }

        output.push_str("  ECC: Enabled\n");

        if let Some(count) = self.correctable_current {
            output.push_str(&format!("  Correctable Errors (Current Boot): {}\n", count));
        }
        if let Some(count) = self.correctable_aggregate {
            output.push_str(&format!("  Correctable Errors (Lifetime): {}\n", count));
        }
        if let Some(count) = self.uncorrectable_current {
            output.push_str(&format!(
                "  Uncorrectable Errors (Current Boot): {}\n",
                count
            ));
        }
        if let Some(count) = self.uncorrectable_aggregate {
            output.push_str(&format!("  Uncorrectable Errors (Lifetime): {}\n", count));
        }
        if let Some(health) = &self.health_status {
            output.push_str(&format!("  Health Status: {}\n", health));
        }

        output
    }
}

/// PCIe metrics display
#[derive(Debug, Clone, Serialize)]
pub struct PcieStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub current_gen: Option<String>,
    pub max_gen: Option<String>,
    pub current_width: Option<String>,
    pub max_width: Option<String>,
    pub tx_throughput_mbs: Option<f64>,
    pub rx_throughput_mbs: Option<f64>,
    pub replay_counter: Option<u64>,
    pub bandwidth_efficiency: Option<f64>,
}

impl TableDisplay for PcieStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);
        if let (Some(current_gen), Some(current_width), Some(max_gen), Some(max_width)) = (
            &self.current_gen,
            &self.current_width,
            &self.max_gen,
            &self.max_width,
        ) {
            output.push_str(&format!(
                "  PCIe Link: {} {} (Max: {} {})\n",
                current_gen, current_width, max_gen, max_width
            ));
        } else {
            output.push_str("  PCIe Metrics: Not Supported\n");
            return output;
        }

        if let (Some(tx), Some(rx)) = (self.tx_throughput_mbs, self.rx_throughput_mbs) {
            output.push_str(&format!(
                "  Throughput: TX {:.2} MB/s, RX {:.2} MB/s\n",
                tx, rx
            ));
        }

        if let Some(eff) = self.bandwidth_efficiency {
            output.push_str(&format!("  Bandwidth Efficiency: {:.1}%\n", eff));
        }

        if let Some(replay_counter) = self.replay_counter {
            output.push_str(&format!("  Replay Counter: {}\n", replay_counter));
        }

        output
    }
}

/// Memory temperature display
#[derive(Debug, Clone, Serialize)]
pub struct MemoryTempStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub gpu_temp: i32,
    pub memory_temp: Option<i32>,
}

impl TableDisplay for MemoryTempStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);
        output.push_str(&format!("  GPU Temperature: {}°C\n", self.gpu_temp));

        if let Some(temp) = self.memory_temp {
            output.push_str(&format!("  Memory Temperature: {}°C\n", temp));
        } else {
            output.push_str("  Memory Temperature: Not Supported\n");
        }

        output
    }
}

/// Video encoder/decoder utilization display
#[derive(Debug, Clone, Serialize)]
pub struct VideoStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub encoder_util: Option<u32>,
    pub decoder_util: Option<u32>,
}

impl TableDisplay for VideoStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);

        if let Some(util) = self.encoder_util {
            output.push_str(&format!("  Encoder Utilization: {}%\n", util));
        } else {
            output.push_str("  Encoder Utilization: Not Supported\n");
        }

        if let Some(util) = self.decoder_util {
            output.push_str(&format!("  Decoder Utilization: {}%\n", util));
        } else {
            output.push_str("  Decoder Utilization: Not Supported\n");
        }

        output
    }
}

/// GPU health status display
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub overall_score: u8,
    pub thermal_score: u8,
    pub power_score: u8,
    pub memory_score: u8,
    pub performance_score: u8,
    pub pcie_score: u8,
    pub status: String,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
    pub throttle_reasons: Option<String>,
}

impl TableDisplay for HealthStatus {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);
        output.push_str(&format!(
            "  Overall Health: {}/100 ({})\n\n",
            self.overall_score, self.status
        ));

        output.push_str("  Component Health Scores:\n");
        output.push_str(&format!("    Thermal:      {}/100\n", self.thermal_score));
        output.push_str(&format!("    Power:        {}/100\n", self.power_score));
        output.push_str(&format!("    Memory:       {}/100\n", self.memory_score));
        output.push_str(&format!(
            "    Performance:  {}/100\n",
            self.performance_score
        ));
        output.push_str(&format!("    PCIe:         {}/100\n", self.pcie_score));

        if !self.issues.is_empty() {
            output.push_str("\n  Issues Detected:\n");
            for issue in &self.issues {
                output.push_str(&format!("    • {}\n", issue));
            }
        }

        if !self.recommendations.is_empty() {
            output.push_str("\n  Recommendations:\n");
            for rec in &self.recommendations {
                output.push_str(&format!("    ✓ {}\n", rec));
            }
        }

        if let Some(throttle) = &self.throttle_reasons {
            output.push_str(&format!("\n  Throttling: {}\n", throttle));
        }

        output
    }
}

/// Process list output
#[derive(Debug, Clone, Serialize)]
pub struct ProcessListOutput {
    pub gpu_name: String,
    pub gpu_index: u32,
    pub process_count: usize,
    pub total_memory_mb: Option<f64>,
    pub total_memory_gb: Option<f64>,
    pub processes: Vec<ProcessEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub memory_mb: Option<f64>,
    pub memory_gb: Option<f64>,
    pub process_type: String,
}

impl TableDisplay for ProcessListOutput {
    fn to_table(&self) -> String {
        let mut output = format!("[{}] {}\n", self.gpu_index, self.gpu_name);
        let total_memory = self
            .total_memory_gb
            .map(|memory| format!("{memory:.2} GB"))
            .unwrap_or_else(|| "Unavailable".to_string());
        output.push_str(&format!(
            "  Processes: {} (Total Memory: {})\n\n",
            self.process_count, total_memory
        ));

        if self.processes.is_empty() {
            output.push_str("  No processes running on GPU\n");
            return output;
        }

        // Table header
        output.push_str("  PID      Memory      Type           Name\n");
        output.push_str("  ────────────────────────────────────────────────────────────\n");

        // Table rows
        for process in &self.processes {
            output.push_str(&format!(
                "  {:<8} {:<11} {:<14} {}\n",
                process.pid,
                process
                    .memory_mb
                    .map(|memory| format!("{memory:.1} MB"))
                    .unwrap_or_else(|| "N/A".to_string()),
                process.process_type,
                process.name
            ));
        }

        output
    }

    fn to_compact(&self) -> String {
        if self.processes.is_empty() {
            format!("GPU {}: No processes", self.gpu_index)
        } else {
            let total = self
                .total_memory_gb
                .map(|memory| format!("{memory:.2} GB"))
                .unwrap_or_else(|| "unknown memory".to_string());
            format!(
                "GPU {}: {} processes, {} total",
                self.gpu_index, self.process_count, total
            )
        }
    }
}

/// Simple message output
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub message: String,
    pub success: bool,
}

impl TableDisplay for Message {
    fn to_table(&self) -> String {
        if self.success {
            format!("✓ {}", self.message)
        } else {
            format!("✗ {}", self.message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_list_entry_table() {
        let entry = GpuListEntry {
            index: 0,
            name: "Test GPU".to_string(),
            uuid: "GPU-123".to_string(),
            fans: 2,
        };

        let output = entry.to_table();
        assert!(output.contains("Test GPU"));
        assert!(output.contains("GPU-123"));
    }

    #[test]
    fn test_message_display() {
        let msg = Message {
            message: "Operation completed".to_string(),
            success: true,
        };

        assert!(msg.to_table().starts_with('✓'));
    }

    #[test]
    fn collected_json_keeps_one_record_as_an_object() {
        let value = collected_json(vec![serde_json::json!({"gpu": 0})]);
        assert!(value.is_object());
    }

    #[test]
    fn collected_json_wraps_multiple_records_in_an_array() {
        let value = collected_json(vec![
            serde_json::json!({"gpu": 0}),
            serde_json::json!({"gpu": 1}),
        ]);
        assert_eq!(value.as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn pcie_width_is_not_prefixed_twice() {
        let status = PcieStatus {
            gpu_name: "Test GPU".to_string(),
            gpu_index: 0,
            current_gen: Some("Gen 4".to_string()),
            max_gen: Some("Gen 5".to_string()),
            current_width: Some("x16".to_string()),
            max_width: Some("x16".to_string()),
            tx_throughput_mbs: None,
            rx_throughput_mbs: None,
            replay_counter: None,
            bandwidth_efficiency: None,
        };

        let table = status.to_table();
        assert!(table.contains("Gen 4 x16"));
        assert!(!table.contains("xx16"));
    }

    #[test]
    fn default_compact_output_has_no_trailing_separator() {
        let status = FanStatus {
            gpu_name: "Test GPU".to_string(),
            gpu_index: 0,
            fans: vec![FanInfo {
                index: 0,
                speed: 50,
                policy: "Auto".to_string(),
            }],
        };

        let compact = status.to_compact();
        assert!(!compact.ends_with(" | "));
        assert!(!compact.contains('\n'));
    }
}
