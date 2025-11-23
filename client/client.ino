#include "config.h"
#include <WiFi.h>
#include <HTTPClient.h>
#include <GxEPD2_7C.h>      // 7.3" color e-ink
#include <Fonts/FreeMonoBold9pt7b.h>

// Display object for reTerminal e1002 (7.3" color, GDEP073E01)
GxEPD2_7C<GxEPD2_730c_GDEP073E01, GxEPD2_730c_GDEP073E01::HEIGHT> display(
  GxEPD2_730c_GDEP073E01(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN)
);

// Image buffer for storing downloaded image
uint8_t image_buffer[IMAGE_BUFFER_SIZE];
size_t image_size = 0;

// SPI instance for display
SPIClass hspi(HSPI);

// Forward declarations
void setup_wifi();
void download_image();
void render_image();
void deep_sleep();

void setup() {
  Serial1.begin(SERIAL_BAUD, SERIAL_8N1, SERIAL_RX, SERIAL_TX);
  delay(1000);
  
  Serial1.println("\n\n=== iot-image Client Starting ===");
  Serial1.printf("Compiled: %s %s\n", __DATE__, __TIME__);
  
  // Initialize SPI for display
  hspi.begin(EPD_SCK_PIN, -1, EPD_MOSI_PIN, -1);
  
  // Initialize display with GxEPD2
  display.epd2.selectSPI(hspi, SPISettings(4000000, MSBFIRST, SPI_MODE0));
  display.init(115200);
  display.setRotation(0);
  display.setTextColor(GxEPD_BLACK);
  display.fillScreen(GxEPD_WHITE);
  
  Serial1.println("[Display] Initialized (7.3\" color e-ink, 296x128)");
  
  // Connect to WiFi
  setup_wifi();
  
  // Download latest image from server
  download_image();
  
  // Render image on display
  render_image();
  
  // Display refresh takes time on e-ink; wait for it
  Serial1.println("[Display] Refresh complete");
  
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
  Serial1.println("\n[WiFi] Connecting to WiFi...");
  Serial1.printf("[WiFi] SSID: %s\n", WIFI_SSID);
  
  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  
  int attempts = 0;
  const int max_attempts = 20; // 20 seconds timeout
  
  while (WiFi.status() != WL_CONNECTED && attempts < max_attempts) {
    delay(500);
    Serial1.print(".");
    attempts++;
  }
  
  Serial1.println();
  
  if (WiFi.status() == WL_CONNECTED) {
    Serial1.println("[WiFi] Connected!");
    Serial1.printf("[WiFi] IP address: %s\n", WiFi.localIP().toString().c_str());
    Serial1.printf("[WiFi] RSSI: %d dBm\n", WiFi.RSSI());
  } else {
    Serial1.println("[WiFi] Failed to connect!");
    Serial1.println("[WiFi] Proceeding with cached image (if available)");
  }
}

/**
 * Download latest image from server
 * Server format: Binary device-specific image format
 * - Header: 4 bytes (magic number 0xDEADBEEF)
 * - Size: 4 bytes (image data size)
 * - Data: variable length image data
 */
void download_image() {
  if (WiFi.status() != WL_CONNECTED) {
    Serial1.println("[Download] WiFi not connected, skipping download");
    return;
  }
  
  HTTPClient http;
  String url = String("http://") + SERVER_HOST + ":" + SERVER_PORT + IMAGE_ENDPOINT;
  
  Serial1.printf("[Download] Fetching: %s\n", url.c_str());
  
  http.begin(url);
  http.setTimeout(10000); // 10 second timeout
  
  int http_code = http.GET();
  Serial1.printf("[Download] HTTP response code: %d\n", http_code);
  
  if (http_code == HTTP_CODE_OK) {
    int total_len = http.getSize();
    Serial1.printf("[Download] Content-Length: %d bytes\n", total_len);
    
    if (total_len > IMAGE_BUFFER_SIZE) {
      Serial1.printf("[Download] ERROR: Image too large (%d > %d)\n", total_len, IMAGE_BUFFER_SIZE);
      http.end();
      return;
    }
    
    // Read the entire response into buffer
    WiFiClient* stream = http.getStreamPtr();
    int bytes_read = 0;
    
    while (http.connected() && bytes_read < total_len) {
      size_t available = stream->available();
      if (available > 0) {
        int to_read = min((size_t)available, (size_t)(total_len - bytes_read));
        int read = stream->readBytes(&image_buffer[bytes_read], to_read);
        bytes_read += read;
        
        if (bytes_read % 4096 == 0) {
          Serial1.printf("[Download] Progress: %d / %d bytes\n", bytes_read, total_len);
        }
      }
    }
    
    image_size = bytes_read;
    Serial1.printf("[Download] Complete: %d bytes received\n", image_size);
    
    // Validate image format
    if (image_size >= 8) {
      uint32_t magic = *(uint32_t*)&image_buffer[0];
      uint32_t data_size = *(uint32_t*)&image_buffer[4];
      Serial1.printf("[Download] Magic: 0x%08X, Data size: %d\n", magic, data_size);
      
      if (magic != 0xDEADBEEF) {
        Serial1.println("[Download] WARNING: Invalid magic number in image header");
      }
    }
  } else {
    Serial1.printf("[Download] Failed to download: %s\n", http.errorToString(http_code).c_str());
  }
  
  http.end();
}

/**
 * Render image on e-ink display
 * 
 * The device-specific image format in the buffer is sent directly to the display driver.
 * This assumes the server generates images optimized for the reTerminal e1002 (GxEPD2_730c format).
 */
void render_image() {
  if (image_size == 0) {
    Serial1.println("[Render] No image data to render");
    return;
  }
  
  Serial1.printf("[Render] Rendering %d bytes to display\n", image_size);
  
  // TODO: Parse device-specific format and call display.drawPixel() for each pixel
  // For now, demonstrate display capability with a simple pattern
  
  display.setFullWindow();
  display.firstPage();
  do {
    display.fillScreen(GxEPD_WHITE);
    display.setCursor(10, 30);
    display.setFont(&FreeMonoBold9pt7b);
    display.println("Weather Image");
    display.println("Downloaded");
  } while (display.nextPage());
  
  Serial1.println("[Render] Display refresh initiated");
  // E-ink display refresh takes 1-3 seconds
  delay(3000);
  Serial1.println("[Render] Display refresh complete");
}

/**
 * Deep sleep until next update
 * Wakes automatically and runs setup() again
 */
void deep_sleep() {
  Serial1.printf("[Sleep] Sleeping for %d seconds\n", UPDATE_INTERVAL_SEC);
  
  // Calculate wake time
  uint64_t sleep_duration_us = UPDATE_INTERVAL_SEC * 1000000ULL;
  
  // Gracefully print before sleeping
  Serial1.flush();
  delay(100);
  
  // Enter deep sleep
  esp_sleep_enable_timer_wakeup(sleep_duration_us);
  esp_deep_sleep_start();
  
  // Fallback: regular delay (for testing/development)
  // Uncomment below if deep sleep causes issues
  // delay(UPDATE_INTERVAL_SEC * 1000);
  // Serial1.println("[Sleep] Woke up!");
}
