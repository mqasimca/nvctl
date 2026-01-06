# /check - Run All Quality Checks

Run the full quality check suite for the nvctl workspace.

## What to do

1. Run `make check` which executes:
   - `cargo fmt --all -- --check` (formatting)
   - `cargo clippy --all -- -D warnings` (linting)
   - `cargo test --all` (tests)

2. Report results clearly:
   - If all pass: "All checks passed"
   - If any fail: Show the specific error and suggest fix

## Command

```bash
make check
```

## On failure

- Format errors: Run `cargo fmt --all` to fix
- Clippy errors: Read the suggestion and apply fix
- Test failures: Investigate and fix the failing test
