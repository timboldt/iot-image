// Configuration for the tri-colour weather display:
// Adafruit Feather ESP32-S2 + 2.9" IL0373 tri-colour eInk FeatherWing.

#include "arduino_secrets.h"

#define WIFI_SSID SECRET_SSID
#define WIFI_PASSWORD SECRET_PASS

// Server configuration
#define SERVER_HOST SECRET_HOST
#define SERVER_PORT 8080
#define SERVER_ENDPOINT "weather-mini/tricolor-296.bin"

// POSIX timezone, so daylight saving switches on its own.
// Pacific: "PST8PDT,M3.2.0,M11.1.0"  Eastern: "EST5EDT,M3.2.0,M11.1.0"
#define TZ_INFO "PST8PDT,M3.2.0,M11.1.0"

// Scheduled wake times (24-hour, local time), ascending.
#define WAKE_HOURS {6, 18}

// Sleep after a failed update (WiFi, HTTP or a bad image). The previous
// image stays on screen meanwhile.
#define ERROR_RETRY_SEC 3600

// Sleep if NTP never syncs, so the schedule is unknown.
#define FALLBACK_SLEEP_SEC (3600 * 6)

// At or below this charge, skip WiFi, say so on screen and sleep a day.
#define LOW_BATTERY_PCT 15
#define LOW_BATTERY_SLEEP_SEC (24 * 3600)

// === ePaper (IL0373 / GDEW029Z10, 296x128, white/black/red) ===
#define EPD_WIDTH 296
#define EPD_HEIGHT 128
#define EPD_CS_PIN 9     // D9
#define EPD_DC_PIN 10    // D10
#define EPD_RES_PIN 5    // D5
#define EPD_BUSY_PIN 6   // D6
// 1 or 3 are landscape; use the other one if the image is upside down.
#define EPD_ROTATION 3

// === MAX17048 fuel gauge on the Feather's I2C bus ===
#define MAX17048_ADDR 0x36
#define MAX17048_REG_SOC 0x04

// Expected bitmap: 8-byte EPBM header + one byte per pixel.
#define EPBM_HEADER_SIZE 8
#define EPBM_SIZE (EPBM_HEADER_SIZE + EPD_WIDTH * EPD_HEIGHT)
