# nvctl

A fast, safe NVIDIA GPU control tool for Linux. Control fan speeds, power limits, and monitor thermals via NVML.

## Features

- **Fan Control** - Manual speed control and automatic fan curves
- **Power Management** - Set and monitor GPU power limits
- **Thermal Monitoring** - Real-time temperature readouts
- **Acoustic Limit** - Set GPU temperature target (GPU throttles to maintain temp)
- **Multi-GPU Support** - Target specific GPUs by index, name, or UUID
- **Multiple Output Formats** - Table, JSON, or compact output
- **Dry Run Mode** - Preview changes before applying
- **Daemon Mode** - Continuous control loop with custom fan curves

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/your-username/nvctl.git
cd nvctl

# Build release binary
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

### Requirements

- NVIDIA GPU with proprietary driver installed
- NVIDIA Management Library (NVML) - included with nvidia-utils
- Root/sudo access for fan and power control

## Quick Start

```bash
# List all GPUs
nvctl list

# Show GPU information
nvctl info

# Set fan speed to 70%
sudo nvctl fan speed 70

# Set power limit to 200W
sudo nvctl power limit 200

# Start fan curve daemon
sudo nvctl control --speed-pair 50:40 --speed-pair 70:70 --speed-pair 85:100
```

## Usage

### List GPUs

```bash
nvctl list
nvctl list --format json
```

### GPU Information

```bash
# Basic info
nvctl info

# Detailed info
nvctl info --all

# Specific categories
nvctl info --fan
nvctl info --power
nvctl info --thermal

# Target specific GPU
nvctl --gpu 0 info
nvctl --gpu-name "RTX 4090" info
```

### Fan Control

```bash
# Show fan status
nvctl fan status

# Set to automatic (GPU-controlled)
sudo nvctl fan policy auto

# Set to manual control
sudo nvctl fan policy manual

# Set fan speed (requires manual policy)
sudo nvctl fan speed 75

# Target specific fan
sudo nvctl fan speed 80 --fan-index 0

# Dry run (preview only)
nvctl --dry-run fan speed 100
```

### Power Control

```bash
# Show power status
nvctl power status

# Set power limit in watts
sudo nvctl power limit 250

# Dry run
nvctl --dry-run power limit 300
```

### Thermal Control

Control the acoustic temperature limit. This tells the GPU to throttle performance to maintain a target temperature (same as GeForce Experience temperature target).

```bash
# Show thermal status and acoustic limits
nvctl thermal status

# Set acoustic temperature limit (GPU throttles to stay at/below this temp)
sudo nvctl thermal limit 80

# Dry run
nvctl --dry-run thermal limit 75
```

Note: Not all GPUs support acoustic temperature limits. If unsupported, you'll see "Not supported" in the status output.

### Control Daemon

Run a continuous control loop with custom fan curves:

```bash
# Basic fan curve
sudo nvctl control \
  --speed-pair 50:30 \
  --speed-pair 65:50 \
  --speed-pair 75:70 \
  --speed-pair 85:100

# With power limit
sudo nvctl control \
  --speed-pair 60:50 \
  --speed-pair 80:100 \
  --power-limit 280

# Custom interval (10 seconds)
sudo nvctl control --interval 10 --speed-pair 70:60

# Single execution (no loop)
sudo nvctl control --single-use --speed-pair 60:50
```

Fan curve format: `TEMP:SPEED` where:
- `TEMP` = GPU temperature threshold in Celsius
- `SPEED` = Fan speed percentage (0-100)

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

nvctl can read settings from a TOML configuration file:

```bash
# Use config file
nvctl --config ~/.config/nvctl/config.toml control

# Or set via environment
export NVCTL_CONFIG=~/.config/nvctl/config.toml
nvctl control
```

Example config file:

```toml
[fan]
default_speed = 30
curve = [
  { temp = 50, speed = 30 },
  { temp = 65, speed = 50 },
  { temp = 75, speed = 70 },
  { temp = 85, speed = 100 },
]

[power]
limit = 280

[control]
interval = 5
retry = true
retry_interval = 10
```

## Output Formats

### Table (default)

```
GPU 0: NVIDIA GeForce RTX 4090
├── Temperature: 45°C
├── Fan Speed:   35%
├── Power:       85W / 450W
└── Utilization: 12%
```

### JSON

```bash
nvctl --format json info
```

```json
{
  "index": 0,
  "name": "NVIDIA GeForce RTX 4090",
  "temperature": 45,
  "fan_speed": 35,
  "power_draw": 85,
  "power_limit": 450
}
```

### Compact

```bash
nvctl --format compact info
```

```
GPU0: 45°C 35% 85/450W
```

## Troubleshooting

### Permission Denied

Fan and power control require root access:

```bash
sudo nvctl fan speed 70
```

Or add yourself to the video group and set up udev rules.

### NVML Library Not Found

Install the NVIDIA driver and utilities:

```bash
# Arch Linux
sudo pacman -S nvidia-utils

# Ubuntu/Debian
sudo apt install nvidia-utils-xxx  # Replace xxx with driver version

# Fedora
sudo dnf install nvidia-driver-libs
```

### No GPUs Found

1. Verify NVIDIA driver is loaded: `nvidia-smi`
2. Check GPU detection: `lspci | grep -i nvidia`
3. Ensure nvidia modules are loaded: `lsmod | grep nvidia`

### Fan Speed Won't Change

1. Set fan policy to manual first: `sudo nvctl fan policy manual`
2. Some GPUs don't support manual fan control
3. Check `nvctl info --fan` for supported features

## Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run lints
cargo clippy

# Format code
cargo fmt

# Generate docs
cargo doc --open
```

## Project Structure

```
nvctl/
├── src/
│   ├── main.rs          # Entry point
│   ├── lib.rs           # Library root
│   ├── error.rs         # Error types
│   ├── cli/             # CLI definitions
│   ├── commands/        # Command handlers
│   ├── domain/          # Domain models
│   ├── services/        # Business logic
│   ├── nvml/            # NVML abstraction
│   └── config/          # Configuration
├── tests/               # Integration tests
├── Cargo.toml
└── README.md
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure `cargo test` and `cargo clippy` pass
5. Submit a pull request

## Acknowledgments

- [nvml-wrapper](https://github.com/Cldfire/nvml-wrapper) - Rust bindings for NVML
- [clap](https://github.com/clap-rs/clap) - Command line argument parser
