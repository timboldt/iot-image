// Configuration for iot-image client
// WiFi and server settings for reTerminal e1002

#include "arduino_secrets.h"

// WiFi credentials (from arduino_secrets.h)
#define WIFI_SSID SECRET_SSID
#define WIFI_PASSWORD SECRET_PASS

// Server configuration
#define SERVER_HOST "pidev.local"
#define SERVER_PORT 8080
#define IMAGE_ENDPOINT "test.png"

// Update interval in seconds (6 hours = 21600 seconds)
#define UPDATE_INTERVAL_SEC 21600

// === ePaper Display Pins (reTerminal e1002) ===
// Using GxEPD2 with GxEPD2_730c_GDEP073E01 driver
// 7.3" full-color e-paper: 800x480 pixels, 8 colors (black, white + 6 colors)
#define EPD_WIDTH     800
#define EPD_HEIGHT    480
#define EPD_SCK_PIN   7
#define EPD_MOSI_PIN  9
#define EPD_CS_PIN    10
#define EPD_DC_PIN    11
#define EPD_RES_PIN   12
#define EPD_BUSY_PIN  13

// === SD Card Pins (reTerminal e1002) ===
// Shared SPI bus with display
#define SD_EN_PIN     16    // Power enable
#define SD_DET_PIN    15    // Card detection
#define SD_CS_PIN     14    // Chip select
#define SD_MISO_PIN   8
#define SD_MOSI_PIN   9     // Shared with display
#define SD_SCK_PIN    7     // Shared with display

// === Serial Port Configuration ===
// reTerminal uses Serial1 with specific pins
#define SERIAL_RX 44
#define SERIAL_TX 43
#define SERIAL_BAUD 115200

// === LED and Buzzer (Optional) ===
#define LED_PIN 6           // GPIO6 - Onboard LED (inverted logic: LOW=ON)
#define BUZZER_PIN 45       // GPIO45 - Buzzer

// === Battery Monitoring (Optional) ===
#define BATTERY_ADC_PIN 1   // GPIO1 - Battery voltage
#define BATTERY_ENABLE_PIN 21  // GPIO21 - Battery monitoring enable

// === Button Pins (Optional) ===
#define BUTTON_KEY0 3       // Right button (Green)
#define BUTTON_KEY1 4       // Middle button
#define BUTTON_KEY2 5       // Left button

// PNG streaming buffer size
// Used for downloading chunks and decoding on-the-fly
#define STREAM_BUFFER_SIZE 8192  // 8KB chunks for efficient streaming
