# nvctl - Claude Code Configuration

> NVML-based GPU control tool for NVIDIA GPUs. Fan curves, power limits, thermal management.

## Quick Commands

```bash
make check          # Run all checks (fmt, lint, test)
make build          # Build release binary to bin/
make ci-full        # Full CI: clean + checks + release
make help           # Show all available targets
```

### Individual Commands
```bash
make fmt            # Format code
make lint           # Clippy (MUST pass)
make test           # Run tests
make clean          # Remove build artifacts
```

## Git Rules

**FORBIDDEN:** `git push`, `git commit`, `git rebase`, `git push --force`
**ALLOWED:** `git status`, `git diff`, `git log`, `git add --dry-run`

---

## Architecture

```
CLI (clap) → Commands → Services → NVML → Hardware

src/main.rs          # Entry point only
src/error.rs         # Error types (AppError, NvmlError, DomainError)
src/cli/args.rs      # CLI definitions
src/cli/output.rs    # Output formatting
src/commands/*.rs    # Command handlers
src/domain/*.rs      # Validated types (FanSpeed, Temperature, PowerLimit)
src/services/*.rs    # Business logic
src/nvml/traits.rs   # GpuDevice trait
src/nvml/device.rs   # Real NVML impl
src/mock.rs          # Test mocks
```

---

## Critical Rules

### IMPORTANT: No Panics in Library Code
```rust
// CORRECT: Return Result
fn get_temp(&self) -> Result<Temperature, NvmlError> { ... }

// WRONG: Never unwrap
fn get_temp(&self) -> Temperature { self.inner.temp().unwrap() }  // NO!
```

### IMPORTANT: Validate Domain Types on Construction
See `src/domain/fan.rs:15` for `FanSpeed::new()` pattern.

### IMPORTANT: Use Trait Abstraction
See `src/nvml/traits.rs:12` for `GpuDevice` trait. Mock via `src/mock.rs`.

---

## Code Patterns

### Error Propagation
```rust
let device = get_device()?;
let temp = device.temperature()?;
```

### Error Mapping
```rust
let result = nvml_call().map_err(NvmlError::from)?;
```

### Option to Result
```rust
devices.get(idx).ok_or(AppError::GpuNotFound(idx))?
```

### Derives
- Value types: `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- Data: `#[derive(Debug, Clone, Serialize, Deserialize)]`
- Errors: `#[derive(Error, Debug)]`

---

## Testing

Every change needs tests. Pattern for mocks:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockDevice;

    #[test]
    fn test_with_mock() {
        let mock = MockDevice::new().with_temperature(Temperature::new(65));
        // test logic
    }
}
```

Test categories: unit tests in-module, boundary tests (0, max, overflow), error cases, CLI parsing.

---

## Workflow: Adding Features

1. Define CLI args in `src/cli/args.rs`
2. Create command in `src/commands/`
3. Add domain types in `src/domain/` (if needed)
4. Add service in `src/services/` (if needed)
5. Write tests at each layer
6. Run quality gates: `make check`

## Workflow: Fixing Bugs

1. Write failing test that reproduces bug
2. Fix the code
3. Verify test passes
4. Run full test suite

---

## Agent Configuration

### Proactive Agents (use automatically)
| Agent | Trigger |
|-------|---------|
| `code-reviewer` | After writing significant code |
| `code-simplifier` | After complex implementations |
| `silent-failure-hunter` | After adding error handling |
| `pr-test-analyzer` | Before PR creation |
| `type-design-analyzer` | When adding new types |

### On-Demand Agents
| Agent | Use Case |
|-------|----------|
| `Explore` | Understanding codebase structure |
| `Plan` | Major feature planning |

### Thinking Modes for Complex Tasks
Use extended thinking for architecture decisions: `think hard` or `ultrathink`

---

## File References

### Core
- Entry point: `src/main.rs:1`
- Error types: `src/error.rs:10` (AppError), `src/error.rs:50` (NvmlError)
- CLI args: `src/cli/args.rs:14` (Cli struct)

### NVML Layer
- GpuDevice trait: `src/nvml/traits.rs:16` (all GPU operations)
- NVML implementation: `src/nvml/device.rs:23` (NvmlDevice)
- Mock device: `src/mock.rs:17` (MockDevice for tests)

### Domain Types
- Fan: `src/domain/fan.rs:10` (FanSpeed, FanPolicy, FanCurve)
- Thermal: `src/domain/thermal.rs:1` (Temperature, ThermalThresholds)
- Power: `src/domain/power.rs:1` (PowerLimit, PowerConstraints)
- Performance: `src/domain/performance.rs:1` (ClockSpeed, Utilization, MemoryInfo, PerformanceState)
- GPU Info: `src/domain/gpu.rs:1` (GpuInfo)

### Config
- Config: `src/config/mod.rs:20` (Config struct)

---

## Logging

```rust
use log::{debug, info, warn, error};

debug!("Internal: temp={}", temp);      // Debug details
info!("Setting fan to {}%", speed);     // User operations
warn!("Fan {} unresponsive", idx);      // Recoverable
error!("Power limit failed: {}", e);    // Failures
```

---

## Quality Checklist

Before completing any task:
- [ ] `make check` passes (fmt + lint + test)
- [ ] No `.unwrap()` or `.expect()` in library code
- [ ] New public items have `///` docs
- [ ] Domain types validate input
- [ ] Binary builds: `make build` (outputs to `bin/`)
