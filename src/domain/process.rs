//! Process monitoring domain types
//!
//! Types for tracking GPU processes and their resource usage.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Information about a process using the GPU
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuProcess {
    /// Process ID
    pub pid: u32,
    /// Process name (if available)
    pub name: Option<String>,
    /// GPU memory used by this process (bytes)
    pub used_memory: u64,
    /// Process type (graphics, compute, both)
    pub process_type: ProcessType,
}

impl GpuProcess {
    /// Create a new GPU process
    pub fn new(pid: u32, used_memory: u64, process_type: ProcessType) -> Self {
        Self {
            pid,
            name: None,
            used_memory,
            process_type,
        }
    }

    /// Create a new GPU process with name
    pub fn with_name(pid: u32, name: String, used_memory: u64, process_type: ProcessType) -> Self {
        Self {
            pid,
            name: Some(name),
            used_memory,
            process_type,
        }
    }

    /// Get memory usage in MB
    pub fn memory_mb(&self) -> f64 {
        self.used_memory as f64 / 1024.0 / 1024.0
    }

    /// Get memory usage in GB
    pub fn memory_gb(&self) -> f64 {
        self.used_memory as f64 / 1024.0 / 1024.0 / 1024.0
    }

    /// Get display name (name or PID)
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Process {}", self.pid))
    }
}

impl fmt::Display for GpuProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PID {} ({}): {:.1} MB",
            self.pid,
            self.process_type,
            self.memory_mb()
        )
    }
}

/// Type of GPU process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProcessType {
    /// Graphics rendering process
    Graphics,
    /// Compute/CUDA process
    Compute,
    /// Both graphics and compute
    GraphicsCompute,
    /// Unknown type
    Unknown,
}

impl ProcessType {
    /// Check if this is a graphics process
    pub fn is_graphics(&self) -> bool {
        matches!(self, Self::Graphics | Self::GraphicsCompute)
    }

    /// Check if this is a compute process
    pub fn is_compute(&self) -> bool {
        matches!(self, Self::Compute | Self::GraphicsCompute)
    }
}

