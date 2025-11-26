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

// Buffer for downloaded PNG data
uint8_t* png_buffer = nullptr;
int png_buffer_size = 0;
int png_buffer_pos = 0;

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
 * PNG memory reader callbacks
 * Called by PNGdec library to read chunks from memory buffer
 */
void* png_open(const char* filename, int32_t* size) {
  *size = png_buffer_size;
  png_buffer_pos = 0;  // Reset position
  return png_buffer;
}

void png_close(void* handle) {
  // Nothing to do; we'll free the buffer elsewhere
}

int32_t png_read(PNGFILE* handle, uint8_t* buffer, int32_t length) {
  int32_t bytes_to_read = length;

  if (png_buffer_pos + length > png_buffer_size) {
    bytes_to_read = png_buffer_size - png_buffer_pos;
  }

  if (bytes_to_read <= 0) {
    return 0;
  }

  memcpy(buffer, png_buffer + png_buffer_pos, bytes_to_read);
  png_buffer_pos += bytes_to_read;

  return bytes_to_read;
}

int32_t png_seek(PNGFILE* handle, int32_t position) {
  if (position >= 0 && position < png_buffer_size) {
    png_buffer_pos = position;
    return position;
  }
  return 0;
}

/**
 * PNG draw callback
 * Called by PNGdec for each decoded line of pixels
 * Returns 1 to continue decoding, 0 to stop
 */
int png_draw(PNGDRAW* pDraw) {
  // pDraw->pPixels contains RGB565 pixel data (array of uint16_t)
  // pDraw->iWidth is the width of this line segment
  // pDraw->y is the current line number

  uint16_t* pixels = (uint16_t*)pDraw->pPixels;

  // Draw the line of pixels to the display
  for (int x = 0; x < pDraw->iWidth; x++) {
    uint16_t color = pixels[x];

    // Extract RGB components from RGB565
    // RGB565 format: RRRRR GGGGGG BBBBB
    uint8_t r = (color >> 11) & 0x1F;  // 5 bits red
    uint8_t g = (color >> 5) & 0x3F;   // 6 bits green
    uint8_t b = color & 0x1F;          // 5 bits blue

    // Convert to 8-bit values for easier comparison
    r = (r << 3) | (r >> 2);  // Scale 5-bit to 8-bit
    g = (g << 2) | (g >> 4);  // Scale 6-bit to 8-bit
    b = (b << 3) | (b >> 2);  // Scale 5-bit to 8-bit

    // Map RGB to e-ink display colors
    uint16_t color_mapped;

    if (r > 240 && g > 240 && b > 240) {
      // Nearly white
      color_mapped = GxEPD_WHITE;
    } else if (r < 15 && g < 15 && b < 15) {
      // Nearly black
      color_mapped = GxEPD_BLACK;
    } else if (r > 200 && g < 100 && b < 100) {
      // Red dominant
      color_mapped = GxEPD_RED;
    } else if (r < 100 && g > 200 && b < 100) {
      // Green dominant
      color_mapped = GxEPD_GREEN;
    } else if (r < 100 && g < 100 && b > 200) {
      // Blue dominant
      color_mapped = GxEPD_BLUE;
    } else if (r > 200 && g > 200 && b < 100) {
      // Yellow (red + green)
      color_mapped = GxEPD_YELLOW;
    } else if (r > 200 && g > 100 && b < 100) {
      // Orange (red + some green)
      color_mapped = GxEPD_ORANGE;
    } else {
      // Default based on brightness
      int brightness = (r + g + b) / 3;
      color_mapped = (brightness > 128) ? GxEPD_WHITE : GxEPD_BLACK;
    }

    display.drawPixel(x, pDraw->y, color_mapped);
  }

  return 1; // Continue decoding
}

/**
 * Download PNG image from server into memory, then decode
 */
void download_image() {
  if (WiFi.status() != WL_CONNECTED) {
    Serial.println("[Download] WiFi not connected, skipping download");
    return;
  }

  HTTPClient http;
  // String url = String("http://") + SERVER_HOST + ":" + SERVER_PORT + IMAGE_ENDPOINT;
  String url = String("http://httpbin.org/image/png");

  Serial.printf("[Download] Fetching: %s\n", url.c_str());

  http.begin(url);
  http.setTimeout(30000); // 30 second timeout for larger images

  int http_code = http.GET();
  Serial.printf("[Download] HTTP response code: %d\n", http_code);

  if (http_code == HTTP_CODE_OK) {
    int total_len = http.getSize();
    Serial.printf("[Download] Content-Length: %d bytes\n", total_len);

    // Allocate buffer for entire PNG
    if (png_buffer != nullptr) {
      free(png_buffer);
      png_buffer = nullptr;
    }

    png_buffer = (uint8_t*)malloc(total_len);
    if (png_buffer == nullptr) {
      Serial.println("[Download] Failed to allocate memory for PNG");
      http.end();
      return;
    }

    png_buffer_size = total_len;

    // Download entire PNG into buffer
    WiFiClient* stream = http.getStreamPtr();
    int bytes_read = 0;
    unsigned long start_time = millis();

    Serial.print("[Download] Downloading: ");
    while (http.connected() && bytes_read < total_len) {
      size_t available = stream->available();
      if (available) {
        int chunk = stream->readBytes(png_buffer + bytes_read,
                                      min((int)available, total_len - bytes_read));
        bytes_read += chunk;

        // Progress indicator every 10%
        if (bytes_read % (total_len / 10) == 0) {
          Serial.print(".");
        }
      } else {
        delay(10);
      }

      // Timeout check (30 seconds)
      if (millis() - start_time > 30000) {
        Serial.println("\n[Download] Timeout!");
        free(png_buffer);
        png_buffer = nullptr;
        http.end();
        return;
      }
    }

    Serial.printf("\n[Download] Downloaded %d bytes in %lu ms\n",
                  bytes_read, millis() - start_time);

    // Now decode the PNG from memory
    int result = png.open(nullptr, png_open, png_close, png_read, png_seek, png_draw);

    if (result == PNG_SUCCESS) {
      Serial.printf("[PNG] Image specs: %dx%d, %d bpp\n",
                     png.getWidth(), png.getHeight(), png.getBpp());

      // Prepare display for drawing
      display.setFullWindow();
      display.firstPage();

      // Decode PNG (calls png_draw callback for each line)
      start_time = millis();
      result = png.decode(nullptr, 0);
      unsigned long decode_time = millis() - start_time;

      if (result == PNG_SUCCESS) {
        Serial.printf("[PNG] Decode successful (%lu ms)\n", decode_time);
      } else {
        Serial.printf("[PNG] Decode failed with error: %d\n", result);
      }

      png.close();
    } else {
      Serial.printf("[PNG] Failed to open PNG: %d\n", result);
    }

    // Free the buffer
    free(png_buffer);
    png_buffer = nullptr;
  } else {
    Serial.printf("[Download] Failed to download: %s\n", http.errorToString(http_code).c_str());
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
  Serial.println("[Render] Refreshing display...");

  // Complete the display update
  display.nextPage();

  Serial.println("[Render] Display refresh initiated");
  // E-ink display refresh takes several seconds
  delay(5000);
  Serial.println("[Render] Display refresh complete");
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
