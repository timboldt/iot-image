# Copilot Instructions for iot-image

## Project Overview

IoT image server (Rust/Axum) that fetches data from external APIs, generates SVGs, dithers them to a 6-color e-ink palette, and serves the result as a custom binary format (EPBM). An Arduino/ESP32-S3 client (reTerminal e1002) downloads and renders these images on a 7.3" color e-ink display.

## Build & Run Commands

### Rust Server
```bash
cd server
cargo build                    # Debug build
cargo build --release          # Release build (lto + strip + opt-level=z)
cargo run -- \
  --lat="37.7749" --lon="-122.4194" \
  --open-weather-key="KEY" \
  --stocks-api-key="KEY" \
  --stock-symbols="BTC/USD,QQQ,TSLA" \
  --fred-api-key="KEY" \
  --weight-data-dir="/path/to/csvs"
```

### Arduino Client
```bash
cd client
arduino-cli compile --verbose --fqbn esp32:esp32:esp32s3 .
arduino-cli upload --verbose --fqbn esp32:esp32:esp32s3 --port /dev/cu.wchusbserial110
arduino-cli monitor -p /dev/cu.wchusbserial110 -c baudrate=115200
```

### Deploy Server (local Linux, systemd)
```bash
cd server && cargo build --release && ../deploy/deploy_server.sh
sudo journalctl -u iot-image-server -f
```

## Architecture

### Data Flow (Server)
Each data source follows an identical pipeline:
1. `fetch_X()` — async function fetches external API data
2. `generate_X_svg()` — generates an 800×480 SVG string
3. SVG written to `/tmp/X.svg`
4. `bitmap::render_svg_to_bitmap()` rasterizes via `resvg`/`tiny-skia`
5. Atkinson dithering maps pixels → 6 e-ink colors (CIELAB Delta E distance)
6. `EpdBitmap::to_bytes()` serializes as EPBM binary
7. Axum handler returns `application/octet-stream`

Each data source has a parallel `/svg` endpoint for browser preview.

### EPBM Binary Format
Custom format (`bitmap.rs`):
- 4 bytes: magic `EPBM`
- 2 bytes big-endian: width (800)
- 2 bytes big-endian: height (480)
- width×height bytes: 1 byte per pixel (0=Black, 1=White, 2=Green, 3=Blue, 4=Red, 5=Yellow)

Total: 384,008 bytes for 800×480.

### Server HTTP Endpoints (port 8080)
| Path | Format |
|------|--------|
| `/weather/seed-e1002.bin` | EPBM binary |
| `/stocks/seed-e1002.bin` | EPBM binary |
| `/fred/seed-e1002.bin` | EPBM binary |
| `/weight/forecast/seed-e1002.bin` | EPBM binary |
| `/weight/velocity/seed-e1002.bin` | EPBM binary |
| `/weather/svg`, `/stocks/svg`, etc. | SVG preview |

Query params: `battery_pct` (u8), `date` (YYYYMMDD), `duration` (days), `user` (weight CSV name).

### Client (Arduino/ESP32-S3)
- Boots → checks wake reason (button vs. timer) → connects WiFi → syncs NTP → downloads EPBM → renders pixel-by-pixel → deep sleep
- Display mode (`MODE_WEATHER`, `MODE_STOCKS`, `MODE_FRED`, `MODE_WEIGHT_*`) persists across deep sleep via `RTC_DATA_ATTR`
- Wakes at scheduled times (6 AM, 12 PM, 6 PM) using NTP; falls back to 6-hour interval if NTP fails

## Key Conventions

### Adding a New Data Source
Follow the existing module pattern:
1. Create `server/src/X.rs` with `fetch_X()` (async, returns data struct) and `generate_X_svg()` (returns `String`)
2. Add module to `main.rs`, register two Axum routes (`/X/seed-e1002.bin` and `/X/svg`)
3. Handler pattern is identical to `get_weather_bitmap` / `get_weather_svg` — copy and adapt
4. Add required CLI arg to `Args` struct and `AppState`

### Color Matching
- `bitmap.rs` uses CIELAB Delta E with pre-computed `PALETTE_LAB` constants
- Saturation is boosted 1.2× before matching to compensate for e-paper's less vivid pigments
- Do not change `PALETTE_LAB` without recalculating from `PALETTE` RGB values using D65 illuminant
- Error fallback always returns `generate_test_bitmap()` (6-color bars)

### Debug Output
`render_svg_to_bitmap()` always writes `debug_render.png` to the CWD and prints color usage percentages to stdout. This is intentional for development.

### Credentials & Secrets
- Server: API keys via CLI args (see `deploy/bundle/env.txt.example` for systemd env file format)
- Client: WiFi credentials in `arduino_secrets.h` (git-ignored); copy from `arduino_secrets.h.template`
- `SECRET_HOST` in `arduino_secrets.h` sets the server hostname/IP

### Release Profile
`Cargo.toml` uses aggressive size optimization (`lto=true`, `opt-level="z"`, `codegen-units=1`, `panic="abort"`, `strip=true`). Debug builds use default settings.
