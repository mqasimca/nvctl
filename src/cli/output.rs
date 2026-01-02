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
            let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
            writeln!(handle, "{}", json)?;
        }
        OutputFormat::Compact => {
            writeln!(handle, "{}", data.to_compact())?;
        }
    }

    Ok(())
}

/// Trait for types that can be displayed as a table
pub trait TableDisplay {
    /// Format as a table string
    fn to_table(&self) -> String;

    /// Format as a compact single line
    fn to_compact(&self) -> String {
        self.to_table().replace('\n', " | ")
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
}
