# nvctl Makefile
# Build and development automation

.PHONY: all build release debug install clean check test lint fmt doc help
.PHONY: ci ci-quick ci-full run completions install-completions

# Disable parallel execution to avoid cargo lock conflicts
.NOTPARALLEL:

# Configuration
BINARY := nvctl
BIN_DIR := bin
CARGO := cargo
INSTALL_PATH := /usr/local/bin

# Default target
all: build

# =============================================================================
# Build Targets
# =============================================================================

## Build release binary to bin/
build: release

## Build optimized release binary
release:
	@mkdir -p $(BIN_DIR)
	$(CARGO) build --release
	@cp target/release/$(BINARY) $(BIN_DIR)/$(BINARY)
	@echo "Built: $(BIN_DIR)/$(BINARY)"

## Build debug binary
debug:
	@mkdir -p $(BIN_DIR)
	$(CARGO) build
	@cp target/debug/$(BINARY) $(BIN_DIR)/$(BINARY)-debug
	@echo "Built: $(BIN_DIR)/$(BINARY)-debug"

## Install binary to system (requires sudo)
install: release
	@sudo cp $(BIN_DIR)/$(BINARY) $(INSTALL_PATH)/$(BINARY)
	@echo "Installed: $(INSTALL_PATH)/$(BINARY)"

## Remove build artifacts
clean:
	$(CARGO) clean
	@rm -rf $(BIN_DIR) $(COMPLETIONS_DIR)
	@echo "Cleaned build artifacts"

# =============================================================================
# Quality Checks
# =============================================================================

## Run all checks (fmt, lint, test)
check: fmt-check lint test
	@echo "All checks passed!"

## Run tests
test:
	$(CARGO) test

## Run clippy linter
lint:
	$(CARGO) clippy -- -D warnings

## Check code formatting
fmt-check:
	$(CARGO) fmt --check

## Format code
fmt:
	$(CARGO) fmt

## Generate documentation
doc:
	$(CARGO) doc --no-deps --open

## Check documentation builds
doc-check:
	$(CARGO) doc --no-deps

# =============================================================================
# CI Targets
# =============================================================================

## Quick CI check (fmt + lint + test)
ci-quick: fmt-check lint test
	@echo "CI quick checks passed!"

## Full CI pipeline (clean build + all checks + release)
ci-full: clean fmt-check lint test release
	@echo "CI full pipeline passed!"
	@echo "Binary: $(BIN_DIR)/$(BINARY)"
	@ls -lh $(BIN_DIR)/$(BINARY)

## Alias for ci-full
ci: ci-full

# =============================================================================
# Development
# =============================================================================

## Run debug build
run: debug
	./$(BIN_DIR)/$(BINARY)-debug $(ARGS)

## Run release build
run-release: release
	./$(BIN_DIR)/$(BINARY) $(ARGS)

## Watch for changes and run tests
watch:
	$(CARGO) watch -x test

## Show binary size info
size: release
	@echo "Binary size:"
	@ls -lh $(BIN_DIR)/$(BINARY)
	@echo ""
	@echo "Stripped size estimate:"
	@strip -o /tmp/$(BINARY)-stripped $(BIN_DIR)/$(BINARY) && ls -lh /tmp/$(BINARY)-stripped

# =============================================================================
# Shell Completions
# =============================================================================

COMPLETIONS_DIR := completions

## Generate shell completions for all shells
completions: release
	@mkdir -p $(COMPLETIONS_DIR)
	@echo "Generating bash completions..."
	@./$(BIN_DIR)/$(BINARY) completions bash > $(COMPLETIONS_DIR)/$(BINARY).bash
	@echo "Generating zsh completions..."
	@./$(BIN_DIR)/$(BINARY) completions zsh > $(COMPLETIONS_DIR)/_$(BINARY)
	@echo "Generating fish completions..."
	@./$(BIN_DIR)/$(BINARY) completions fish > $(COMPLETIONS_DIR)/$(BINARY).fish
	@echo "Completions generated in $(COMPLETIONS_DIR)/"
	@ls -la $(COMPLETIONS_DIR)/

## Install completions to system directories (requires sudo)
install-completions: completions
	@echo "Installing bash completions..."
	@sudo install -Dm644 $(COMPLETIONS_DIR)/$(BINARY).bash /usr/share/bash-completion/completions/$(BINARY)
	@echo "Installing zsh completions..."
	@sudo install -Dm644 $(COMPLETIONS_DIR)/_$(BINARY) /usr/share/zsh/site-functions/_$(BINARY)
	@echo "Installing fish completions..."
	@sudo install -Dm644 $(COMPLETIONS_DIR)/$(BINARY).fish /usr/share/fish/vendor_completions.d/$(BINARY).fish
	@echo "Completions installed!"
	@echo ""
	@echo "Reload your shell or run:"
	@echo "  bash: source /usr/share/bash-completion/completions/$(BINARY)"
	@echo "  zsh:  compinit"
	@echo "  fish: (automatic)"

# =============================================================================
# Help
# =============================================================================

## Show this help
help:
	@echo "nvctl - NVIDIA GPU Control Tool"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build:"
	@echo "  build       Build release binary to bin/"
	@echo "  release     Build optimized release binary"
	@echo "  debug       Build debug binary"
	@echo "  install     Install to $(INSTALL_PATH) (requires sudo)"
	@echo "  clean       Remove build artifacts"
	@echo ""
	@echo "Quality:"
	@echo "  check       Run all checks (fmt, lint, test)"
	@echo "  test        Run tests"
	@echo "  lint        Run clippy linter"
	@echo "  fmt         Format code"
	@echo "  fmt-check   Check code formatting"
	@echo "  doc         Generate and open documentation"
	@echo ""
	@echo "CI:"
	@echo "  ci          Full CI pipeline (clean + checks + release)"
	@echo "  ci-quick    Quick checks (fmt + lint + test)"
	@echo "  ci-full     Full CI pipeline"
	@echo ""
	@echo "Completions:"
	@echo "  completions         Generate shell completions to completions/"
	@echo "  install-completions Install completions system-wide (requires sudo)"
	@echo ""
	@echo "Development:"
	@echo "  run         Build and run debug binary"
	@echo "  run-release Build and run release binary"
	@echo "  size        Show binary size info"
	@echo ""
	@echo "Pass arguments with: make run ARGS='list --format json'"
