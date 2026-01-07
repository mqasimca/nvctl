//! CLI argument definitions using clap derive
//!
//! Defines all command-line arguments and subcommands.

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// NVML-based GPU control tool
///
/// Control NVIDIA GPU fan speeds, power limits, and thermal settings.
#[derive(Parser, Debug)]
#[command(name = "nvctl")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Path to configuration file
    #[arg(short, long, global = true, env = "NVCTL_CONFIG")]
    pub config: Option<String>,

    /// Target GPU by index (0-based)
    #[arg(long, global = true)]
    pub gpu: Option<u32>,

    /// Target GPU by name (partial match)
    #[arg(long, global = true)]
    pub gpu_name: Option<String>,

    /// Target GPU by UUID
    #[arg(long, global = true)]
    pub gpu_uuid: Option<String>,

    /// Dry run mode - don't actually apply changes
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List all detected GPUs
    List,

    /// Show GPU information
    Info(InfoArgs),

    /// Control fan settings
    Fan(FanArgs),

    /// Control power settings
    Power(PowerArgs),

    /// Control thermal/acoustic settings
    Thermal(ThermalArgs),

    /// Start the control loop daemon
    Control(ControlArgs),

    /// Manage alert system
    Alerts(AlertArgs),

    /// Check GPU health status
    Health,

    /// List processes running on GPU
    Processes(ProcessesArgs),

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Arguments for the info command
#[derive(Parser, Debug)]
pub struct InfoArgs {
    /// Show all information
    #[arg(short, long)]
    pub all: bool,

    /// Show fan information
    #[arg(long)]
    pub fan: bool,

    /// Show power information
    #[arg(long)]
    pub power: bool,

    /// Show thermal information
    #[arg(long)]
    pub thermal: bool,

    /// Show ECC memory error information
    #[arg(long)]
    pub ecc: bool,

    /// Show PCIe metrics
    #[arg(long)]
    pub pcie: bool,

    /// Show memory temperature
    #[arg(long)]
    pub memory_temp: bool,

    /// Show video encoder/decoder utilization
    #[arg(long)]
    pub video: bool,
}

/// Arguments for fan control commands
#[derive(Parser, Debug)]
pub struct FanArgs {
    #[command(subcommand)]
    pub command: FanCommands,
}

/// Fan subcommands
#[derive(Subcommand, Debug)]
pub enum FanCommands {
    /// Show current fan status
    Status,

    /// Set fan control policy
    Policy {
        /// Policy to set
        #[arg(value_enum)]
        policy: FanPolicyArg,
    },

    /// Set fan speed (requires manual policy)
    Speed {
        /// Fan speed percentage (0-100)
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        speed: u8,

        /// Target specific fan index
        #[arg(long)]
        fan_index: Option<u32>,
    },
}

/// Fan policy argument
#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum FanPolicyArg {
    /// Automatic fan control (GPU-controlled)
    Auto,
    /// Manual fan control
    Manual,
}

/// Arguments for power control commands
#[derive(Parser, Debug)]
pub struct PowerArgs {
    #[command(subcommand)]
    pub command: PowerCommands,
}

/// Power subcommands
#[derive(Subcommand, Debug)]
pub enum PowerCommands {
    /// Show current power status
    Status,

    /// Set power limit
    Limit {
        /// Power limit in watts
        watts: u32,
    },
}

/// Arguments for thermal control commands
#[derive(Parser, Debug)]
pub struct ThermalArgs {
    #[command(subcommand)]
    pub command: ThermalCommands,
}

/// Thermal subcommands
#[derive(Subcommand, Debug)]
pub enum ThermalCommands {
    /// Show thermal thresholds and acoustic limits
    Status,

    /// Set acoustic temperature limit (GPU will throttle to maintain this temp)
    Limit {
        /// Target temperature in Celsius (GPU will throttle to stay at or below this)
        #[arg(value_parser = clap::value_parser!(i32).range(0..=100))]
        celsius: i32,
    },
}

/// Arguments for the processes command
#[derive(Parser, Debug)]
pub struct ProcessesArgs {
    /// Sort by memory usage (default)
    #[arg(short, long)]
    pub sort_memory: bool,

    /// Sort by PID
    #[arg(long)]
    pub sort_pid: bool,

    /// Show top N processes (by memory usage)
    #[arg(short = 'n', long)]
    pub top: Option<usize>,

    /// Filter by process type
    #[arg(short = 't', long, value_enum)]
    pub process_type: Option<ProcessTypeFilter>,
}

/// Process type filter for CLI
#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum ProcessTypeFilter {
    /// Graphics rendering processes
    Graphics,
    /// Compute/CUDA processes
    Compute,
    /// Both graphics and compute
    Both,
}

