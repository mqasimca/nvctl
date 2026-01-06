# nvctl

A fast, safe NVIDIA GPU control tool written in Rust. Manage fan speeds, power limits, and thermal settings via NVML.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

## Features

### CLI Tool
- **Fan Control** - Manual speed control and automatic fan curves
- **Power Management** - Set and monitor GPU power limits with constraint validation
- **Thermal Monitoring** - Real-time temperature and threshold management
- **Acoustic Limiting** - GPU temperature targets for noise control
- **Multi-GPU Support** - Target by index, name, or UUID
- **Multiple Output Formats** - Table, JSON, and compact output
- **Dry-Run Mode** - Preview changes before applying
- **Daemon Mode** - Continuous control loop with custom fan curves
- **Configuration Files** - TOML-based persistent configuration

### GUI Application (nvctl-gui)
- **Glossy Glassmorphism Design** - Modern, vibrant interface with glass effects
- **Real-time Monitoring** - Live temperature, fan speed, and power gauges
- **Interactive Fan Curves** - Drag-and-drop curve editor with visual feedback
- **Multi-GPU Dashboard** - Overview of all GPUs with link/unlink control
- **Profile System** - Save and load configuration profiles
- **Temperature History** - Visual graph of temperature over time
- **Per-Fan Control** - Individual fan speed control with cooler target info

## Installation

### From Source

```bash
git clone https://github.com/your-repo/nvctl.git
cd nvctl

# Build CLI tool
cargo build --release
sudo cp target/release/nvctl /usr/local/bin/

# Build GUI (optional)
cargo build --release --package nvctl-gui
sudo cp target/release/nvctl-gui /usr/local/bin/
```

### Requirements

- NVIDIA GPU with proprietary driver installed
- NVIDIA driver version 520+ recommended
- Linux with libnvidia-ml.so available (included with nvidia-utils)

## Quick Start

```bash
# List all GPUs
nvctl list

# Show GPU information
nvctl info --all

# Check fan status
nvctl fan status

# Set fan to manual mode and 75% speed
sudo nvctl fan policy manual
sudo nvctl fan speed 75

# Set power limit to 250W
sudo nvctl power limit 250

# Preview changes without applying
nvctl --dry-run fan speed 100
```

## GUI Application

Launch the graphical interface for visual GPU control:

```bash
# Run GUI (release mode for smooth animations)
make gui

# Or directly
nvctl-gui
```

### GUI Features

- **Dashboard** - Real-time gauges for temperature, fan speed, and power with colorful gradients
- **Fan Control** - Interactive fan curve editor with drag-and-drop points
- **Power Control** - Slider-based power limit adjustment with constraints display
- **Thermal Control** - Temperature threshold configuration
- **Profiles** - Save/load/delete configuration profiles
- **Settings** - Refresh rate, theme preferences, and startup options

### GUI Make Commands

```bash
make gui          # Run GUI (release mode)
make gui-dev      # Run GUI (debug mode)
make gui-check    # Check GUI code (fmt + clippy)
make gui-test     # Run GUI tests
make gui-build    # Build GUI release binary
```

## CLI Usage

### Listing GPUs

```bash
nvctl list
nvctl list --format json
```

### GPU Information

```bash
nvctl info                    # Basic info
nvctl info --all              # All details
nvctl info --fan              # Fan info only
nvctl info --power            # Power info only
nvctl info --thermal          # Thermal info only
nvctl --gpu 0 info --all      # Specific GPU
nvctl --gpu-name "RTX 4090" info  # By name
```

### Fan Control

```bash
# Check status
nvctl fan status

# Set control policy
sudo nvctl fan policy manual   # Enable manual control
sudo nvctl fan policy auto     # Return to automatic

# Set fan speed (requires manual policy)
sudo nvctl fan speed 50        # All fans to 50%
sudo nvctl fan speed 80 --fan-index 0  # Specific fan

# Dry run
nvctl --dry-run fan speed 100
```

### Power Management

```bash
# Check current power status
nvctl power status

# Set power limit
sudo nvctl power limit 250     # Set to 250W

# Dry run
nvctl --dry-run power limit 300
```

### Thermal Control

Control the acoustic temperature limit. The GPU throttles performance to maintain the target temperature (same as GeForce Experience temperature target).

