//! Processes command implementation
//!
//! Lists processes running on GPU with memory usage.

use crate::cli::args::{OutputFormat, ProcessTypeFilter, ProcessesArgs};
use crate::cli::output::{print_output, ProcessEntry, ProcessListOutput};
use crate::domain::{GpuProcess, ProcessType};
use crate::error::Result;
use crate::nvml::{GpuDevice, GpuManager, NvmlManager};

/// Execute the processes command
pub fn run_processes(
    args: &ProcessesArgs,
    format: OutputFormat,
    gpu_index: Option<u32>,
) -> Result<()> {
    let manager = NvmlManager::new()?;

    // Determine which GPUs to check
    let indices: Vec<u32> = if let Some(idx) = gpu_index {
        vec![idx]
    } else {
        (0..manager.device_count()?).collect()
    };

    for &idx in &indices {
        let device = manager.device_by_index(idx)?;
        let info = device.info()?;
        let process_list = device.running_processes()?;

        // Filter by process type if specified
        let mut processes: Vec<&GpuProcess> = if let Some(filter) = args.process_type {
            match filter {
                ProcessTypeFilter::Graphics => process_list.graphics_processes(),
                ProcessTypeFilter::Compute => process_list.compute_processes(),
                ProcessTypeFilter::Both => process_list
                    .processes
                    .iter()
                    .filter(|p| p.process_type == ProcessType::GraphicsCompute)
                    .collect(),
            }
        } else {
            process_list.processes.iter().collect()
        };

        // Sort processes
        if args.sort_pid {
            processes.sort_by_key(|p| p.pid);
        } else {
            // Default: sort by memory (descending)
            processes.sort_by(|a, b| b.used_memory.cmp(&a.used_memory));
        }

        // Limit to top N if specified
        if let Some(n) = args.top {
            processes.truncate(n);
        }

        // Convert to output format
        let process_entries: Vec<ProcessEntry> = processes
            .iter()
            .map(|p| ProcessEntry {
                pid: p.pid,
                name: p.display_name(),
                memory_mb: p.memory_mb(),
                memory_gb: p.memory_gb(),
                process_type: p.process_type.to_string(),
            })
            .collect();

        let output = ProcessListOutput {
            gpu_name: info.name.clone(),
            gpu_index: idx,
            process_count: process_entries.len(),
            total_memory_mb: process_list.total_memory_mb(),
            total_memory_gb: process_list.total_memory_gb(),
            processes: process_entries,
        };

        print_output(&output, format)?;

        if indices.len() > 1 {
            println!(); // Separator between GPUs
        }
    }

    Ok(())
}
