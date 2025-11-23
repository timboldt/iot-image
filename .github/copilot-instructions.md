# AI Coding Agent Instructions for iot-image

## Project Overview

`iot-image` is a service for generating images suitable for IoT devices with constrained resources. The system consists of two components:
- **Server (Rust)** - Runs on Raspberry Pi Zero 2 W (512 MB RAM, ARM Cortex-A53 CPU), fetches weather data, generates optimized images
- **Client (Arduino/ESP32-S3)** - Runs on Seed Studio reTerminal e1002, downloads and displays images on 7.3" e-ink display

The server downloads weather data from a public API, generates device-optimized images, and serves them via HTTP. The client periodically fetches the latest image and renders it to the e-ink display.

## Developer Workflows

### Rust Server - Build & Run (Local Development)
```bash
cd server
cargo build        # Debug build
cargo build --release  # Optimized build
cargo run -- --lat="$OPEN_WEATHER_LAT" --lon="$OPEN_WEATHER_LON" --open-weather-key="$OPEN_WEATHER_KEY"
```

### Rust Server - Cross-Compilation for Raspberry Pi Zero 2 W
Target: `aarch64-unknown-linux-gnu` (64-bit ARM)

**Setup cross-compilation:**
```bash
rustup target add aarch64-unknown-linux-gnu
cargo install cross  # Use 'cross' crate for easier compilation
```

**Build for RPi:**
```bash
cd server
cross build --release --target aarch64-unknown-linux-gnu
# Binary: target/aarch64-unknown-linux-gnu/release/iot-image-server
```

**Deploy to RPi (via SCP or similar):**
```bash
scp server/target/aarch64-unknown-linux-gnu/release/iot-image-server pidev:~/bin
ssh pidev ~/bin/iot-image-server --lat="37.7749" --lon="-122.4194" --open-weather-key="YOUR_KEY"
```

### Arduino Client - Setup & Build (ESP32-S3 on reTerminal e1002)

**Install Arduino CLI:**
```bash
# macOS
brew install arduino-cli

# Linux
curl -fsSL https://raw.githubusercontent.com/arduino/arduino-cli/master/install.sh | sh
```

**Initialize Arduino CLI and add ESP32 board support:**
```bash
arduino-cli core install esp32:esp32
```

**Build and upload to device:**
```bash
cd client
arduino-cli compile --fqbn esp32:esp32:esp32s3 .
arduino-cli upload -p /dev/ttyUSB0 --fqbn esp32:esp32:esp32s3 .
```

**Monitor serial output:**
```bash
arduino-cli monitor -p /dev/ttyUSB0 -c baudrate=115200
```

## Architecture & Data Flow

1. **Rust Server** fetches weather data from OpenWeatherMap API
2. **Server** generates optimized images (PBM/PNG format) suitable for e-ink display
3. **ESP32-S3 Client** periodically downloads images from server via HTTP
4. **Client** renders images on 7.3" e-ink display using Seed Studio drivers

## Code Organization

```
iot-image/
├── server/              # Rust backend service
│   ├── src/
│   │   ├── main.rs      # CLI orchestration & weather printing
│   │   └── weather.rs   # OpenWeatherMap API integration
│   └── Cargo.toml
├── client/              # Arduino/ESP32-S3 frontend
│   ├── client.ino       # Main sketch
│   ├── config.h         # WiFi & server configuration
│   └── libraries/       # Seed Studio display drivers
└── .github/
    └── copilot-instructions.md
```

## Rust Server - Architecture & Patterns

### Code Structure (`server/src/`)
- **`main.rs`** - CLI orchestration, weather data printing
  - Uses `clap` derive macros for argument parsing: `--lat`, `--lon`, `--open-weather-key`
  - Helper functions for weather categorization: `temperature_text()`, `humidity_text()`, `wind_text()`
  - Prints 6-day forecast with temperatures, humidity, wind, sunrise/sunset times

