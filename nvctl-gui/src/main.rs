//! nvctl-gui - GPU Control GUI
//!
//! A stunning GPU control application for Linux built with Iced.

mod app;
mod message;
mod services;
mod state;
mod theme;
mod views;
mod widgets;

use app::NvctlGui;
use clap::Parser;
use fs2::FileExt;
use iced::{window, Size};
use std::fs::{self, File};
use std::path::PathBuf;
use std::process;

/// nvctl-gui - GPU Control Application
#[derive(Parser, Debug)]
#[command(name = "nvctl-gui", version, about)]
struct Args {
    /// Run as daemon only (no GUI, applies fan curves in background)
    #[arg(short, long)]
    daemon: bool,

    /// Config file path for daemon mode
    #[arg(short, long)]
    config: Option<String>,
}

/// Get the lock file path
fn lock_file_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "nvctl")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("nvctl-gui.lock")
}

/// Try to acquire single-instance lock
/// Returns the lock file handle if successful (must be kept alive)
fn acquire_instance_lock() -> Option<File> {
    let lock_path = lock_file_path();

    // Ensure parent directory exists
    if let Some(parent) = lock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Open or create the lock file
    let file = match File::create(&lock_path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Failed to create lock file: {}", e);
            return None;
        }
    };

    // Try to acquire exclusive lock (non-blocking)
    match file.try_lock_exclusive() {
        Ok(()) => {
            log::debug!("Acquired instance lock at {:?}", lock_path);
            Some(file)
        }
        Err(_) => {
            log::info!("Another instance of nvctl-gui is already running");
            None
        }
    }
}

fn main() -> iced::Result {
    // Initialize logging with wgpu noise filtered out
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("wgpu_hal", log::LevelFilter::Error)
        .filter_module("wgpu_core", log::LevelFilter::Error)
        .init();

    // Parse CLI arguments
    let args = Args::parse();

    // Handle daemon mode
    if args.daemon {
        log::info!("Starting nvctl-gui in daemon mode");

        // Check for existing daemon instance
        let _lock = match acquire_instance_lock() {
            Some(lock) => lock,
            None => {
                eprintln!("Another instance of nvctl-gui is already running.");
                process::exit(1);
            }
        };

        // Run standalone daemon
        if let Err(e) = services::curve_daemon::run_daemon_standalone(args.config.as_deref()) {
            eprintln!("Daemon error: {}", e);
            process::exit(1);
        }
        return Ok(());
    }

    log::info!("Starting nvctl-gui");

    // Check for existing instance
    let _lock = match acquire_instance_lock() {
        Some(lock) => lock,
        None => {
            eprintln!("nvctl-gui is already running. Only one instance allowed.");
            process::exit(1);
        }
    };

    // Run the application
    // Note: _lock is kept alive for the duration of the app
    iced::application(NvctlGui::title, NvctlGui::update, NvctlGui::view)
        .subscription(NvctlGui::subscription)
        .theme(NvctlGui::theme)
        .window(window::Settings {
            size: Size::new(1200.0, 800.0),
            min_size: Some(Size::new(900.0, 600.0)),
            ..Default::default()
        })
        .antialiasing(true)
        .run_with(NvctlGui::new)
}
