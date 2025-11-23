# AI Coding Agent Instructions for iot-image

## Project Overview

`iot-image` is a service for generating images suitable for IoT devices with constrained resources. The service will run on a Raspberry Pi Zero 2 W (512 MB RAM, ARM Cortex-A53 CPU) and produce images in a format suitable for a Seed Studio 7.3" E-Ink display, specifically the [reTerminal e1002](https://wiki.seeedstudio.com/getting_started_with_reterminal_e1002/).

The service is written in Rust.

It downloads weather data from a public API, generates device-optimized images, and serves them via a lightweight HTTP server.

## Developer Workflows

### Build & Run (Local Development)
```bash
cargo build        # Debug build
cargo build --release  # Optimized build
cargo run -- --lat="$OPEN_WEATHER_LAT" --lon="$OPEN_WEATHER_LON" --open-weather-key="$OPEN_WEATHER_KEY" # Run it locally with required args.
```

### Cross-Compilation for Raspberry Pi Zero 2 W
Target: `aarch64-unknown-linux-gnu` (64-bit ARM)

**Setup cross-compilation:**
```bash
rustup target add aarch64-unknown-linux-gnu
cargo install cross  # Use 'cross' crate for easier compilation
```

**Build for RPi:**
```bash
cross build --release --target aarch64-unknown-linux-gnu
# Binary: target/aarch64-unknown-linux-gnu/release/iot-image
```

**Deploy to RPi (via SCP or similar):**
```bash
scp target/aarch64-unknown-linux-gnu/release/iot-image pidev:~/bin
ssh pidev ~/bin/iot-image
```