impl fmt::Display for ProcessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graphics => write!(f, "Graphics"),
            Self::Compute => write!(f, "Compute"),
            Self::GraphicsCompute => write!(f, "Graphics+Compute"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Summary of all processes using a GPU
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessList {
    /// List of processes
    pub processes: Vec<GpuProcess>,
    /// Total memory used by all processes (bytes)
    pub total_used_memory: u64,
}

impl ProcessList {
    /// Create a new process list
    pub fn new(processes: Vec<GpuProcess>) -> Self {
        let total_used_memory = processes.iter().map(|p| p.used_memory).sum();
        Self {
            processes,
            total_used_memory,
        }
    }

    /// Get number of processes
    pub fn count(&self) -> usize {
        self.processes.len()
    }

    /// Get total memory usage in MB
    pub fn total_memory_mb(&self) -> f64 {
        self.total_used_memory as f64 / 1024.0 / 1024.0
    }

    /// Get total memory usage in GB
    pub fn total_memory_gb(&self) -> f64 {
        self.total_used_memory as f64 / 1024.0 / 1024.0 / 1024.0
    }

    /// Get processes sorted by memory usage (descending)
    pub fn sorted_by_memory(&self) -> Vec<&GpuProcess> {
        let mut sorted: Vec<&GpuProcess> = self.processes.iter().collect();
        sorted.sort_by(|a, b| b.used_memory.cmp(&a.used_memory));
        sorted
    }

    /// Get top N processes by memory usage
    pub fn top_by_memory(&self, n: usize) -> Vec<&GpuProcess> {
        let sorted = self.sorted_by_memory();
        sorted.into_iter().take(n).collect()
    }

    /// Filter by process type
    pub fn filter_by_type(&self, process_type: ProcessType) -> Vec<&GpuProcess> {
        self.processes
            .iter()
            .filter(|p| p.process_type == process_type)
            .collect()
    }

    /// Get graphics processes
    pub fn graphics_processes(&self) -> Vec<&GpuProcess> {
        self.processes
            .iter()
            .filter(|p| p.process_type.is_graphics())
            .collect()
    }

    /// Get compute processes
    pub fn compute_processes(&self) -> Vec<&GpuProcess> {
        self.processes
            .iter()
            .filter(|p| p.process_type.is_compute())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_process_creation() {
        let process = GpuProcess::new(1234, 512 * 1024 * 1024, ProcessType::Graphics);
        assert_eq!(process.pid, 1234);
        assert_eq!(process.used_memory, 512 * 1024 * 1024);
        assert_eq!(process.process_type, ProcessType::Graphics);
        assert!(process.name.is_none());
    }

    #[test]
    fn test_gpu_process_with_name() {
        let process = GpuProcess::with_name(
            1234,
            "game.exe".to_string(),
            512 * 1024 * 1024,
            ProcessType::Graphics,
        );
        assert_eq!(process.name, Some("game.exe".to_string()));
        assert_eq!(process.display_name(), "game.exe");
    }

    #[test]
    fn test_gpu_process_memory_conversions() {
        let process = GpuProcess::new(1234, 1024 * 1024 * 1024, ProcessType::Compute); // 1 GB
        assert_eq!(process.memory_mb(), 1024.0);
        assert_eq!(process.memory_gb(), 1.0);
    }

    #[test]
    fn test_process_type_checks() {
        assert!(ProcessType::Graphics.is_graphics());
        assert!(!ProcessType::Graphics.is_compute());
        assert!(ProcessType::Compute.is_compute());
        assert!(!ProcessType::Compute.is_graphics());
        assert!(ProcessType::GraphicsCompute.is_graphics());
        assert!(ProcessType::GraphicsCompute.is_compute());
    }

    #[test]
    fn test_process_list_creation() {
        let processes = vec![
            GpuProcess::new(100, 512 * 1024 * 1024, ProcessType::Graphics),
            GpuProcess::new(200, 1024 * 1024 * 1024, ProcessType::Compute),
        ];
        let list = ProcessList::new(processes);
        assert_eq!(list.count(), 2);
        assert_eq!(
            list.total_used_memory,
            512 * 1024 * 1024 + 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_process_list_sorting() {
        let processes = vec![
            GpuProcess::new(100, 256 * 1024 * 1024, ProcessType::Graphics),
            GpuProcess::new(200, 1024 * 1024 * 1024, ProcessType::Compute),
            GpuProcess::new(300, 512 * 1024 * 1024, ProcessType::Graphics),
        ];
        let list = ProcessList::new(processes);
        let sorted = list.sorted_by_memory();

        assert_eq!(sorted[0].pid, 200); // 1024 MB
        assert_eq!(sorted[1].pid, 300); // 512 MB
        assert_eq!(sorted[2].pid, 100); // 256 MB
    }

    #[test]
    fn test_process_list_top() {
        let processes = vec![
            GpuProcess::new(100, 256 * 1024 * 1024, ProcessType::Graphics),
            GpuProcess::new(200, 1024 * 1024 * 1024, ProcessType::Compute),
            GpuProcess::new(300, 512 * 1024 * 1024, ProcessType::Graphics),
        ];
        let list = ProcessList::new(processes);
        let top2 = list.top_by_memory(2);

        assert_eq!(top2.len(), 2);
        assert_eq!(top2[0].pid, 200);
        assert_eq!(top2[1].pid, 300);
    }

    #[test]
    fn test_process_list_filter_by_type() {
        let processes = vec![
            GpuProcess::new(100, 256 * 1024 * 1024, ProcessType::Graphics),
            GpuProcess::new(200, 1024 * 1024 * 1024, ProcessType::Compute),
            GpuProcess::new(300, 512 * 1024 * 1024, ProcessType::Graphics),
        ];
        let list = ProcessList::new(processes);

        let graphics = list.graphics_processes();
        assert_eq!(graphics.len(), 2);

        let compute = list.compute_processes();
        assert_eq!(compute.len(), 1);
    }
}
