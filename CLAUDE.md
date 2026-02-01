# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> NVML-based GPU control tool for NVIDIA GPUs. Fan curves, power limits, thermal management.

## Quick Commands

```bash
make check          # Run all checks (fmt, lint, test)
make build          # Build release binary to bin/
make build-docker   # Build using Docker container
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

### GUI Commands
```bash
make gui            # Run GUI (release mode, smooth animations)
make gui-dev        # Run GUI (debug mode)
make gui-check      # Check GUI (fmt + clippy)
make gui-test       # Run GUI tests
make gui-build      # Build GUI release binary
```

## Git Rules

**FORBIDDEN:** `git push`, `git commit`, `git rebase`, `git push --force`
**ALLOWED:** `git status`, `git diff`, `git log`, `git add --dry-run`

---

## Architecture

### Layered Architecture Pattern

```
User Input (CLI/GUI)
    ↓
Commands Layer (CLI args → command handlers)
    ↓
Services Layer (business logic, orchestration)
    ↓
Domain Layer (validated types, business rules)
    ↓
NVML Abstraction (GpuDevice/GpuManager traits)
    ↓ (production)         ↓ (testing)
Real NVML Device       MockDevice
    ↓
NVIDIA Hardware
```

**Key Principle**: Commands never talk directly to NVML. Always go through services and use trait abstractions.

### Data Flow Example: `nvctl fan speed 50`

```
1. CLI parsing (args.rs)
   → FanArgs { command: Speed { speed: 50 } }

2. Command handler (commands/fan.rs)
   → Initialize NvmlManager
   → Get device via trait: manager.device_by_index(0)?
   → Create FanService with domain types

3. Service layer (services/fan_service.rs)
   → Validate: FanSpeed::new(50)?
   → Apply via trait: device.set_fan_speed(fan_idx, speed)?

4. NVML layer (nvml/device.rs or mock.rs)
   → Real: Call nvml-wrapper
   → Mock: Store in HashMap for testing

5. Output (cli/output.rs)
   → Format as Table/JSON/Compact
```

---

## Critical Architectural Patterns

### 1. Trait-Based Abstraction (Testing Without Hardware)

```rust
// ALWAYS use traits, NEVER concrete types in services
pub trait GpuDevice: Send + Sync {
    fn temperature(&self) -> Result<Temperature, NvmlError>;
    fn set_fan_speed(&mut self, fan_idx: u32, speed: FanSpeed)
        -> Result<(), NvmlError>;
}

// Production: Real NVML
impl GpuDevice for NvmlDevice<'_> { /* nvml-wrapper calls */ }

// Testing: Mock
impl GpuDevice for MockDevice { /* HashMap storage */ }

// Services are generic over GpuDevice trait
impl FanService {
    pub fn apply_curve<D: GpuDevice>(&self, device: &mut D)
        -> Result<FanSpeed, ServiceError> {
        let temp = device.temperature()?;  // Works with both!
        // ...
    }
}
```

**Location**: `src/nvml/traits.rs:16` (GpuDevice), `src/mock.rs:17` (MockDevice)

### 2. Validated Domain Types (Make Invalid States Unrepresentable)

```rust
// WRONG: Raw primitives allow invalid values
fn set_fan_speed(speed: u8) -> Result<()> {
    if speed > 100 { return Err(...); }  // Runtime check, can be forgotten
    // ...
}

// CORRECT: Domain type validates on construction
#[derive(Debug, Clone, Copy)]
pub struct FanSpeed(u8);  // Private field!

impl FanSpeed {
    pub fn new(value: u8) -> Result<Self, DomainError> {
        if value > 100 {
            return Err(DomainError::InvalidFanSpeed(value));
        }
        Ok(Self(value))  // Cannot construct invalid instance
    }

    pub fn as_percentage(&self) -> u8 { self.0 }
}

