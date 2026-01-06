# /gui - Run the GUI Application

Launch the nvctl-gui application in release mode.

## What to do

1. Run `make gui` to start the GUI in release mode (smooth animations)
2. For debug mode with faster compilation: `make gui-dev`

## Commands

```bash
# Release mode (recommended for testing)
make gui

# Debug mode (faster compile)
make gui-dev
```

## Notes

- Release mode provides smoother animations
- Debug mode compiles faster but may have lower FPS
- GUI requires NVIDIA GPU with drivers installed
