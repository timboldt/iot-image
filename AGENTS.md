# Agent Instructions for iot-image

Guidance for AI coding agents working in this repository. This is the single
source of truth for how the project is built, run, and deployed.

## Project Overview

A server that renders dashboards for a battery-powered e-ink display, plus the
firmware that shows them.

- **Server (Rust / Axum)** — fetches data from external APIs, builds an SVG,
  rasterises it, dithers it to the display's 6-colour palette, and serves the
  result as a raw bitmap.
- **Client (Arduino / ESP32-S3)** — a Seeed Studio reTerminal e1002 with a 7.3"
  colour e-paper panel. Wakes on a schedule or a button press, streams the
  bitmap, renders it, and returns to deep sleep.
- **Tri-colour client (Arduino / ESP32-S2)** — an Adafruit Feather ESP32-S2 with
  a 2.9" IL0373 white/black/red FeatherWing (296x128). Shows a compact weather
  screen (`weather-mini`) at 06:00 and 18:00.

The server holds no database and caches nothing between requests: every request
re-fetches upstream data and re-renders.

## Repository Layout

```
iot-image/
├── server/src/
│   ├── main.rs        # CLI args, shared state, route registration
│   ├── weather.rs     # OpenWeatherMap One Call 3.0 + forecast/overview SVGs
│   ├── stocks.rs      # Twelve Data candlesticks + 2x2 chart grid SVG
│   ├── fred.rs        # FRED series, yield-curve analysis + "crash monitor" SVG
│   ├── weight.rs      # CSV weight data, projections + forecast/velocity SVGs
│   ├── kalman.rs      # Shared constant-velocity Kalman filter
│   ├── bitmap.rs      # SVG -> RGBA -> dithered EPBM bitmap
│   └── svg_common.rs  # SVG snippets shared across the generators
├── client/
│   ├── client.ino     # Firmware: wake, fetch, stream-render, sleep
│   ├── config.h       # Pins, wake schedule, timezone
│   └── arduino_secrets.h  # WiFi + server host (git-ignored)
├── client-tricolor/   # Same, for the 296x128 tri-colour Feather
│   ├── client-tricolor.ino
│   ├── config.h
│   └── arduino_secrets.h  # git-ignored
├── deploy/            # systemd unit and deployment script
├── assets/static/     # Weather icon sets (loaded at runtime; see gotcha below)
│                      #   tricolor/{20,70}px/ — 3-colour PNGs for weather-mini
└── data/              # Weight CSVs (git-ignored)
```

## Build & Run

```bash
cd server
cargo build --release
cargo clippy --all-targets     # expected to be warning-free
cargo test
```

Running locally requires every API key, because all arguments are mandatory:

```bash
cargo run -- \
  --lat=37.7749 --lon=-122.4194 \
  --open-weather-key=KEY \
  --stocks-api-key=KEY \
  --stock-symbols="BTC/USD,QQQ,IONQ,TSLA" \
  --fred-api-key=KEY \
  --weight-data-dir=../data \
  --port=8080
```

**Gotcha:** weather icons are loaded from `assets/static/fill-svg-static/` at
runtime, resolved *relative to the working directory*. Run the server from the
repository root (or from the install directory, which has its own `assets/`
copy), otherwise icons silently do not render — the code skips an icon it cannot
read rather than failing the request.

## Endpoints

Every endpoint is served in two forms, registered together by `register()` in
`main.rs` with the `DisplayProfile` it renders for:

- `/<base>/<suffix>` — EPBM bitmap for the device
- `/<base>/svg` — the same SVG, for previewing in a browser

| Profile | Suffix | Size | Colours | Dither | Bases |
|---|---|---|---|---|---|
| `SEED_E1002` | `seed-e1002.bin` | 800x480 | all 6 | Atkinson | `weather`, `weather-overview`, `stocks`, `fred`, `weight/forecast`, `weight/velocity` |
| `FEATHER_TRICOLOR` | `tricolor-296.bin` | 296x128 | black/white/red | none | `weather-mini` |

Keep the `ENDPOINTS` list (startup banner) in step with the `register()` calls.

Query parameters: `battery_pct`, `lat`/`lon` (weather, overriding the defaults),
`user` (weight, selects `<dir>/<user>.csv`), `date`/`duration` (FRED).

Failures never return an HTTP error. A `.bin` request falls back to the colour-bar
test pattern; an `/svg` request returns an SVG containing the error text. This is
deliberate: the device has no way to display an HTTP status.

## Rendering Pipeline

1. A per-endpoint `async fn` returns `SvgResult` (`Result<String, String>`).
2. `bitmap::render_svg_to_bitmap` parses the SVG with `usvg`, rasterises via
   `resvg`/`tiny-skia` onto a white background.
3. Each pixel is matched to the nearest palette entry in CIELAB space, after a
   20% saturation boost that compensates for e-paper's muted pigments.
