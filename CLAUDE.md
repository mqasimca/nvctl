# Claude Code Rules for nvctl

## Project Overview

nvctl is an NVML-based GPU control tool written in Rust. It provides fan control, power management, and thermal monitoring for NVIDIA GPUs via a single binary CLI.

---

## CRITICAL: Forbidden Operations

### Git - NEVER Execute These
- `git push` - User pushes manually
- `git commit` - User commits manually
- `git push --force` - Destructive operation
- `git rebase` - User manages history

### Allowed Git (Read-Only)
```bash
git status
git diff
git log
git add --dry-run
```

---

## Code Writing Standards

### Rule 1: Always Read Before Write
**NEVER modify code without reading it first.** Before any edit:
1. Read the target file completely
2. Read related files (imports, tests)
3. Understand existing patterns
4. Then write code

### Rule 2: Error Handling - No Panics
```rust
// CORRECT - Return Result
pub fn get_temperature(&self) -> Result<Temperature, NvmlError> {
    self.device.temperature(TemperatureSensor::Gpu)
        .map(Temperature::new)
        .map_err(NvmlError::from)
}

// WRONG - Never do this
pub fn get_temperature(&self) -> Temperature {
    Temperature::new(self.device.temperature(...).unwrap())  // NO!
}
```

### Rule 3: Newtype Validation
All domain types validate on construction:
```rust
pub struct FanSpeed(u8);

impl FanSpeed {
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if value > 100 {
            return Err(DomainError::InvalidFanSpeed(value));
        }
        Ok(Self(value))
    }

    pub fn as_percentage(&self) -> u8 { self.0 }
}

impl TryFrom<u8> for FanSpeed {
    type Error = DomainError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
```

### Rule 4: Trait-Based Design
```rust
// Define trait for abstraction
pub trait GpuDevice: Send + Sync {
    fn temperature(&self) -> Result<Temperature, NvmlError>;
    fn fan_speed(&self, fan: u32) -> Result<FanSpeed, NvmlError>;
    fn set_fan_speed(&self, fan: u32, speed: FanSpeed) -> Result<(), NvmlError>;
    fn power_limit(&self) -> Result<PowerLimit, NvmlError>;
    fn set_power_limit(&self, limit: PowerLimit) -> Result<(), NvmlError>;
}

// Real implementation
impl GpuDevice for NvmlDevice { /* ... */ }

// Mock for testing
#[cfg(test)]
impl GpuDevice for MockDevice { /* ... */ }
```

### Rule 5: Error Types with thiserror
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("NVML error: {0}")]
    Nvml(#[from] NvmlError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("No GPUs found")]
    NoGpusFound,

    #[error("GPU index {0} not found")]
    GpuNotFound(u32),
}
```

---

## Architecture

```
Request Flow:
CLI (clap) → Commands → Services → NVML Abstraction → Hardware

src/
├── main.rs              # Entry point ONLY (no business logic)
├── lib.rs               # Public API, module exports
├── error.rs             # All error types
├── cli/
│   ├── mod.rs
│   ├── args.rs          # Clap definitions
│   └── output.rs        # Output formatting
├── commands/
│   ├── mod.rs
│   ├── list.rs          # nvctl list
│   ├── info.rs          # nvctl info
│   ├── fan.rs           # nvctl fan
│   ├── power.rs         # nvctl power
│   └── control.rs       # nvctl control (daemon)
├── domain/
│   ├── mod.rs
│   ├── fan.rs           # FanSpeed, FanPolicy
│   ├── power.rs         # PowerLimit, PowerState
│   ├── thermal.rs       # Temperature
│   └── gpu.rs           # GpuInfo
├── services/
│   ├── mod.rs
│   ├── fan_service.rs   # Fan control logic
│   ├── power_service.rs # Power control logic
│   └── monitor.rs       # Monitoring daemon
├── nvml/
│   ├── mod.rs
│   ├── traits.rs        # GpuDevice trait
│   ├── device.rs        # NvmlDevice impl
│   └── wrapper.rs       # NVML initialization
├── config/
│   ├── mod.rs
│   ├── file.rs          # Config file parsing
│   └── builder.rs       # Config builder
└── mock.rs              # Test mocks (#[cfg(test)])
```

---

## Testing Requirements

### Every PR Must Have Tests

#### Unit Tests - In Module
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_speed_validates_range() {
        assert!(FanSpeed::new(0).is_ok());
        assert!(FanSpeed::new(100).is_ok());
        assert!(FanSpeed::new(101).is_err());
    }

    #[test]
    fn fan_speed_boundary_conditions() {
        // Test boundaries explicitly
        let min = FanSpeed::new(0).unwrap();
        let max = FanSpeed::new(100).unwrap();
        assert_eq!(min.as_percentage(), 0);
        assert_eq!(max.as_percentage(), 100);
    }
}
```