// Now impossible to create invalid values
let speed = FanSpeed::new(50)?;  // ✓ OK
let invalid = FanSpeed::new(150)?;  // ✗ Compile-time safety
```

**Pattern**: All domain types validate in `new()`. See `src/domain/fan.rs:15`.

**Key Domain Types**:
- `FanSpeed`: 0-100 percentage
- `Temperature`: Celsius (no bounds, physical reality constrains)
- `PowerLimit`: Watts (validates against `PowerConstraints`)
- `FanCurve`: Sorted temperature/speed points

### 3. Error Hierarchy (Context-Specific Handling)

```
AppError (top-level, user-facing in main.rs)
├── NvmlError (NVML operations fail)
│   ├── LibraryNotFound → "Install NVIDIA drivers"
│   ├── InsufficientPermissions → "Run with sudo"
│   ├── NotSupported → "Feature unavailable on this GPU"
│   └── GpuLost → "GPU disconnected/crashed"
├── DomainError (validation fails)
│   ├── InvalidFanSpeed(u8) → "Speed must be 0-100"
│   ├── InvalidPowerLimit { value, min, max }
│   └── UnsortedFanCurve → "Curve points must be sorted"
├── ConfigError (file parsing)
└── ServiceError (internal logic)
    └── DryRun (special: preview mode)
```

**Error Propagation**: All errors auto-convert to `AppError` via `From` impls.

```rust
let device = manager.device_by_index(0)?;  // NvmlError → AppError
let speed = FanSpeed::new(val)?;  // DomainError → AppError
let config = Config::load(path)?;  // ConfigError → AppError
```

**Location**: `src/error.rs:10` (AppError), `src/error.rs:50` (NvmlError)

### 4. No-Panic Guarantee in Library Code

**RULE**: Library code (`src/lib.rs` exports) NEVER panics.

```rust
// ✗ FORBIDDEN in library code
fn get_temp(&self) -> Temperature {
    self.device.temperature().unwrap()  // NO!
}

fn process_data(&self, data: &[u8]) {
    let value = data[0];  // NO! Can panic on empty slice
}

// ✓ REQUIRED in library code
fn get_temp(&self) -> Result<Temperature, NvmlError> {
    self.device.temperature()  // Returns Result
}

fn process_data(&self, data: &[u8]) -> Result<u8, AppError> {
    data.get(0).copied().ok_or(AppError::InvalidData)
}
```

**Exceptions**: Only `main.rs` and test code may use `.unwrap()` or `.expect()`.

### 5. Dry-Run Pattern (Safe Testing on Production)

```rust
// Services always support dry-run mode
pub struct FanService {
    dry_run: bool,
}

impl FanService {
    pub fn apply_speed<D: GpuDevice>(
        &self,
        device: &mut D,
        speed: FanSpeed
    ) -> Result<(), ServiceError> {
        if self.dry_run {
            log::info!("DRY RUN: Would set fan to {}", speed);
            return Err(ServiceError::DryRun);  // Special error
        }

        device.set_fan_speed(0, speed)?;  // Actual mutation
        Ok(())
    }
}
```

**CLI Integration**: `nvctl --dry-run fan speed 100` previews changes without applying.

---

## GUI Architecture (Elm Architecture)

### The Elm Architecture (TEA) Pattern

```
State (AppState, GpuState, MetricsHistory)
    ↓
View (renders UI from state)
    ↓
User Interaction (click, drag, type)
    ↓
Message (enum of all possible actions)
    ↓
Update (State → Message → new State)
    ↓
(loop back to State)
```

### Message Hierarchy (Type-Safe Events)

```rust
// Top-level messages
pub enum Message {
    // Navigation
    ViewChanged(View),

    // GPU polling
    Tick(Instant),
    GpuStateUpdated(Box<GpuStateSnapshot>),

    // Sub-domain messages (namespaced)
    FanControl(FanControlMessage),
    PowerControl(PowerControlMessage),
    Profile(ProfileMessage),

    // Results
    OperationResult(Result<String, String>),
}

// Sub-domain messages are type-safe
pub enum FanControlMessage {
    PolicyChanged(u32, FanPolicy),
    SpeedChanged(u32, FanSpeed),
    CurvePointMoved { index: usize, temp: i32, speed: u8 },
    ApplyCurve,
}
```

### State Management

```rust
// Centralized application state
pub struct AppState {
    pub current_view: View,
    pub gpus: Vec<GpuState>,
    pub selected_gpu: usize,
    pub linked_gpus: bool,
    pub editing_curves: Vec<Option<FanCurve>>,
    pub profiles: Vec<Profile>,
    // ...
}

// Per-GPU state
pub struct GpuState {
    pub info: GpuInfo,
    pub temperature: Temperature,
    pub fan_speeds: Vec<FanSpeed>,
    pub power_limit: PowerLimit,
    pub metrics_history: MetricsHistory,
    // ...
}

