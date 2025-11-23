#include "config.h"
#include <WiFi.h>
#include <HTTPClient.h>
#include <GxEPD2_7C.h>      // 7.3" color e-ink
#include <PNGdec.h>         // PNG streaming decoder
#include <Fonts/FreeMonoBold9pt7b.h>

// Display object for reTerminal e1002 (7.3" color, GDEP073E01)
GxEPD2_7C<GxEPD2_730c_GDEP073E01, GxEPD2_730c_GDEP073E01::HEIGHT> display(
  GxEPD2_730c_GDEP073E01(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN)
);

// PNG decoder instance
PNG png;

// Stream buffer for PNG decoding
uint8_t stream_buffer[STREAM_BUFFER_SIZE];

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
  
  Serial1.printf("[Display] Initialized (7.3\" color e-ink, %dx%d)\n", EPD_WIDTH, EPD_HEIGHT);
  
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
 * PNG stream reader callback
 * Called by PNGdec library to read chunks of data from HTTP stream
 */
WiFiClient* png_stream_ptr = nullptr;

void* png_open(const char* filename, int32_t* size) {
  // Return the stream pointer; size is unknown for streaming
  *size = 0;
  return png_stream_ptr;
}

void png_close(void* handle) {
  // Nothing to do; HTTP client handles cleanup
}

int32_t png_read(PNGFILE* handle, uint8_t* buffer, int32_t length) {
  if (!png_stream_ptr || !png_stream_ptr->connected()) {
    return 0;
  }

  int32_t bytes_read = 0;
  unsigned long timeout = millis() + 5000; // 5 second timeout

  while (bytes_read < length && millis() < timeout) {
    if (png_stream_ptr->available()) {
      int chunk = png_stream_ptr->readBytes(buffer + bytes_read, length - bytes_read);
      if (chunk > 0) {
        bytes_read += chunk;
      } else {
        break;
      }
    } else {
      delay(10);
    }
  }

  return bytes_read;
}

int32_t png_seek(PNGFILE* handle, int32_t position) {
  // Seeking not supported in streaming mode
  return 0;
}

/**
 * PNG draw callback
 * Called by PNGdec for each decoded line of pixels
 */
void png_draw(PNGDRAW* pDraw) {
  uint16_t* pixels = (uint16_t*)pDraw->pPixels;

  // Draw the line of pixels to the display
  for (int x = 0; x < pDraw->iWidth; x++) {
    uint16_t color = pixels[x];

    // Convert RGB565 to GxEPD2 colors
    // This is a simple mapping; adjust based on your color palette
    uint16_t color_mapped;
    if (color == 0xFFFF) {
      color_mapped = GxEPD_WHITE;
    } else if (color == 0x0000) {
      color_mapped = GxEPD_BLACK;
    } else if ((color & 0xF800) > 0xC000) {
      color_mapped = GxEPD_RED;  // Reddish colors
    } else if ((color & 0x07E0) > 0x0600) {
      color_mapped = GxEPD_GREEN; // Greenish colors
    } else if ((color & 0x001F) > 0x0018) {
      color_mapped = GxEPD_BLUE;  // Bluish colors
    } else if ((color & 0xFFE0) > 0xC000) {
      color_mapped = GxEPD_YELLOW; // Yellowish colors
    } else {
      color_mapped = GxEPD_BLACK; // Default to black
    }

    display.drawPixel(x, pDraw->y, color_mapped);
  }
}

/**
 * Download and render PNG image from server using streaming decoder
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
  http.setTimeout(30000); // 30 second timeout for larger images

  int http_code = http.GET();
  Serial1.printf("[Download] HTTP response code: %d\n", http_code);

  if (http_code == HTTP_CODE_OK) {
    int total_len = http.getSize();
    Serial1.printf("[Download] Content-Length: %d bytes\n", total_len);

    // Get stream pointer for PNG decoder
    png_stream_ptr = http.getStreamPtr();

    // Open PNG and start decoding
    int result = png.open(nullptr, png_open, png_close, png_read, png_seek, png_draw);

    if (result == PNG_SUCCESS) {
      Serial1.printf("[PNG] Image specs: %dx%d, %d bpp\n",
                     png.getWidth(), png.getHeight(), png.getBpp());

      // Prepare display for drawing
      display.setFullWindow();
      display.firstPage();

      // Decode PNG (calls png_draw callback for each line)
      unsigned long start_time = millis();
      result = png.decode(nullptr, 0);
      unsigned long decode_time = millis() - start_time;

      if (result == PNG_SUCCESS) {
        Serial1.printf("[PNG] Decode successful (%lu ms)\n", decode_time);
      } else {
        Serial1.printf("[PNG] Decode failed with error: %d\n", result);
      }

      png.close();
    } else {
      Serial1.printf("[PNG] Failed to open PNG: %d\n", result);
    }

    png_stream_ptr = nullptr;
  } else {
    Serial1.printf("[Download] Failed to download: %s\n", http.errorToString(http_code).c_str());
  }

  http.end();
}

/**
 * Render image on e-ink display
 *
 * Image is already rendered during PNG decoding via png_draw callback.
 * This function just triggers the display refresh.
 */
void render_image() {
  Serial1.println("[Render] Refreshing display...");

  // Complete the display update
  display.nextPage();

  Serial1.println("[Render] Display refresh initiated");
  // E-ink display refresh takes several seconds
  delay(5000);
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
