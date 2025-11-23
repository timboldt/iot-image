# iot-image Client

Arduino/ESP32-S3 client for the iot-image system. Runs on Seed Studio reTerminal e1002 and displays weather images on the 7.3" e-ink display.

## Hardware

- **Device:** Seed Studio reTerminal e1002
- **Processor:** ESP32-S3 (240 MHz dual-core Xtensa)
- **Display:** 7.3" e-ink (296 × 128 pixels, 3-color)
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

### 4. Build & Upload

```bash
cd client
arduino-cli compile --fqbn esp32:esp32:esp32s3 .
arduino-cli upload -p /dev/ttyUSB0 --fqbn esp32:esp32:esp32s3 .
```

### 5. Monitor Serial Output

```bash
arduino-cli monitor -p /dev/ttyUSB0 -c baudrate=115200
```

## Device-Specific Image Format

The server generates images in a binary device-specific format optimized for the reTerminal e1002:

### Binary Format
```
Byte   0-3:  Magic number (0xDEADBEEF)
Byte   4-7:  Data size (uint32_t, little-endian)
Byte   8+:   Image data (variable length)
```

### Image Data
- **Dimensions:** 296 × 128 pixels
- **Format:** Raw pixel data optimized for e-ink
- **Size:** Typically 38-50 KB
- **Encoding:** Device-specific (server-dependent)

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
- `download_image()` - Fetch image from server
- `render_image()` - Display image on e-ink
- `deep_sleep()` - Sleep until next update

## Update Cycle

1. **Connect to WiFi** (5-10 seconds)
2. **Download image** from server (5-15 seconds depending on size)
3. **Render on display** (20+ seconds)
4. **Deep sleep** until next update (default: 15 minutes)

Total cycle time: ~1-2 minutes, then sleep

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
[Download] Content-Length: 38400 bytes
[Download] Complete: 38400 bytes received
[Render] Rendering 38400 bytes to display
[Sleep] Sleeping for 900 seconds
```

## TODO

- [ ] Integrate Seed Studio e-ink display driver
- [ ] Implement actual deep sleep (`esp_deep_sleep_start()`)
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
- Validate image size is within `IMAGE_BUFFER_SIZE`
- Check display driver is properly initialized

**Serial output shows gibberish?**
- Ensure baud rate is 115200
- Check USB cable connection
- Verify correct COM port with `arduino-cli board list`
