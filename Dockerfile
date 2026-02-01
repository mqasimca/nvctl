# Dockerfile for nvctl builds
# Includes Rust and all system dependencies for CLI and GUI builds

FROM rust:latest

# Install system dependencies for GUI builds
RUN apt-get update && apt-get install -y \
    libdbus-1-dev \
    pkg-config \
    libxkbcommon-dev \
    libwayland-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Keep cargo cache writable
RUN chmod -R 777 /usr/local/cargo

# Default command
CMD ["cargo", "build", "--release"]