// Update pattern (state machine)
impl NvctlGui {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GpuStateUpdated(snapshot) => {
                // Update state
                self.state.gpus[snapshot.index].temperature = snapshot.temperature;
                Task::none()
            }

            Message::FanControl(FanControlMessage::ApplyCurve) => {
                // Spawn async task
                let curve = self.state.editing_curves[self.state.selected_gpu].clone();
                Task::perform(
                    async move { apply_fan_curve(curve).await },
                    |result| Message::OperationResult(result),
                )
            }

            // ...
        }
    }
}
```

### Canvas Widget Pattern (Smooth Rendering)

```rust
// Cached canvas rendering
pub struct TempGauge {
    cache: canvas::Cache,
}

impl canvas::Program<Message> for TempGauge {
    fn draw(&self, state: &State, bounds: Rectangle) -> Vec<Geometry> {
        let geometry = self.cache.draw(bounds.size(), |frame| {
            // Draw only when cache is invalid
            draw_circular_gauge(frame, state.temperature);
        });
        vec![geometry]
    }
}

// Invalidate cache only when data changes
pub fn update(&mut self, msg: Message) {
    if let Message::GpuStateUpdated(_) = msg {
        self.cache.clear();  // ← Force redraw
    }
}
```

**Location**: `nvctl-gui/src/widgets/` (all canvas widgets)

---

## Code Patterns

### Error Propagation
```rust
let device = get_device()?;  // ? converts error types automatically
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

## Testing Strategy

