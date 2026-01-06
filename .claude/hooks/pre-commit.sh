#!/bin/bash
# Pre-commit hook for nvctl-gui

set -e

echo "Running pre-commit checks for nvctl-gui..."

# Check if nvctl-gui files are staged
if git diff --cached --name-only | grep -q "^nvctl-gui/"; then
    echo "Checking nvctl-gui..."

    # Format check
    echo "  Checking formatting..."
    cargo fmt --package nvctl-gui -- --check

    # Clippy
    echo "  Running clippy..."
    cargo clippy --package nvctl-gui -- -D warnings

    # Tests
    echo "  Running tests..."
    cargo test --package nvctl-gui

    # Check for unwrap/expect in src (not tests)
    echo "  Checking for unwrap/expect..."
    if grep -r "\.unwrap()\|\.expect(" nvctl-gui/src/*.rs nvctl-gui/src/**/*.rs 2>/dev/null | grep -v "#\[test\]" | grep -v "mod tests"; then
        echo "ERROR: Found unwrap() or expect() in source code"
        exit 1
    fi

    echo "nvctl-gui checks passed!"
fi
