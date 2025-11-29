#include "config.h"
#include <WiFi.h>
#include <HTTPClient.h>
#include <GxEPD2_7C.h>      // 7.3" color e-ink
#include <Fonts/FreeMonoBold9pt7b.h>

// Display object for reTerminal e1002 (7.3" color, GDEP073E01)
GxEPD2_7C<GxEPD2_730c_GDEP073E01, GxEPD2_730c_GDEP073E01::HEIGHT> display(
  GxEPD2_730c_GDEP073E01(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN)
);

// Buffer for downloaded bitmap data (allocated dynamically)
uint8_t* bitmap_buffer = nullptr;
int bitmap_buffer_size = 0;

// SPI instance for display
SPIClass hspi(HSPI);

// Forward declarations
void setup_wifi();
void download_image();
void render_image();
void deep_sleep();

void setup() {
  // Use USB Serial for debugging (Serial, not Serial1)
  Serial.begin(115200);
  delay(1000);

  Serial.println("\n\n=== iot-image Client Starting ===");
  Serial.printf("Compiled: %s %s\n", __DATE__, __TIME__);
  
  // Initialize SPI for display
  hspi.begin(EPD_SCK_PIN, -1, EPD_MOSI_PIN, -1);
  
  // Initialize display with GxEPD2
  display.epd2.selectSPI(hspi, SPISettings(4000000, MSBFIRST, SPI_MODE0));
  display.init(115200);
  display.setRotation(0);
  display.setTextColor(GxEPD_BLACK);
  display.fillScreen(GxEPD_WHITE);
  
  Serial.printf("[Display] Initialized (7.3\" color e-ink, %dx%d)\n", EPD_WIDTH, EPD_HEIGHT);
  
  // Connect to WiFi
  setup_wifi();
  
  // Download latest image from server
  download_image();
  
  // Render image on display
  render_image();
  
  // Display refresh takes time on e-ink; wait for it
  Serial.println("[Display] Refresh complete");
  
  // Disconnect WiFi to save power
  WiFi.disconnect(true); // true = turn off radio
  
  // Sleep until next update
  deep_sleep();
}

void loop() {
  // Loop not used; device wakes from deep sleep and runs setup() again
}

/**
 * Connect to WiFi network with retries
 */
void setup_wifi() {
  Serial.println("\n[WiFi] Connecting to WiFi...");
  Serial.printf("[WiFi] SSID: %s\n", WIFI_SSID);
  Serial.printf("[WiFi] Password length: %d\n", strlen(WIFI_PASSWORD));

  // Disconnect any previous connection
  WiFi.disconnect(true);
  delay(1000);

  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  int attempts = 0;
  const int max_attempts = 40; // 20 seconds timeout

  while (WiFi.status() != WL_CONNECTED && attempts < max_attempts) {
    delay(500);
    Serial.print(".");
    attempts++;

    // Print status every 5 attempts
    if (attempts % 5 == 0) {
      Serial.printf("\n[WiFi] Status: %d ", WiFi.status());
    }
  }

  Serial.println();

  if (WiFi.status() == WL_CONNECTED) {
    Serial.println("[WiFi] Connected!");
    Serial.printf("[WiFi] IP address: %s\n", WiFi.localIP().toString().c_str());
    Serial.printf("[WiFi] RSSI: %d dBm\n", WiFi.RSSI());
  } else {
    Serial.printf("[WiFi] Failed to connect! Final status: %d\n", WiFi.status());
    Serial.println("[WiFi] Status codes: 0=IDLE, 1=NO_SSID, 3=CONNECTED, 4=CONNECT_FAILED, 6=DISCONNECTED");
    Serial.println("[WiFi] Proceeding with cached image (if available)");
  }
}

/**
 * Parse EPBM bitmap header and validate
 * Returns true if valid, false otherwise
 */
bool parse_bitmap_header(uint16_t* width, uint16_t* height) {
  if (bitmap_buffer_size < 8) {
    Serial.println("[Bitmap] Buffer too small for header");
    return false;
  }

  // Check magic number "EPBM"
  if (memcmp(bitmap_buffer, "EPBM", 4) != 0) {
    Serial.println("[Bitmap] Invalid magic number");
    return false;
  }

  // Read width (big-endian)
  *width = (bitmap_buffer[4] << 8) | bitmap_buffer[5];

  // Read height (big-endian)
  *height = (bitmap_buffer[6] << 8) | bitmap_buffer[7];

  Serial.printf("[Bitmap] Header: %dx%d\n", *width, *height);

  // Validate dimensions
  if (*width != EPD_WIDTH || *height != EPD_HEIGHT) {
    Serial.printf("[Bitmap] Dimension mismatch: expected %dx%d\n", EPD_WIDTH, EPD_HEIGHT);
    return false;
  }

  return true;
}

/**
 * Map EPBM color value to GxEPD color constant
 */
uint16_t map_epbm_color(uint8_t color) {
  switch (color) {
    case 0:  return GxEPD_BLACK;
    case 1:  return GxEPD_WHITE;
    case 2:  return GxEPD_GREEN;
    case 3:  return GxEPD_BLUE;
    case 4:  return GxEPD_RED;
    case 5:  return GxEPD_YELLOW;
    case 6:  return GxEPD_ORANGE;
    default: return GxEPD_WHITE;  // Default to white for unknown colors
  }
}

/**
 * Download raw bitmap image from server
 * Format: EPBM header (8 bytes) + pixel data (1 byte per pixel)
 */