### Mock-Based Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockDevice;

    #[test]
    fn test_fan_service_applies_curve() {
        // Arrange
        let mut mock = MockDevice::new(0)
            .with_temperature(Temperature::new(80));

        let curve = FanCurve::default();
        let service = FanService::new(curve, false);

        // Act
        let speed = service.apply_curve(&mut mock).unwrap();

        // Assert
        assert_eq!(speed.as_percentage(), 50);
    }

    #[test]
    fn test_domain_validation() {
        assert!(FanSpeed::new(100).is_ok());
        assert!(FanSpeed::new(101).is_err());
    }
}
```

**Test Categories**:
- Unit tests: In-module, using mocks
- Boundary tests: 0, max, overflow
- Error cases: Invalid inputs, NVML failures
- CLI parsing: Argument validation

**Location**: `src/mock.rs:17` (MockDevice, MockManager)

---

## Workflow: Adding a New Command

1. **Define CLI args** (`src/cli/args.rs`)
   ```rust
   #[derive(Parser)]
   pub struct NewCommandArgs {
       #[arg(short, long)]
       pub value: u32,
   }

   pub enum Commands {
       NewCommand(NewCommandArgs),
       // ...
   }
   ```

2. **Create command handler** (`src/commands/new_command.rs`)
   ```rust
   pub fn run_new_command(
       args: &NewCommandArgs,
       format: OutputFormat,
       gpu_index: Option<u32>,
       dry_run: bool,
   ) -> Result<()> {
       let manager = NvmlManager::new()?;
       let mut device = manager.device_by_index(gpu_index.unwrap_or(0))?;

       // Use domain types
       let value = DomainType::new(args.value)?;

       // Use services
       let service = Service::new(dry_run);
       let result = service.apply(&mut device, value)?;

       print_output(&result, format)?;
       Ok(())
   }
   ```

3. **Add domain types** (`src/domain/`) if needed
   ```rust
   pub struct DomainType(u32);

   impl DomainType {
       pub fn new(value: u32) -> Result<Self, DomainError> {
           // Validation
           Ok(Self(value))
       }
   }
   ```

4. **Add service** (`src/services/`) if needed
   ```rust
   pub struct Service {
       dry_run: bool,
   }

   impl Service {
       pub fn apply<D: GpuDevice>(
           &self,
           device: &mut D,
           value: DomainType
       ) -> Result<Output, ServiceError> {
           if self.dry_run {
               log::info!("DRY RUN: Would apply {}", value);
               return Err(ServiceError::DryRun);
           }

           // Use trait methods
           device.some_operation(value)?;
           Ok(Output::new())
       }
   }
   ```

5. **Wire in main.rs**
   ```rust
   Commands::NewCommand(args) => {
       run_new_command(args, cli.format, cli.gpu, cli.dry_run)
   }
   ```

6. **Write tests**
   ```rust
   #[test]
   fn test_new_command() {
       let mut mock = MockDevice::new(0);
       let service = Service::new(false);
       let result = service.apply(&mut mock, value).unwrap();
       assert_eq!(result, expected);
   }
   ```

7. **Run quality gates**: `make check`

---

## File Structure

### CLI (nvctl)

```
src/
├── main.rs              # Entry point, error hints
├── lib.rs               # Library exports
├── error.rs             # Error hierarchy (AppError, NvmlError, DomainError)
├── cli/
│   ├── args.rs          # CLI argument definitions (clap)
│   └── output.rs        # Output formatting (Table, JSON, Compact)
├── commands/            # Command handlers (one per command)
│   ├── list.rs          # List GPUs
│   ├── info.rs          # GPU information
│   ├── fan.rs           # Fan control
│   ├── power.rs         # Power management
│   ├── thermal.rs       # Thermal/acoustic control
│   ├── control.rs       # Control loop daemon
│   ├── health.rs        # Health monitoring
│   ├── processes.rs     # Process listing
│   └── alerts.rs        # Alert management
├── services/            # Business logic layer
│   ├── fan_service.rs   # Fan control logic
│   ├── power_service.rs # Power management logic
│   ├── monitor.rs       # GPU monitoring service
│   └── alert_service.rs # Alert processing
├── domain/              # Validated domain types
│   ├── fan.rs           # FanSpeed, FanPolicy, FanCurve
│   ├── thermal.rs       # Temperature, ThermalThresholds
│   ├── power.rs         # PowerLimit, PowerConstraints
│   ├── performance.rs   # ClockSpeed, Utilization, PerformanceState
│   ├── memory.rs        # MemoryInfo, EccErrors
│   ├── pcie.rs          # PcieMetrics, PcieLinkStatus
│   ├── process.rs       # GpuProcess, ProcessType, ProcessList
│   └── gpu.rs           # GpuInfo
├── nvml/                # NVML abstraction layer
│   ├── traits.rs        # GpuDevice, GpuManager traits
│   ├── device.rs        # Real NVML implementation (C API bindings)
│   └── wrapper.rs       # NVML initialization and manager
├── config/              # Configuration system
│   └── mod.rs           # TOML configuration
├── health/              # Health scoring system
│   └── mod.rs           # Health score calculation
├── alerts/              # Alert system
│   ├── mod.rs           # Alert types and rules
│   ├── config.rs        # Alert configuration
│   └── daemon.rs        # Alert monitoring daemon
└── mock.rs              # Test mocks (MockDevice, MockManager)
```

### GUI (nvctl-gui)

```
nvctl-gui/src/
├── main.rs              # Entry point
├── app.rs               # Iced application (state, update, view)
├── message.rs           # Message types for Elm architecture
├── state.rs             # Application state management
├── theme.rs             # Glossy theme colors, fonts, spacing
├── views/               # Screen views (The Elm Architecture)
│   ├── dashboard.rs     # Dashboard view with all metrics
│   ├── fan.rs           # Fan control view with curve editor
│   ├── power.rs         # Power control view
│   ├── thermal.rs       # Thermal control view
│   ├── profiles.rs      # Profile management view
│   └── settings.rs      # Settings view
├── widgets/             # Custom canvas widgets
│   ├── temp_gauge.rs    # Temperature circular gauge
│   ├── fan_gauge.rs     # Fan speed circular gauge
│   ├── power_bar.rs     # Power usage circular gauge
│   ├── util_gauge.rs    # Utilization circular gauge
│   ├── health_gauge.rs  # Health score circular gauge
│   ├── ecc_gauge.rs     # ECC error display
│   ├── pcie_gauge.rs    # PCIe bandwidth gauge
│   ├── mem_temp_gauge.rs# Memory temperature gauge
│   ├── video_gauge.rs   # Encoder/decoder gauge
│   ├── vram_bar.rs      # VRAM usage bar
│   ├── fan_curve.rs     # Interactive fan curve editor
│   ├── multi_series_graph.rs  # Multi-series time series
│   └── time_series.rs   # Single-series graphs
└── services/            # GUI-specific services
    ├── gpu_monitor.rs   # Background GPU polling
    ├── curve_daemon.rs  # Fan curve application
    ├── profiles.rs      # Profile save/load
    └── config.rs        # GUI configuration
```

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
- [ ] Domain types validate input in constructors
- [ ] Services are generic over `GpuDevice` trait
- [ ] Tests use `MockDevice` for trait abstraction
- [ ] Binary builds: `make build` (outputs to `bin/`)