```bash
# Check thermal status
nvctl thermal status

# Set acoustic temperature limit
sudo nvctl thermal limit 80

# Dry run
nvctl --dry-run thermal limit 75
```

Note: Not all GPUs support acoustic temperature limits.

### Daemon Mode (Fan Curves)

Run a continuous control loop with custom fan curves:

```bash
sudo nvctl control \
  --speed-pair 40:30 \
  --speed-pair 50:40 \
  --speed-pair 60:50 \
  --speed-pair 70:70 \
  --speed-pair 80:100 \
  --interval 5
```

This sets:
- 30% fan speed at 40°C
- 40% at 50°C
- 50% at 60°C
- 70% at 70°C
- 100% at 80°C+

Options:
- `--interval N` - Check temperature every N seconds (default: 5)
- `--single-use` - Apply once and exit
- `--retry` - Retry on errors
- `--retry-interval N` - Retry wait time in seconds (default: 10)
- `--default-speed N` - Speed below first curve point (default: 30)
- `--power-limit N` - Also enforce power limit in watts

With power limit:

```bash
sudo nvctl control \
  --speed-pair 60:50 \
  --speed-pair 80:100 \
  --power-limit 280
```

### Global Options

```bash
nvctl [OPTIONS] <COMMAND>

Options:
  -v, --verbose          Enable verbose output
      --format <FORMAT>  Output format [table|json|compact]
      --gpu <INDEX>      Target GPU by index (0-based)
      --gpu-name <NAME>  Target GPU by name (partial match)
      --gpu-uuid <UUID>  Target GPU by UUID
      --dry-run          Preview changes without applying
  -c, --config <FILE>    Path to config file
  -h, --help             Print help
  -V, --version          Print version
```

## Configuration

Create `~/.config/nvctl/config.toml`:

```toml
[general]
verbose = false
dry_run = false
interval = 5

[gpu]
index = 0

[fan]
default_speed = 30

[[fan.curve]]
temperature = 40
speed = 30

[[fan.curve]]
temperature = 60
speed = 50

[[fan.curve]]
temperature = 75
speed = 70

[[fan.curve]]
temperature = 85
speed = 100

[power]
limit_watts = 300

[thermal]
acoustic_limit = 83
```

Use with:

```bash
nvctl --config ~/.config/nvctl/config.toml control

# Or set environment variable
export NVCTL_CONFIG=~/.config/nvctl/config.toml
nvctl control
```

## Output Formats

### Table (default)

```
GPU 0: NVIDIA GeForce RTX 4090
  Temperature: 45°C
  Fan Speed: 35% (Auto)
  Power: 85W / 450W
```

### JSON

```bash
nvctl info --format json
```

```json
{
  "index": 0,
  "name": "NVIDIA GeForce RTX 4090",
  "temperature": 45,
  "fan_speed": 35,
  "fan_policy": "auto",
  "power_usage": 85,
  "power_limit": 450
}
```

### Compact

```bash
nvctl info --format compact
```

```
GPU0: 45°C 35% 85W/450W
```

## Permissions

Most read operations work without root. Write operations require root:

```bash
# Works without sudo
nvctl list
nvctl info
nvctl fan status
nvctl power status

# Requires sudo
sudo nvctl fan policy manual
sudo nvctl fan speed 75
sudo nvctl power limit 250
sudo nvctl thermal limit 80
```

For non-root access, add a udev rule:

```bash
# /etc/udev/rules.d/99-nvidia.rules
KERNEL=="nvidia[0-9]*", MODE="0666"
```

Then reload: `sudo udevadm control --reload-rules && sudo udevadm trigger`

## Troubleshooting

### "NVML library not found"

Ensure NVIDIA drivers are installed:

```bash
# Check driver
nvidia-smi

# Library location
ldconfig -p | grep libnvidia-ml

# Arch Linux
sudo pacman -S nvidia-utils

# Ubuntu/Debian
sudo apt install nvidia-utils-xxx  # Replace with driver version

# Fedora
sudo dnf install nvidia-driver-libs
```

### "Insufficient permissions"

Use sudo or configure udev rules (see Permissions section).

### "Fan control not available"

Some GPUs (especially mobile/laptop) don't support manual fan control via NVML. Check with `nvctl info --fan`.

