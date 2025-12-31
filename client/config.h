// Configuration for iot-image client
// WiFi and server settings for reTerminal e1002

#include "arduino_secrets.h"

// WiFi credentials (from arduino_secrets.h)
#define WIFI_SSID SECRET_SSID
#define WIFI_PASSWORD SECRET_PASS

// Server configuration
#define SERVER_HOST SECRET_HOST
#define SERVER_PORT 8080
// IMAGE_ENDPOINT is now dynamic based on button press:
//   - Button 1 (KEY0): weather/seed-e1002.bin
//   - Button 2 (KEY1): stocks/seed-e1002.bin

// Timezone configuration (Pacific Time example)
// Common timezones:
//   Pacific: -8, Eastern: -5, Central: -6, Mountain: -7, UTC: 0
#define TIMEZONE_OFFSET_SEC (-8 * 3600)  // Offset from UTC in seconds
#define DST_OFFSET_SEC 3600               // Daylight saving adjustment (1 hour)

// Scheduled wake times (24-hour format, local time)
#define WAKE_HOUR_1 6   // 6:00 AM
#define WAKE_HOUR_2 12  // 12:00 PM (noon)
#define WAKE_HOUR_3 18  // 6:00 PM

// Fallback sleep duration if NTP fails
#define FALLBACK_SLEEP_SEC (3600 * 6)

// === ePaper Display Pins (reTerminal e1002) ===
// Using GxEPD2 with GxEPD2_730c_GDEP073E01 driver
// 7.3" full-color e-paper: 800x480 pixels, 8 colors (black, white + 6 colors)
#define EPD_WIDTH 800
#define EPD_HEIGHT 480
#define EPD_SCK_PIN 7
#define EPD_MOSI_PIN 9
#define EPD_CS_PIN 10
#define EPD_DC_PIN 11
#define EPD_RES_PIN 12
#define EPD_BUSY_PIN 13

// === SD Card Pins (reTerminal e1002) ===
// Shared SPI bus with display
#define SD_EN_PIN 16   // Power enable
#define SD_DET_PIN 15  // Card detection
#define SD_CS_PIN 14   // Chip select
#define SD_MISO_PIN 8
#define SD_MOSI_PIN 9  // Shared with display
#define SD_SCK_PIN 7   // Shared with display

// === Serial Port Configuration ===
// reTerminal uses Serial1 with specific pins
#define SERIAL_RX 44
#define SERIAL_TX 43
#define SERIAL_BAUD 115200

// === LED and Buzzer (Optional) ===
#define LED_PIN 6      // GPIO6 - Onboard LED (inverted logic: LOW=ON)
#define BUZZER_PIN 45  // GPIO45 - Buzzer

// === Battery Monitoring (Optional) ===
#define BATTERY_ADC_PIN 1      // GPIO1 - Battery voltage
#define BATTERY_ENABLE_PIN 21  // GPIO21 - Battery monitoring enable

// === Button Pins ===
#define BUTTON_KEY0 3  // Right button (Green) - Wake & refresh current mode
#define BUTTON_KEY1 4  // Middle button - Switch to Stocks
#define BUTTON_KEY2 5  // Left button - Switch to Weather

// PNG streaming buffer size
// Used for downloading chunks and decoding on-the-fly
#define STREAM_BUFFER_SIZE 8192  // 8KB chunks for efficient streaming