#### Integration Tests - In tests/
```rust
// tests/cli_integration.rs
use assert_cmd::Command;

#[test]
fn cli_shows_help() {
    Command::cargo_bin("nvctl")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}
```

#### Mock Testing Pattern
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockDevice;

    #[test]
    fn service_handles_device_error() {
        let mock = MockDevice::new()
            .with_temperature_error(NvmlError::Unknown);

        let service = FanService::new(Box::new(mock));
        let result = service.get_status();

        assert!(matches!(result, Err(AppError::Nvml(_))));
    }
}
```

### Test Checklist
- [ ] Unit tests for all domain types
- [ ] Boundary value tests (0, max, overflow)
- [ ] Error case tests
- [ ] Mock-based service tests
- [ ] CLI argument parsing tests

---

## Quality Gates

### Before Marking Work Complete
Run these commands and fix all issues:

```bash
# Format code
cargo fmt

# Lint check - fix ALL warnings
cargo clippy -- -D warnings

# Run all tests
cargo test

# Check docs compile
cargo doc --no-deps
```

### Code Quality Checklist
- [ ] No `.unwrap()` or `.expect()` in library code
- [ ] All public items documented with `///`
- [ ] Tests for new functionality
- [ ] Error types properly defined
- [ ] Domain types validate input

---

## Dependencies

```toml
[dependencies]
nvml-wrapper = "0.11"
clap = { version = "4", features = ["derive", "env"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
thiserror = "2"
log = "0.4"
env_logger = "0.11"

[dev-dependencies]
tempfile = "3"
```

---

## Agent Workflows

### When Adding a New Command
1. Define args in `src/cli/args.rs`
2. Create handler in `src/commands/new_cmd.rs`
3. Add service logic in `src/services/`
4. Add domain types if needed in `src/domain/`
5. Export from `mod.rs` files
6. Add tests at each layer
7. Run quality gates

### When Fixing a Bug
1. Write a failing test that reproduces the bug
2. Fix the code
3. Verify test passes
4. Run full test suite
5. Run quality gates

### When Refactoring
1. Ensure tests exist for current behavior
2. Make incremental changes
3. Run tests after each change
4. Keep commits atomic

---

## Logging Conventions

```rust
use log::{debug, info, warn, error};

// Debug: Internal details
debug!("Calculating fan curve for temp={}", temp);

// Info: User-visible operations
info!("Setting fan speed to {}%", speed);

// Warn: Recoverable issues
warn!("Fan {} not responding, retrying", fan_idx);

// Error: Failures
error!("Failed to set power limit: {}", e);
```

---

## Agent Skills Reference

| Skill | When to Use |
|-------|-------------|
| `code-reviewer` | After writing significant code |
| `code-simplifier` | When code is complex or verbose |
| `Explore` | To understand codebase structure |
| `Plan` | Before implementing major features |
| `pr-test-analyzer` | Before creating a PR |
| `silent-failure-hunter` | After adding error handling |

---

## Quick Reference

### Common Patterns

```rust
// Propagate errors
fn example() -> Result<(), AppError> {
    let device = get_device()?;
    device.set_fan_speed(0, speed)?;
    Ok(())
}

// Map errors
let result = nvml_call()
    .map_err(NvmlError::from)?;

// Option to Result
let device = devices.get(idx)
    .ok_or(AppError::GpuNotFound(idx))?;
```

### Derive Attributes
```rust
// Small value types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

// Data structures
#[derive(Debug, Clone, Serialize, Deserialize)]

// Error types
#[derive(Error, Debug)]
```