### "Fan speed won't change"

1. Set policy to manual first: `sudo nvctl fan policy manual`
2. Then set speed: `sudo nvctl fan speed 75`

### "GPU not found"

```bash
# List available GPUs
nvctl list

# Check NVML directly
nvidia-smi -L

# Verify driver
lsmod | grep nvidia
```

## Architecture

```
CLI (clap) → Commands → Services → NVML Abstraction → Hardware
GUI (iced) → App State → Services → NVML Abstraction → Hardware
```

### CLI Structure (nvctl)

```
src/
├── main.rs           # Entry point
├── lib.rs            # Library exports
├── error.rs          # Error types (AppError, NvmlError, DomainError)
├── cli/
│   ├── args.rs       # CLI argument definitions
│   └── output.rs     # Output formatting
├── commands/         # Command handlers (list, info, fan, power, thermal, control)
├── domain/           # Validated types (FanSpeed, Temperature, PowerLimit)
├── services/         # Business logic (FanService, PowerService)
├── nvml/
│   ├── traits.rs     # GpuDevice trait abstraction
│   ├── device.rs     # Real NVML implementation
│   └── wrapper.rs    # NVML initialization
├── config/           # TOML configuration system
└── mock.rs           # Test mocks
```

### GUI Structure (nvctl-gui)

```
nvctl-gui/src/
├── main.rs           # Entry point
├── app.rs            # Iced application (state, update, view)
├── message.rs        # Message types for Elm architecture
├── state.rs          # Application state management
├── theme.rs          # Glossy theme colors and styling
├── views/            # Screen views (dashboard, fan, power, thermal, profiles)
├── widgets/          # Custom canvas widgets (gauges, graphs, curve editor)
└── services/         # GPU monitor, curve daemon, profiles, config
```

## Building

Uses a Makefile for all build and check operations:

```bash
# Build release binary (outputs to bin/)
make build

# Run all quality checks
make check

# Full CI pipeline (clean + checks + build)
make ci-full

# Show all available targets
make help
```

### Individual Commands

```bash
make release    # Build optimized release binary
make debug      # Build debug binary
make test       # Run tests
make lint       # Run clippy
make fmt        # Format code
make clean      # Remove build artifacts
make doc        # Generate and open documentation
make install    # Install to /usr/local/bin (requires sudo)
```

### Running

```bash
make run ARGS='list --format json'
make run-release ARGS='info --all'
```

## Shell Completions

nvctl supports tab-completion for bash, zsh, and fish.

### Generate and Install (Recommended)

```bash
# Generate completions for all shells
make completions

# Install system-wide (requires sudo)
sudo make install-completions
```

### Manual Generation

```bash
# Generate for specific shell
nvctl completions bash > nvctl.bash
nvctl completions zsh > _nvctl
nvctl completions fish > nvctl.fish
```

### Manual Installation

**Bash:**
```bash
# System-wide
sudo cp nvctl.bash /usr/share/bash-completion/completions/nvctl

# User only
mkdir -p ~/.local/share/bash-completion/completions
cp nvctl.bash ~/.local/share/bash-completion/completions/nvctl
```

**Zsh:**
```bash
# System-wide
sudo cp _nvctl /usr/share/zsh/site-functions/_nvctl

# User only (add to fpath in .zshrc)
mkdir -p ~/.zfunc
cp _nvctl ~/.zfunc/_nvctl
# Add to .zshrc: fpath=(~/.zfunc $fpath)
```

**Fish:**
```bash
# System-wide
sudo cp nvctl.fish /usr/share/fish/vendor_completions.d/nvctl.fish

# User only
cp nvctl.fish ~/.config/fish/completions/nvctl.fish
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Ensure no clippy warnings: `cargo clippy -- -D warnings`
6. Submit a pull request

### Code Standards

- No `.unwrap()` or `.expect()` in library code - use Result
- All public items must have `///` documentation
- Domain types validate input on construction
- Tests required for new functionality
- Follow existing patterns in the codebase

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [nvml-wrapper](https://crates.io/crates/nvml-wrapper) - Rust bindings for NVML
- [clap](https://crates.io/crates/clap) - CLI argument parsing
- [iced](https://crates.io/crates/iced) - Cross-platform GUI framework for the GUI application
- NVIDIA for the NVML library
