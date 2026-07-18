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
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
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
/// Returns `Ok(None)` only when another process owns the lock.
fn acquire_instance_lock_at(lock_path: &Path) -> Result<Option<File>, String> {
    // Ensure parent directory exists
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create lock directory {}: {error}",
                parent.display()
            )
        })?;
    }

    // Do not truncate a lock file that another process may currently own.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| format!("Failed to open lock file {}: {error}", lock_path.display()))?;

    // Try to acquire exclusive lock (non-blocking)
    match file.try_lock_exclusive() {
        Ok(()) => {
            log::debug!("Acquired instance lock at {:?}", lock_path);
            Ok(Some(file))
        }
        Err(error) if error.kind() == ErrorKind::WouldBlock => {
            log::info!("Another instance of nvctl-gui is already running");
            Ok(None)
        }
        Err(error) => Err(format!("Failed to lock {}: {error}", lock_path.display())),
    }
}

/// Try to acquire the process-wide single-instance lock.
fn acquire_instance_lock() -> Result<Option<File>, String> {
    acquire_instance_lock_at(&lock_file_path())
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
            Ok(Some(lock)) => lock,
            Ok(None) => {
                eprintln!("Another instance of nvctl-gui is already running.");
                process::exit(1);
            }
            Err(error) => {
                eprintln!("Could not establish the daemon instance lock: {error}");
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
        Ok(Some(lock)) => lock,
        Ok(None) => {
            eprintln!("nvctl-gui is already running. Only one instance allowed.");
            process::exit(1);
        }
        Err(error) => {
            eprintln!("Could not establish the GUI instance lock: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_lock_distinguishes_contention_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.lock");

        let first = acquire_instance_lock_at(&path).unwrap().unwrap();
        assert!(acquire_instance_lock_at(&path).unwrap().is_none());
        drop(first);
        assert!(acquire_instance_lock_at(&path).unwrap().is_some());
    }
}