/// Arguments for the control loop command
#[derive(Parser, Debug)]
pub struct ControlArgs {
    /// Control loop interval in seconds
    #[arg(short, long, default_value = "5")]
    pub interval: u64,

    /// Run once and exit (single-use mode)
    #[arg(long)]
    pub single_use: bool,

    /// Enable retry on errors
    #[arg(long)]
    pub retry: bool,

    /// Retry interval in seconds
    #[arg(long, default_value = "10")]
    pub retry_interval: u64,

    /// Fan curve speed pairs (format: TEMP:SPEED, e.g., 60:50)
    #[arg(long = "speed-pair", value_name = "TEMP:SPEED")]
    pub speed_pairs: Vec<String>,

    /// Default fan speed (when below first curve point)
    #[arg(long, default_value = "30")]
    pub default_speed: u8,

    /// Power limit in watts (optional)
    #[arg(long)]
    pub power_limit: Option<u32>,
}

/// Arguments for alert commands
#[derive(Parser, Debug)]
pub struct AlertArgs {
    #[command(subcommand)]
    pub command: AlertCommands,
}

/// Alert subcommands
#[derive(Subcommand, Debug)]
pub enum AlertCommands {
    /// Start the alert monitoring daemon
    Start {
        /// Check interval in seconds
        #[arg(short, long, default_value = "5")]
        interval: u64,

        /// Path to alert configuration file
        #[arg(short, long)]
        config: Option<String>,

        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop the alert monitoring daemon
    Stop,

    /// List active alerts
    List {
        /// Show all alerts including resolved
        #[arg(short, long)]
        all: bool,

        /// Filter by severity
        #[arg(short, long)]
        severity: Option<String>,
    },

    /// List configured alert rules
    Rules {
        /// Path to alert configuration file
        #[arg(short, long)]
        config: Option<String>,
    },

    /// Acknowledge an alert
    Ack {
        /// Alert ID to acknowledge
        alert_id: String,
    },

    /// Silence an alert
    Silence {
        /// Alert ID to silence
        alert_id: String,
    },

    /// Clear all resolved alerts from history
    Clear,

    /// Test alert configuration
    Test {
        /// Path to alert configuration file
        #[arg(short, long)]
        config: Option<String>,
    },
}

/// Output format
#[derive(ValueEnum, Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    /// Human-readable table format
    #[default]
    Table,
    /// JSON format for machine parsing
    Json,
    /// Compact single-line format
    Compact,
}

/// Generate shell completions and print to stdout
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_list() {
        let args = Cli::try_parse_from(["nvctl", "list"]).unwrap();
        assert!(matches!(args.command, Commands::List));
    }

    #[test]
    fn test_cli_parse_verbose() {
        let args = Cli::try_parse_from(["nvctl", "-v", "list"]).unwrap();
        assert!(args.verbose);
    }

    #[test]
    fn test_cli_parse_gpu_selection() {
        let args = Cli::try_parse_from(["nvctl", "--gpu", "1", "list"]).unwrap();
        assert_eq!(args.gpu, Some(1));
    }

    #[test]
    fn test_cli_parse_fan_speed() {
        let args = Cli::try_parse_from(["nvctl", "fan", "speed", "75"]).unwrap();
        if let Commands::Fan(fan_args) = args.command {
            if let FanCommands::Speed { speed, .. } = fan_args.command {
                assert_eq!(speed, 75);
            } else {
                panic!("Expected Speed command");
            }
        } else {
            panic!("Expected Fan command");
        }
    }

    #[test]
    fn test_cli_fan_speed_validation() {
        // Should fail for > 100
        let result = Cli::try_parse_from(["nvctl", "fan", "speed", "150"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_parse_control_args() {
        let args = Cli::try_parse_from([
            "nvctl",
            "control",
            "--interval",
            "10",
            "--single-use",
            "--speed-pair",
            "60:50",
            "--speed-pair",
            "80:100",
        ])
        .unwrap();

        if let Commands::Control(ctrl) = args.command {
            assert_eq!(ctrl.interval, 10);
            assert!(ctrl.single_use);
            assert_eq!(ctrl.speed_pairs.len(), 2);
        } else {
            panic!("Expected Control command");
        }
    }

    #[test]
    fn test_cli_parse_thermal_limit() {
        let args = Cli::try_parse_from(["nvctl", "thermal", "limit", "75"]).unwrap();
        if let Commands::Thermal(thermal_args) = args.command {
            if let ThermalCommands::Limit { celsius } = thermal_args.command {
                assert_eq!(celsius, 75);
            } else {
                panic!("Expected Limit command");
            }
        } else {
            panic!("Expected Thermal command");
        }
    }

    #[test]
    fn test_cli_thermal_limit_validation() {
        // Should fail for > 100
        let result = Cli::try_parse_from(["nvctl", "thermal", "limit", "150"]);
        assert!(result.is_err());
    }
}