void download_image() {
  if (WiFi.status() != WL_CONNECTED) {
    Serial.println("[Download] WiFi not connected, skipping download");
    return;
  }

  HTTPClient http;
  String url = String("http://") + SERVER_HOST + ":" + SERVER_PORT + "/" + IMAGE_ENDPOINT;

  Serial.printf("[Download] Fetching: %s\n", url.c_str());

  http.begin(url);
  http.setTimeout(30000); // 30 second timeout

  int http_code = http.GET();
  Serial.printf("[Download] HTTP response code: %d\n", http_code);

  if (http_code != HTTP_CODE_OK) {
    Serial.printf("[Download] Failed: %s\n", http.errorToString(http_code).c_str());
    http.end();
    return;
  }

  int total_len = http.getSize();
  Serial.printf("[Download] Content-Length: %d bytes\n", total_len);

  // Expected size: 8 bytes header + 800*480 bytes data = 384008 bytes
  int expected_size = 8 + (EPD_WIDTH * EPD_HEIGHT);
  if (total_len != expected_size) {
    Serial.printf("[Download] Size mismatch: expected %d bytes\n", expected_size);
    http.end();
    return;
  }

  // Free old buffer if it exists
  if (bitmap_buffer != nullptr) {
    free(bitmap_buffer);
    bitmap_buffer = nullptr;
  }

  // Allocate buffer for entire bitmap
  bitmap_buffer = (uint8_t*)malloc(total_len);
  if (bitmap_buffer == nullptr) {
    Serial.printf("[Download] malloc(%d) failed - out of memory\n", total_len);
    http.end();
    return;
  }

  bitmap_buffer_size = total_len;
  Serial.printf("[Download] Allocated %d bytes\n", total_len);

  // Download entire bitmap into buffer
  WiFiClient* stream = http.getStreamPtr();
  int bytes_read = 0;
  unsigned long start_time = millis();

  Serial.print("[Download] Progress: ");
  while (http.connected() && bytes_read < total_len) {
    size_t available = stream->available();
    if (available) {
      int to_read = min((int)available, total_len - bytes_read);
      int chunk = stream->readBytes(bitmap_buffer + bytes_read, to_read);
      bytes_read += chunk;

      // Progress dots every 10%
      static int last_pct = 0;
      int pct = (bytes_read * 100) / total_len;
      if (pct >= last_pct + 10) {
        Serial.print(".");
        last_pct = pct;
      }
    } else {
      delay(10);
    }

    // Timeout check
    if (millis() - start_time > 30000) {
      Serial.println("\n[Download] Timeout!");
      free(bitmap_buffer);
      bitmap_buffer = nullptr;
      http.end();
      return;
    }
  }

  Serial.printf(" 100%%\n[Download] Downloaded %d bytes in %lu ms\n",
                bytes_read, millis() - start_time);

  http.end();

  // Verify we got all the data
  if (bytes_read != total_len) {
    Serial.printf("[Download] Incomplete: got %d, expected %d\n", bytes_read, total_len);
    free(bitmap_buffer);
    bitmap_buffer = nullptr;
    return;
  }

  Serial.println("[Download] Bitmap download complete");
}

/**
 * Render raw bitmap on e-ink display
 */
void render_image() {
  if (bitmap_buffer == nullptr) {
    Serial.println("[Render] No bitmap data to render");
    return;
  }

  // Parse and validate header
  uint16_t width, height;
  if (!parse_bitmap_header(&width, &height)) {
    Serial.println("[Render] Invalid bitmap header");
    return;
  }

  Serial.println("[Render] Rendering bitmap to display...");
  unsigned long start_time = millis();

  // Prepare display for drawing
  display.setFullWindow();
  display.firstPage();

  // Pixel data starts after 8-byte header
  uint8_t* pixel_data = bitmap_buffer + 8;
  int pixel_count = width * height;

  // Draw all pixels
  for (int i = 0; i < pixel_count; i++) {
    uint8_t color_value = pixel_data[i];
    uint16_t gxepd_color = map_epbm_color(color_value);

    int x = i % width;
    int y = i / width;

    display.drawPixel(x, y, gxepd_color);
  }

  // Trigger display refresh
  display.nextPage();

  unsigned long render_time = millis() - start_time;
  Serial.printf("[Render] Rendered %d pixels in %lu ms\n", pixel_count, render_time);

  // E-ink display refresh takes several seconds
  Serial.println("[Render] Display refresh in progress...");
  delay(5000);
  Serial.println("[Render] Display refresh complete");

  // Free the bitmap buffer
  free(bitmap_buffer);
  bitmap_buffer = nullptr;
  bitmap_buffer_size = 0;
}

/**
 * Deep sleep until next update
 * Wakes automatically and runs setup() again
 */
void deep_sleep() {
  Serial.printf("[Sleep] Sleeping for %d seconds\n", UPDATE_INTERVAL_SEC);
  
  // Calculate wake time
  uint64_t sleep_duration_us = UPDATE_INTERVAL_SEC * 1000000ULL;
  
  // Gracefully print before sleeping
  Serial.flush();
  delay(100);
  
  // Enter deep sleep
  esp_sleep_enable_timer_wakeup(sleep_duration_us);
  esp_deep_sleep_start();
  
  // Fallback: regular delay (for testing/development)
  // Uncomment below if deep sleep causes issues
  // delay(UPDATE_INTERVAL_SEC * 1000);
  // Serial.println("[Sleep] Woke up!");
}
