#!/bin/bash
# Pre-commit hook for nvctl workspace

set -e

echo "Running pre-commit checks..."

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
    cargo test --package nvctl-gui --quiet

    # Check for unwrap/expect in src (not tests)
    echo "  Checking for unwrap/expect..."
    if find nvctl-gui/src -name "*.rs" -exec grep -l "\.unwrap()\|\.expect(" {} \; 2>/dev/null | head -1 | grep -q .; then
        echo "WARNING: Found unwrap() or expect() in source code - review manually"
    fi

    echo "nvctl-gui checks passed!"
fi

# Check if nvctl lib files are staged
if git diff --cached --name-only | grep -q "^src/"; then
    echo "Checking nvctl lib..."

    # Format check
    echo "  Checking formatting..."
    cargo fmt --package nvctl -- --check

    # Clippy
    echo "  Running clippy..."
    cargo clippy --package nvctl -- -D warnings

    # Tests
    echo "  Running tests..."
    cargo test --package nvctl --quiet

    echo "nvctl lib checks passed!"
fi

echo "All pre-commit checks passed!"
