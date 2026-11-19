# iot-image tri-colour client

Arduino firmware for an Adafruit Feather ESP32-S2 with the 2.9" IL0373
tri-colour eInk FeatherWing (296x128, white/black/red). It replaces the
CircuitPython code in cp-tri-color. Once a day it fetches
`/weather-mini/tricolor-296.bin` from the iot-image server, shows it, and deep-sleeps.

Preview the screen in a browser at `http://<server>:8080/weather-mini/svg`.

## Setup

```bash
arduino-cli core install esp32:esp32
arduino-cli lib install "GxEPD2"
cp arduino_secrets.h.template arduino_secrets.h   # then fill it in
../deploy/flash_tricolor.sh                        # compile, upload, monitor
```

Settings (wake hours, timezone, low-battery threshold, rotation) are in `config.h`.

## Behaviour

- **Failure** (WiFi, HTTP, wrong size or header): the panel is left alone and it
  retries in an hour.
- **Battery at or below 15%:** skips WiFi, shows "Battery low", sleeps 24 h.
- **Upside down?** Set `EPD_ROTATION` to 1 instead of 3.
- **Garbled or blank?** Newer FeatherWings use an SSD1680. Swap `GxEPD2_290c`
  for `GxEPD2_290_C90c`.
