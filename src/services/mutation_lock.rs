//! Cross-process ownership locks for commands that mutate GPU state.

use fs2::FileExt;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Failure to establish exclusive ownership of a GPU mutation session.
#[derive(Debug, Error)]
pub enum MutationLockError {
    #[error("GPU {gpu_uuid} is already controlled by another nvctl process")]
    Busy { gpu_uuid: String },
    #[error("failed to manage GPU mutation locks: {0}")]
    Io(#[from] io::Error),
}

/// Holds exclusive advisory locks for a sorted set of stable GPU UUIDs.
pub struct GpuMutationGuard {
    files: Vec<File>,
}

impl GpuMutationGuard {
    /// Claim every selected GPU for the duration of a mutating command.
    pub fn acquire(gpu_uuids: &[String], owner: &str) -> Result<Self, MutationLockError> {
        Self::acquire_in(&lock_directory(), gpu_uuids, owner)
    }

    fn acquire_in(
        directory: &Path,
        gpu_uuids: &[String],
        owner: &str,
    ) -> Result<Self, MutationLockError> {
        fs::create_dir_all(directory)?;
        let mut sorted_uuids = gpu_uuids.to_vec();
        sorted_uuids.sort_unstable();
        sorted_uuids.dedup();

        let mut files = Vec::with_capacity(sorted_uuids.len());
        for gpu_uuid in sorted_uuids {
            let path = directory.join(lock_filename(&gpu_uuid));
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?;
            match file.try_lock_exclusive() {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Err(MutationLockError::Busy { gpu_uuid });
                }
                Err(error) => return Err(error.into()),
            }

            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            writeln!(file, "pid={}", std::process::id())?;
            writeln!(file, "owner={owner}")?;
            file.sync_data()?;
            files.push(file);
        }
        Ok(Self { files })
    }
}

impl Drop for GpuMutationGuard {
    fn drop(&mut self) {
        for file in self.files.iter().rev() {
            if let Err(error) = FileExt::unlock(file) {
                log::error!("Failed to release a GPU mutation lock: {error}");
            }
        }
    }
}

fn lock_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nvctl")
        .join("locks")
}

fn lock_filename(gpu_uuid: &str) -> String {
    let mut filename = String::from("gpu-");
    for byte in gpu_uuid.as_bytes() {
        let _ = write!(filename, "{byte:02x}");
    }
    filename.push_str(".lock");
    filename
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_contention_and_releases_on_drop() {
        let directory = tempdir().unwrap();
        let uuids = vec!["GPU-B".to_string(), "GPU-A".to_string()];
        let first = GpuMutationGuard::acquire_in(directory.path(), &uuids, "first").unwrap();

        assert!(matches!(
            GpuMutationGuard::acquire_in(
                directory.path(),
                &["GPU-A".to_string()],
                "second"
            ),
            Err(MutationLockError::Busy { gpu_uuid }) if gpu_uuid == "GPU-A"
        ));

        drop(first);
        GpuMutationGuard::acquire_in(directory.path(), &uuids, "second").unwrap();
    }

    #[test]
    fn duplicate_uuids_are_locked_once() {
        let directory = tempdir().unwrap();
        GpuMutationGuard::acquire_in(
            directory.path(),
            &["GPU-A".to_string(), "GPU-A".to_string()],
            "test",
        )
        .unwrap();
    }

    #[test]
    fn partial_multi_gpu_acquisition_releases_earlier_locks() {
        let directory = tempdir().unwrap();
        let held =
            GpuMutationGuard::acquire_in(directory.path(), &["GPU-B".to_string()], "held").unwrap();

        assert!(matches!(
            GpuMutationGuard::acquire_in(
                directory.path(),
                &["GPU-A".to_string(), "GPU-B".to_string()],
                "contender"
            ),
            Err(MutationLockError::Busy { gpu_uuid }) if gpu_uuid == "GPU-B"
        ));
        GpuMutationGuard::acquire_in(directory.path(), &["GPU-A".to_string()], "after-failure")
            .unwrap();
        drop(held);
    }
}