- **`weather.rs`** - OpenWeatherMap API integration
  - Data structures: `WeatherData`, `CurrentWeather`, `DailyWeather`, `TempRange`, `Weather`
  - `fetch_weather()` async function queries OpenWeatherMap 3.0 One Call API
  - Includes all fields: temperature ranges (min/max/morn/day/night), humidity, wind_speed, sunrise/sunset, weather icons

### Dependencies
- `reqwest` (0.11) - HTTP client with JSON support
- `tokio` (1.x) - Async runtime
- `chrono` (0.4.38) - DateTime parsing and formatting
- `clap` (4.4) - CLI argument parsing with derive macros
- `serde` / `serde_json` - JSON serialization for API responses

### Release Optimizations (for RPi)
```toml
[profile.release]
lto = true              # Link-time optimization
strip = true            # Strip debug symbols
opt-level = "z"         # Optimize for size
```

## Arduino Client - Architecture & Patterns

### Hardware Target
- **Device:** Seed Studio reTerminal e1002
- **Processor:** ESP32-S3 (32-bit Xtensa dual-core, 240 MHz)
- **Display:** The reTerminal E1002 features a 7.3-inch full-color e-paper display with an 800x480 pixel resolution and a color depth of 6 colors (plus black and white). 
- **Connectivity:** WiFi (2.4 GHz 802.11 b/g/n)

### Code Structure (`client/`)
- **`client.ino`** - Main Arduino sketch
  - WiFi connection with SSID/password from `config.h`
  - HTTP client to download image from server
  - Display driver calls to render image on e-ink screen
  - Update loop with configurable interval (15-30 minutes typical)

- **`config.h`** - Configuration constants
  - WiFi credentials (never commit actual values)
  - Server URL/IP and port
  - Update interval in seconds
  - Display pin assignments (SPI: MOSI, CLK, CS, DC, RST, BUSY)

### Key Libraries
- **WiFi** - Built-in ESP32 WiFi support
- **HTTPClient** - Built-in HTTP request library
- **Seed Studio display driver** - e-ink display communication
- Optional: **ArduinoJson** - JSON parsing if client needs to parse server metadata

### WiFi & Network Handling
- Store credentials in `config.h` as string constants (or use Arduino Secrets tab)
- Implement WiFi reconnection logic: if connection drops, attempt reconnect
- Add timeout for HTTP requests to prevent hanging on network issues

### Display Update Pattern
```
1. Connect to WiFi
2. GET /image/latest.png (or similar endpoint) from server
3. Write binary image data to display buffer
4. Call display.refresh() (note: eink displays need 180+ sec between refreshes)
5. Sleep for configured interval
6. Repeat
```

## Integration Points

### Server → Client Communication
- **Server exposes HTTP endpoint** to serve generated image file
  - Suggested endpoint: `/image/latest.png` or `/image/current`
  - Returns binary PNG/PBM data
  
- **Client queries endpoint periodically**
  - Request: `GET http://raspberrypi.local:8080/image/latest`
  - Response: Binary image data (50-150 KB typical)

### Environment & Configuration
- **Server:** Environment variables or CLI args for `OPEN_WEATHER_LAT`, `OPEN_WEATHER_LON`, `OPEN_WEATHER_KEY`
- **Client:** Hardcoded constants in `config.h` for server URL, WiFi SSID, WiFi password
- Both should handle network failures gracefully (timeouts, retries)

## Testing & Validation

### Rust Server
- Build: `cd server && cargo build --release`
- Run locally: `cargo run -- --lat="37.7749" --lon="-122.4194" --open-weather-key="YOUR_KEY"`
- Verify output: Check console for 6-day forecast with temperature categories, humidity, wind

### Arduino Client
- Monitor serial: `arduino-cli monitor -p /dev/ttyUSB0`
- Expected output: WiFi connection status, HTTP download status, image render confirmation
- Verify on device: Check if image appears on e-ink display after upload

### End-to-End
1. Start server on RPi: `./iot-image-server --lat=... --lon=... --open-weather-key=...`
2. Upload client to ESP32-S3 with server IP in `config.h`
3. Verify image appears on e-ink display within expected update interval
4. Monitor serial output for any WiFi/HTTP errors
