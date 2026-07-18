//! nvctl - NVML-based GPU control tool
//!
//! A command-line tool for controlling NVIDIA GPU fan speeds, power limits,
//! and thermal settings.

use clap::Parser;
use nvctl::cli::args::{generate_completions, Cli, Commands};
use nvctl::commands::{
    run_alerts, run_control, run_fan, run_health, run_info, run_list, run_power, run_processes,
    run_thermal, GpuSelector,
};
use nvctl::config::{Config, ConfigBuilder};
use nvctl::error::AppError;

fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();
    let config_result = load_config(&cli);
    let verbose = config_result
        .as_ref()
        .map(|config| config.general.verbose)
        .unwrap_or(cli.verbose);

    // Configure the logger's actual filter before installing it. Raising only
    // `log`'s global maximum after initialization does not change env_logger's
    // own filter and used to make --verbose ineffective.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if verbose {
        "debug"
    } else {
        "warn"
    }))
    .format_timestamp(None)
    .init();

    let result = config_result.and_then(|config| run(&cli, &config));

    if let Err(e) = result {
        log::error!("{}", e);
        print_error(&e);
        std::process::exit(1);
    }
}

fn load_config(cli: &Cli) -> Result<Config, AppError> {
    // Alert subcommands use the global-looking `--config` flag for their own
    // alert configuration. Clap propagates global arguments into nested
    // subcommands. Alert commands do not consume the main nvctl configuration,
    // so bypass both the explicit path and default-file discovery; otherwise
    // an unrelated malformed application config can block alert management.
    if matches!(cli.command, Commands::Alerts(_)) {
        let config = ConfigBuilder::new()
            .with_verbose(cli.verbose.then_some(true))
            .with_dry_run(cli.dry_run.then_some(true))
            .build();
        config.validate()?;
        return Ok(config);
    }

    let config = ConfigBuilder::new()
        .with_file(cli.config.as_deref())?
        .with_verbose(cli.verbose.then_some(true))
        .with_dry_run(cli.dry_run.then_some(true))
        .with_gpu_selection(cli.gpu, cli.gpu_name.clone(), cli.gpu_uuid.clone())
        .build();
    config.validate()?;
    Ok(config)
}

fn run(cli: &Cli, config: &Config) -> Result<(), AppError> {
    let selector = GpuSelector::from_config(&config.gpu)?;

    match &cli.command {
        Commands::List => run_list(cli.format),

        Commands::Info(args) => run_info(args, cli.format, &selector),

        Commands::Fan(args) => run_fan(args, cli.format, &selector, config.general.dry_run),

        Commands::Power(args) => run_power(args, cli.format, &selector, config.general.dry_run),

        Commands::Thermal(args) => run_thermal(args, cli.format, &selector, config.general.dry_run),

        Commands::Control(args) => run_control(args, cli.format, &selector, config),

        Commands::Alerts(args) => run_alerts(&args.command, cli.format),

        Commands::Health => run_health(cli.format, &selector),

        Commands::Processes(args) => run_processes(args, cli.format, &selector),

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

#[cfg(test)]
mod tests {
    use super::*;
    use nvctl::cli::args::AlertCommands;

    #[test]
    fn alert_config_path_is_not_loaded_as_the_main_config() {
        let cli = Cli::try_parse_from([
            "nvctl",
            "alerts",
            "start",
            "--config",
            "/definitely/missing/nvctl-alerts.toml",
        ])
        .unwrap();

        let Commands::Alerts(args) = &cli.command else {
            panic!("expected alerts command");
        };
        let AlertCommands::Start { config, .. } = &args.command else {
            panic!("expected alerts start command");
        };
        assert_eq!(
            config.as_deref(),
            Some("/definitely/missing/nvctl-alerts.toml")
        );
        assert!(load_config(&cli).is_ok());
    }
}
