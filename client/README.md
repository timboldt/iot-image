# iot-image Client

Arduino/ESP32-S3 client for the iot-image system. Runs on Seed Studio reTerminal e1002 and displays weather images on the 7.3" e-ink display.

## Hardware

- **Device:** Seed Studio reTerminal e1002
- **Processor:** ESP32-S3 (240 MHz dual-core Xtensa)
- **Display:** 7.3" full-color e-paper (800 × 480 pixels, 8 colors: black, white + 6 colors)
- **Connectivity:** WiFi 802.11 b/g/n (2.4 GHz)

## Setup

### 1. Configure WiFi & Server

Edit `arduino-secrets.h`:
```c
#define SECRET_SSID "your_ssid"
#define SECRET_PASS "your_password"
#define SECRET_HOST "your_server_host"
```

### 2. Install Arduino CLI

**macOS:**
```bash
brew install arduino-cli
```

**Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/arduino/arduino-cli/master/install.sh | sh
```

### 3. Setup ESP32 Board Support

```bash
arduino-cli core install esp32:esp32
```

### 4. Install Required Libraries

```bash
# GxEPD2 - E-ink display driver
arduino-cli lib install "GxEPD2"
```

**Library Details:**
- **GxEPD2**: Supports various e-ink displays including GDEP073E01 (7.3" 7-color e-paper)

### 5. Build & Upload

```bash
cd client
arduino-cli compile --fqbn esp32:esp32:esp32s3 .
arduino-cli upload -p /dev/ttyUSB0 --fqbn esp32:esp32:esp32s3 .
```

### 6. Monitor Serial Output

```bash
arduino-cli monitor -p /dev/ttyUSB0 -c baudrate=115200
```

## Image Format

The server generates raw bitmaps in the native EPBM format for maximum efficiency:

### EPBM (E-Paper BitMap) Format
- **Header:** 8 bytes
  - Magic number: "EPBM" (4 bytes)
  - Width: 16-bit big-endian (2 bytes)
  - Height: 16-bit big-endian (2 bytes)
- **Pixel Data:** 1 byte per pixel, representing the color value directly
- **Dimensions:** 800 × 480 pixels
- **File Size:** 384,008 bytes (8 header + 384,000 pixel data)
- **Colors:** 6 colors mapped to GxEPD2 palette
  - 0 = Black
  - 1 = White
  - 2 = Green
  - 3 = Blue
  - 4 = Red
  - 5 = Yellow
  
### Why Raw Bitmap?
- **Zero decoding overhead:** No PNG/JPEG decompression required
- **Direct rendering:** Color values map 1:1 to display commands
- **Predictable performance:** Fixed transfer and render time
- **Simple implementation:** No external decoder libraries needed
- **Optimal for e-ink:** Format designed specifically for the display's color palette

## Code Structure

### `config.h`
Configuration constants:
- WiFi credentials
- Server hostname/port
- Display pin assignments
- Buffer sizes
- Update intervals

### `client.ino`
Main sketch with functions:
- `setup()` - Initialize and run main loop
- `setup_wifi()` - Connect to WiFi with retries
- `download_image()` - Download raw bitmap from server
- `parse_bitmap_header()` - Validate EPBM header and extract dimensions
- `map_epbm_color()` - Convert EPBM color values to GxEPD2 constants
- `render_image()` - Render bitmap to display and trigger refresh
- `deep_sleep()` - Sleep until next update

## Update Cycle

1. **Connect to WiFi** (5-10 seconds)
2. **Download raw bitmap** from server (~5-10 seconds for 384KB over WiFi)
   - Downloads entire bitmap into RAM
   - Validates EPBM header
3. **Render to display** (~20-30 seconds to draw 384,000 pixels)
   - Reads pixel data and maps colors directly
   - No decoding required
4. **Display refresh** (5-10 seconds for full e-ink refresh)
5. **Deep sleep** until next update (default: 6 hours)

Total active time: ~40-60 seconds, then sleep

## Power Optimization

- WiFi disabled during sleep
- Deep sleep consumes ~0.1 mA
- Update interval configurable via `UPDATE_INTERVAL_SEC`
- E-ink display uses power only during refresh

## Debugging

Serial output provides detailed status:
```
[WiFi] Connecting to WiFi...
[WiFi] Connected!
[WiFi] IP address: 192.168.1.100
[Download] Fetching: http://<server>:8080/weather/seed-e1002.bin
[Download] HTTP response code: 200
[Download] Content-Length: 384008 bytes
[Download] Bitmap download complete
[Bitmap] Header: 800x480
[Render] Rendering bitmap to display...
[Render] Rendered 384000 pixels in 25340 ms
[Render] Display refresh complete
[Sleep] Sleeping for 21600 seconds
```

## TODO

- [x] Replace PNG format with native EPBM bitmap format
- [x] Update display dimensions to 800x480
- [x] Direct color mapping using GxEPD2 palette
- [ ] Add fallback image cache (SPIFFS)
- [ ] Implement WiFi reconnection on failure
- [ ] Add battery monitoring (if supported by reTerminal)
- [ ] OTA firmware updates
- [ ] Optimize pixel rendering (use writeImage or buffer)

## Troubleshooting

**Can't connect to WiFi?**
- Check SSID/password in `config.h`
- Verify ESP32-S3 antenna placement
- Check WiFi signal strength with serial monitor

**Image not rendering?**
- Verify server is running with `--serve` flag
- Check server is accessible at SERVER_HOST:SERVER_PORT
- Ensure server is returning EPBM format (384,008 bytes)
- Verify bitmap dimensions are 800×480
- Check display driver is properly initialized
- Monitor serial output for bitmap header validation errors

**Serial output shows gibberish?**
- Ensure baud rate is 115200
- Check USB cable connection
- Verify correct COM port with `arduino-cli board list`
