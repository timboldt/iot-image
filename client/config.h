// Configuration for iot-image client
// WiFi and server settings for reTerminal e1002

// WiFi credentials
#define WIFI_SSID "your_ssid"
#define WIFI_PASSWORD "your_password"

// Server configuration
#define SERVER_HOST "raspberrypi.local"
#define SERVER_PORT 8080
#define IMAGE_ENDPOINT "/image/latest"

// Update interval in seconds (15 minutes = 900 seconds)
#define UPDATE_INTERVAL_SEC 900

// === ePaper Display Pins (reTerminal e1002) ===
// Using GxEPD2 with GxEPD2_730c_GDEP073E01 driver (7.3" color)
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

// Image buffer size
// Device-specific format for 296x128 display: ~38KB
#define IMAGE_BUFFER_SIZE 40960
