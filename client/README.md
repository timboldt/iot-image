# iot-image Client

Arduino/ESP32-S3 client for the iot-image system. Runs on Seed Studio reTerminal e1002 and displays weather images on the 7.3" e-ink display.

## Hardware

- **Device:** Seed Studio reTerminal e1002
- **Processor:** ESP32-S3 (240 MHz dual-core Xtensa)
- **Display:** 7.3" full-color e-paper (800 × 480 pixels, 8 colors: black, white + 6 colors)
- **Connectivity:** WiFi 802.11 b/g/n (2.4 GHz)

## Setup

### 1. Configure WiFi & Server

Edit `config.h`:
```c
#define WIFI_SSID "your_ssid"
#define WIFI_PASSWORD "your_password"
#define SERVER_HOST "raspberrypi.local"  // or IP address
#define SERVER_PORT 8080
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

# PNGdec - Streaming PNG decoder (lightweight, no external dependencies)
arduino-cli lib install "PNGdec"
```

**Library Details:**
- **GxEPD2**: Supports various e-ink displays including GDEP073E01
- **PNGdec** by Larry Bank: Memory-efficient PNG decoder that streams data without requiring the entire image in RAM. Perfect for embedded systems with limited memory.

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

The server generates PNG images that are streamed and decoded on-the-fly by the client:

### PNG Streaming
- **Format:** Standard PNG (8-bit indexed or 24-bit RGB)
- **Dimensions:** 800 × 480 pixels
- **Colors:** 8 colors (black, white, red, green, blue, yellow, and variations)
- **Typical Size:** 50-150 KB (depending on image complexity)
- **Decoding:** Streaming decoder processes the image line-by-line without loading the entire image into RAM

### Why PNG?
- **Memory efficient:** Decodes on-the-fly with only ~8KB RAM buffer
- **Standard format:** Easy to generate server-side with any image library
- **Lossless:** Perfect for weather icons, text, and graphics
- **No large buffers needed:** 800×480 raw image would be ~384KB, but streaming PNG only needs ~8KB working memory

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
- `download_image()` - Stream and decode PNG from server
- `png_draw()` - Callback to render each decoded line to display
- `png_read()`, `png_open()`, `png_close()`, `png_seek()` - PNG decoder callbacks
- `render_image()` - Trigger final display refresh
- `deep_sleep()` - Sleep until next update

## Update Cycle

1. **Connect to WiFi** (5-10 seconds)
2. **Stream and decode PNG** from server (10-30 seconds depending on image size and WiFi speed)
   - Downloads in chunks, decodes line-by-line
   - Draws directly to display buffer during decoding
3. **Display refresh** (5-10 seconds for full e-ink refresh)
4. **Deep sleep** until next update (default: 15 minutes)

Total active time: ~30-60 seconds, then sleep

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
[Download] Fetching: http://raspberrypi.local:8080/image/latest
[Download] HTTP response code: 200
[Download] Content-Length: 85420 bytes
[PNG] Image specs: 800x480, 24 bpp
[PNG] Decode successful (2340 ms)
[Render] Refreshing display...
[Render] Display refresh complete
[Sleep] Sleeping for 900 seconds
```

## TODO

- [x] Integrate PNG streaming decoder
- [x] Update display dimensions to 800x480
- [ ] Fine-tune color mapping from RGB to e-ink palette
- [ ] Add fallback image cache (SPIFFS)
- [ ] Implement WiFi reconnection on failure
- [ ] Add battery monitoring (if supported by reTerminal)
- [ ] OTA firmware updates

## Troubleshooting

**Can't connect to WiFi?**
- Check SSID/password in `config.h`
- Verify ESP32-S3 antenna placement
- Check WiFi signal strength with serial monitor

**Image not rendering?**
- Verify server is running and accessible at SERVER_HOST:SERVER_PORT
- Check HTTP endpoint matches `/image/latest`
- Ensure server is returning PNG format (not another format)
- Verify PNG dimensions are 800×480
- Check display driver is properly initialized
- Monitor serial output for PNG decode errors

**Serial output shows gibberish?**
- Ensure baud rate is 115200
- Check USB cable connection
- Verify correct COM port with `arduino-cli board list`