4. Only the profile's palette is searched. With dithering, Atkinson error
   diffusion spreads the quantisation error (6/8 of it; the rest is absorbed,
   which keeps contrast crisp). Without it (tri-colour), each pixel snaps to the
   nearest colour, which keeps small text legible at 296x128.

The system font database is expensive to build, so it is loaded once into a
`OnceLock` and shared by every render. Do not move it back inside the render path.

### EPBM format

8-byte header — `"EPBM"` magic, then width and height as big-endian `u16` —
followed by one byte per pixel. For 800x480 that is exactly 384,008 bytes. Byte
values index the palette: 0 black, 1 white, 2 green, 3 blue, 4 red, 5 yellow.
A tri-colour bitmap is the same format using only 0, 1 and 4: 37,896 bytes.

## Chart Conventions (server)

- `fred.rs` positions everything through `ChartFrame`, which owns the mapping
  from data values to pixels. Curve, threshold lines, gradient stops and axis
  labels must all go through the same frame — deriving a second scale is what
  previously put the dashed threshold lines at the wrong height.
- Ranges are padded so regime thresholds are always on screen, and those
  thresholds are then always drawn.
- `weather.rs` draws its meters with `GradientBar`; add new meters through it
  rather than emitting the rect/clipPath/rect trio by hand.
- The weather "today" panel is 24 hours of hourly sparklines positioned through
  `SparkFrame`, which shares one time axis across rows. Colour rows with
  `SparkFrame::smooth_gradient`, which anchors the gradient at its stop
  values so an autoscaled row still colours a value as the meter bars do.
- SVG `clipPath` and gradient ids must be unique within a document. Charts that
  can repeat build ids from their coordinates.
- `generate_weather_mini_svg` (tri-colour) uses only `black`, `white`, `red`,
  bold DejaVu Sans at 11px or larger, and red only for things needing
  attention (alerts, rain, <=32°F / >=90°F, low battery). With no dithering,
  a 1px line must sit on a pixel centre (x.5): otherwise it renders as two grey
  pixels that snap to white and disappear.

## Client Behaviour

- **Wake schedule:** 06:00, 12:00 and 18:00 local (`WAKE_HOUR_*` in `config.h`),
  or any button press. Falls back to a 6-hour timer if NTP never syncs.
- **Buttons** — the mode persists across deep sleep in `RTC_DATA_ATTR` memory:
  - `BUTTON_KEY2` (left) — weather: forecast <-> overview
  - `BUTTON_KEY1` (middle) — finance: stocks <-> FRED
  - `BUTTON_KEY0` (right, green) — refresh the current screen
  The weight modes are still defined but no button selects them.
- **Streaming:** the bitmap is *not* buffered whole. It is read in
  `STREAM_BUFFER_SIZE` (8 KB) chunks and drawn pixel-by-pixel as it arrives,
  because 384 KB will not fit alongside the framebuffer.
- **Battery** is read from an ADC behind an enable pin and sent up as
  `battery_pct` so the server can draw the gauge.
- On any failure the client draws a red border as a "stale data" indicator.

### Build & flash

```bash
cd client
arduino-cli compile --fqbn esp32:esp32:esp32s3 .
arduino-cli upload -p /dev/ttyUSB0 --fqbn esp32:esp32:esp32s3 .
arduino-cli monitor -p /dev/ttyUSB0 -c baudrate=115200
```

`deploy/flash_client.sh` chains all three.

### Tri-colour client (`client-tricolor/`)

Timer wake only (`WAKE_HOURS`, default 06:00 and 18:00), no buttons. It downloads and
validates the whole 37,896-byte bitmap *before* touching the panel, so on any
failure the previous image stays up and it retries in an hour
(`ERROR_RETRY_SEC`). The battery comes from the MAX17048 fuel gauge over I2C. At
or below 15% it skips WiFi, draws a low-battery notice locally, and sleeps a
day. Uses GxEPD2 (`GxEPD2_290c`); if the panel shows garbage, the FeatherWing
may be the SSD1680 revision (`GxEPD2_290_C90c`).

```bash
deploy/flash_tricolor.sh      # FQBN esp32:esp32:adafruit_feather_esp32s2, PORT=/dev/ttyACM0
```

## Deployment

The server runs as a **systemd system service** installed under
`/opt/iot-image-server`:

```bash
cd server
cargo build --release && ../deploy/deploy_server.sh
```

The script stops the service, copies the binary and `assets/` into
`/opt/iot-image-server`, installs `deploy/bundle/iot-image-server.service`, then
enables and starts it. Configuration lives in `/opt/iot-image-server/env.txt`
(see `deploy/bundle/env.txt.example`); the unit expands those variables into
command-line flags.

```bash
sudo systemctl status iot-image-server
sudo journalctl -u iot-image-server -f
```

## Conventions

- Keep `cargo clippy --all-targets` clean.
- The SVG generators are pure functions of their input data, which is what makes
  them testable — keep I/O in the `fetch_*` functions.
- Avoid per-request logging in the render path; it lands in the journal on every
  device wake and has no reader.
