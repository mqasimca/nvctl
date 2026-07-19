//! Shared process-lifetime helpers for mutating command sessions.

use crate::error::{AppError, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) fn install_shutdown_handler() -> Result<Arc<AtomicBool>> {
    let running = Arc::new(AtomicBool::new(true));
    let running_for_signal = Arc::clone(&running);
    ctrlc::set_handler(move || running_for_signal.store(false, Ordering::SeqCst)).map_err(
        |error| {
            AppError::Io(std::io::Error::other(format!(
                "failed to install shutdown handler: {error}"
            )))
        },
    )?;
    Ok(running)
}
