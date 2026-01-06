# /build - Build Release Binaries

Build optimized release binaries for both CLI and GUI.

## What to do

1. Run `make build` for CLI only, or `make build-all` for both CLI and GUI
2. Binaries are output to `bin/` directory
3. Report build success with binary locations and sizes

## Commands

```bash
# CLI only
make build

# Both CLI and GUI
make build-all

# GUI only
make gui-build
```

## Output

- CLI binary: `bin/nvctl`
- GUI binary: `bin/nvctl-gui`

## On failure

- Check for compilation errors
- Ensure dependencies are available
- Run `make check` first to catch issues early
