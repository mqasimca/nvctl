//! nvctl - NVML-based GPU control tool
//!
//! A command-line tool for controlling NVIDIA GPU fan speeds, power limits,
//! and thermal settings.

use clap::Parser;
use nvctl::cli::args::{generate_completions, Cli, Commands};
use nvctl::commands::{run_control, run_fan, run_info, run_list, run_power, run_thermal};
use nvctl::error::AppError;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Set log level based on verbose flag
    if cli.verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }

    // Run the appropriate command
    let result = run(&cli);

    if let Err(e) = result {
        log::error!("{}", e);
        print_error(&e);
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<(), AppError> {
    match &cli.command {
        Commands::List => run_list(cli.format),

        Commands::Info(args) => run_info(args, cli.format, cli.gpu),

        Commands::Fan(args) => run_fan(args, cli.format, cli.gpu, cli.dry_run),

        Commands::Power(args) => run_power(args, cli.format, cli.gpu, cli.dry_run),

        Commands::Thermal(args) => run_thermal(args, cli.format, cli.gpu, cli.dry_run),

        Commands::Control(args) => run_control(args, cli.format, cli.gpu, cli.dry_run, cli.verbose),

        Commands::Completions { shell } => {
            generate_completions(*shell);
            Ok(())
        }
    }
}

fn print_error(err: &AppError) {
    eprintln!("Error: {}", err);

    // Print helpful hints for common errors
    match err {
        AppError::Nvml(nvctl::error::NvmlError::LibraryNotFound) => {
            eprintln!();
            eprintln!("Hint: Make sure the NVIDIA driver is installed.");
            eprintln!("      On Linux, install the nvidia-utils package.");
        }
        AppError::Nvml(nvctl::error::NvmlError::InsufficientPermissions(_)) => {
            eprintln!();
            eprintln!("Hint: Try running with sudo or as root.");
        }
        AppError::NoGpusFound => {
            eprintln!();
            eprintln!("Hint: Make sure you have an NVIDIA GPU installed.");
            eprintln!("      Check 'nvidia-smi' for GPU detection.");
        }
        _ => {}
    }
}
