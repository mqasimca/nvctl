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

### High-Level Flow

```
CLI (clap) → Commands → Services → NVML Abstraction → Hardware
GUI (iced) → App State → Services → NVML Abstraction → Hardware
```

### CLI Structure (nvctl)

```
src/
├── main.rs           # Entry point only
├── lib.rs            # Library exports
├── error.rs          # Error types (AppError, NvmlError, DomainError)
├── cli/
│   ├── args.rs       # CLI argument definitions
│   └── output.rs     # Output formatting (table, JSON, compact)
├── commands/         # Command handlers
│   ├── mod.rs        # Command exports
│   ├── list.rs       # List GPUs
│   ├── info.rs       # GPU information
│   ├── fan.rs        # Fan control
│   ├── power.rs      # Power management
│   ├── thermal.rs    # Thermal/acoustic control
│   ├── control.rs    # Control loop daemon
│   ├── health.rs     # Health monitoring
│   ├── processes.rs  # Process listing
│   └── alerts.rs     # Alert management
├── domain/           # Validated domain types
│   ├── mod.rs        # Type exports
│   ├── fan.rs        # FanSpeed, FanPolicy, FanCurve
│   ├── thermal.rs    # Temperature, ThermalThresholds
│   ├── power.rs      # PowerLimit, PowerConstraints
│   ├── performance.rs # ClockSpeed, Utilization, PerformanceState
│   ├── memory.rs     # MemoryInfo, EccErrors
│   ├── pcie.rs       # PcieMetrics, PcieLinkStatus
│   ├── process.rs    # GpuProcess, ProcessType, ProcessList
│   └── gpu.rs        # GpuInfo
├── services/         # Business logic layer
│   ├── mod.rs        # Service exports
│   ├── fan_service.rs     # Fan control logic
│   ├── power_service.rs   # Power management logic
│   ├── monitor.rs         # GPU monitoring service
│   └── alert_service.rs   # Alert processing
├── health/           # Health scoring system
│   ├── mod.rs        # Health score calculation
│   └── scorer.rs     # Component health scoring
├── alerts/           # Alert system
│   ├── mod.rs        # Alert types and rules
│   ├── config.rs     # Alert configuration
│   └── daemon.rs     # Alert monitoring daemon
├── nvml/             # NVML abstraction layer
│   ├── traits.rs     # GpuDevice, GpuManager traits
│   ├── device.rs     # Real NVML implementation (with C API bindings)
│   └── wrapper.rs    # NVML initialization and manager
├── config/           # Configuration system
│   └── mod.rs        # TOML configuration
└── mock.rs           # Test mocks (MockDevice, MockManager)
```

### GUI Structure (nvctl-gui)

```
nvctl-gui/src/
├── main.rs           # Entry point
├── app.rs            # Iced application (state, update, view)
├── message.rs        # Message types for Elm architecture
├── state.rs          # Application state management
├── theme.rs          # Glossy theme colors, fonts, spacing
├── views/            # Screen views (The Elm Architecture)
│   ├── mod.rs        # View exports
│   ├── dashboard.rs  # Dashboard view with all metrics
│   ├── fan.rs        # Fan control view with curve editor
│   ├── power.rs      # Power control view
│   ├── thermal.rs    # Thermal control view
│   ├── profiles.rs   # Profile management view
│   └── settings.rs   # Settings view
├── widgets/          # Custom canvas widgets
│   ├── mod.rs        # Widget exports
│   ├── temp_gauge.rs      # Temperature circular gauge
│   ├── fan_gauge.rs       # Fan speed circular gauge
│   ├── power_bar.rs       # Power usage circular gauge
│   ├── util_gauge.rs      # Utilization circular gauge
│   ├── health_gauge.rs    # Health score circular gauge
│   ├── ecc_gauge.rs       # ECC error display
│   ├── pcie_gauge.rs      # PCIe bandwidth gauge
│   ├── mem_temp_gauge.rs  # Memory temperature gauge
│   ├── video_gauge.rs     # Encoder/decoder gauge
│   ├── vram_bar.rs        # VRAM usage bar
│   ├── fan_curve.rs       # Interactive fan curve editor
│   ├── multi_series_graph.rs  # Multi-series time series
│   └── time_series.rs     # Single-series graphs
└── services/         # GUI-specific services
    ├── mod.rs        # Service exports
    ├── gpu_monitor.rs     # Background GPU polling
    ├── curve_daemon.rs    # Fan curve application
    ├── profiles.rs        # Profile save/load
    └── config.rs          # GUI configuration
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
- Output formatting: `src/cli/output.rs` (TableDisplay, JSON, Compact)

### NVML Layer
- GpuDevice trait: `src/nvml/traits.rs:16` (all GPU operations)
- NVML implementation: `src/nvml/device.rs:23` (NvmlDevice with C API bindings)
- NVML manager: `src/nvml/wrapper.rs:1` (NvmlManager)
- Mock device: `src/mock.rs:17` (MockDevice for tests)

### Domain Types
- Fan: `src/domain/fan.rs:10` (FanSpeed, FanPolicy, FanCurve)
- Thermal: `src/domain/thermal.rs:1` (Temperature, ThermalThresholds)
- Power: `src/domain/power.rs:1` (PowerLimit, PowerConstraints)
- Performance: `src/domain/performance.rs:1` (ClockSpeed, Utilization, PerformanceState)
- Memory: `src/domain/memory.rs:1` (MemoryInfo, EccErrors)
- PCIe: `src/domain/pcie.rs:1` (PcieMetrics, PcieLinkStatus)
- Process: `src/domain/process.rs:1` (GpuProcess, ProcessType, ProcessList)
- GPU Info: `src/domain/gpu.rs:1` (GpuInfo)

### Commands
- List: `src/commands/list.rs` (GPU listing)
- Info: `src/commands/info.rs` (GPU information with all metrics)
- Fan: `src/commands/fan.rs` (Fan control)
- Power: `src/commands/power.rs` (Power management)
- Thermal: `src/commands/thermal.rs` (Thermal control)
- Control: `src/commands/control.rs` (Daemon mode)
- Health: `src/commands/health.rs` (Health monitoring)
- Processes: `src/commands/processes.rs` (Process listing)
- Alerts: `src/commands/alerts.rs` (Alert management)

### Services
- Fan service: `src/services/fan_service.rs` (Fan control logic)
- Power service: `src/services/power_service.rs` (Power management logic)
- Monitor: `src/services/monitor.rs` (GPU monitoring)
- Alert service: `src/services/alert_service.rs` (Alert processing)

### Health & Alerts
- Health module: `src/health/mod.rs` (HealthScore, component scoring)
- Alert types: `src/alerts/mod.rs` (Alert, AlertRule, AlertLevel)
- Alert config: `src/alerts/config.rs` (TOML configuration)
- Alert daemon: `src/alerts/daemon.rs` (Background monitoring)

### Config
- Config: `src/config/mod.rs:20` (TOML configuration system)

### GUI (nvctl-gui)
- App: `nvctl-gui/src/app.rs` (Iced application state machine)
- Messages: `nvctl-gui/src/message.rs` (Elm Architecture messages)
- Theme: `nvctl-gui/src/theme.rs` (Colors, fonts, spacing constants)
- Dashboard: `nvctl-gui/src/views/dashboard.rs` (Main dashboard view)
- Widgets: `nvctl-gui/src/widgets/` (All custom canvas widgets)

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
