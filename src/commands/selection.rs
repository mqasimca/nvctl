//! GPU selection shared by CLI command handlers.

use crate::config::GpuConfig;
use crate::error::{AppError, ConfigError, Result};
use crate::nvml::{GpuDevice, GpuManager};

/// A validated GPU selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuSelector {
    /// Target every detected GPU.
    All,
    /// Target a GPU by zero-based NVML index.
    Index(u32),
    /// Target the only GPU whose name contains this string.
    Name(String),
    /// Target a GPU by its NVML UUID.
    Uuid(String),
}

impl GpuSelector {
    /// Build a selector from merged configuration and reject ambiguous input.
    pub fn from_config(config: &GpuConfig) -> Result<Self> {
        let configured = [
            config.index.is_some(),
            config.name.is_some(),
            config.uuid.is_some(),
        ]
        .into_iter()
        .filter(|value| *value)
        .count();

        if configured > 1 {
            return Err(ConfigError::InvalidValue {
                key: "gpu".to_string(),
                message: "configure only one of index, name, or uuid".to_string(),
            }
            .into());
        }

        Ok(if let Some(index) = config.index {
            Self::Index(index)
        } else if let Some(name) = &config.name {
            Self::Name(name.clone())
        } else if let Some(uuid) = &config.uuid {
            Self::Uuid(uuid.clone())
        } else {
            Self::All
        })
    }
}

/// Resolve a selector to concrete device indices and reject an empty result.
pub fn select_gpu_indices<M: GpuManager>(manager: &M, selector: &GpuSelector) -> Result<Vec<u32>> {
    let indices = match selector {
        GpuSelector::All => (0..manager.device_count()?).collect(),
        GpuSelector::Index(index) => {
            manager.device_by_index(*index)?;
            vec![*index]
        }
        GpuSelector::Name(name) => {
            let needle = name.to_lowercase();
            let mut matches = Vec::new();
            for index in 0..manager.device_count()? {
                let device = manager.device_by_index(index)?;
                if device.name()?.to_lowercase().contains(&needle) {
                    matches.push(index);
                }
            }
            match matches.as_slice() {
                [] => return Err(AppError::GpuNotFound(format!("name contains '{}'", name))),
                [index] => vec![*index],
                _ => {
                    return Err(ConfigError::InvalidValue {
                        key: "gpu.name".to_string(),
                        message: format!(
                            "'{}' matches multiple GPU indices {:?}; use --gpu or --gpu-uuid",
                            name, matches
                        ),
                    }
                    .into())
                }
            }
        }
        GpuSelector::Uuid(uuid) => {
            let device = manager.device_by_uuid(uuid).map_err(|error| match error {
                crate::error::NvmlError::DeviceNotFound(_)
                | crate::error::NvmlError::DeviceNotFoundByUuid(_) => {
                    AppError::GpuNotFound(format!("UUID {}", uuid))
                }
                other => AppError::Nvml(other),
            })?;
            vec![device.index()]
        }
    };

    if indices.is_empty() {
        Err(AppError::NoGpusFound)
    } else {
        Ok(indices)
    }
}

/// Resolve selected indices to stable UUIDs before a controller captures state.
pub fn resolve_gpu_uuids<M: GpuManager>(manager: &M, indices: &[u32]) -> Result<Vec<String>> {
    indices
        .iter()
        .map(|index| manager.device_by_index(*index)?.uuid().map_err(Into::into))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockDevice, MockManager};

    #[test]
    fn resolves_name_and_uuid_selectors() {
        let manager = MockManager::with_devices(vec![
            MockDevice::new(0).with_name("RTX 4090"),
            MockDevice::new(1).with_name("RTX 5080"),
        ]);

        assert_eq!(
            select_gpu_indices(&manager, &GpuSelector::Name("5080".into())).unwrap(),
            vec![1]
        );
        assert_eq!(
            select_gpu_indices(&manager, &GpuSelector::Uuid("GPU-MOCK-0001".into())).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn rejects_ambiguous_config_and_empty_manager() {
        let config = GpuConfig {
            index: Some(0),
            name: Some("RTX".into()),
            uuid: None,
        };
        assert!(GpuSelector::from_config(&config).is_err());

        let manager = MockManager::new(0);
        assert!(matches!(
            select_gpu_indices(&manager, &GpuSelector::All),
            Err(AppError::NoGpusFound)
        ));
    }

    #[test]
    fn rejects_ambiguous_name_matches() {
        let manager = MockManager::with_devices(vec![
            MockDevice::new(0).with_name("RTX 4090"),
            MockDevice::new(1).with_name("RTX 4090"),
        ]);

        assert!(matches!(
            select_gpu_indices(&manager, &GpuSelector::Name("4090".into())),
            Err(AppError::Config(ConfigError::InvalidValue { .. }))
        ));
    }

    #[test]
    fn resolves_stable_uuids_for_selected_indices() {
        let manager = MockManager::new(2);
        assert_eq!(
            resolve_gpu_uuids(&manager, &[1, 0]).unwrap(),
            vec!["GPU-MOCK-0001", "GPU-MOCK-0000"]
        );
    }
}
