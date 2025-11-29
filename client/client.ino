#include <Fonts/FreeMonoBold9pt7b.h>
#include <GxEPD2_7C.h>  // 7.3" color e-ink
#include <HTTPClient.h>
#include <WiFi.h>

#include "config.h"

// Display object for reTerminal e1002 (7.3" color, GDEP073E01)
GxEPD2_7C<GxEPD2_730c_GDEP073E01, GxEPD2_730c_GDEP073E01::HEIGHT> display(
    GxEPD2_730c_GDEP073E01(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN));

// SPI instance for display
SPIClass hspi(HSPI);

// Forward declarations
void setup_wifi();
void download_and_render_image();
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

    Serial.printf("[Display] Initialized (7.3\" color e-ink, %dx%d)\n",
                  EPD_WIDTH, EPD_HEIGHT);

    // Connect to WiFi
    setup_wifi();

    // Download and render image in streaming mode
    download_and_render_image();

    // Display refresh takes time on e-ink; wait for it
    Serial.println("[Display] Refresh complete");

    // Disconnect WiFi to save power
    WiFi.disconnect(true);  // true = turn off radio

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
    const int max_attempts = 40;  // 20 seconds timeout

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
        Serial.printf("[WiFi] IP address: %s\n",
                      WiFi.localIP().toString().c_str());
        Serial.printf("[WiFi] RSSI: %d dBm\n", WiFi.RSSI());
    } else {
        Serial.printf("[WiFi] Failed to connect! Final status: %d\n",
                      WiFi.status());
        Serial.println(
            "[WiFi] Status codes: 0=IDLE, 1=NO_SSID, 3=CONNECTED, "
            "4=CONNECT_FAILED, 6=DISCONNECTED");
        Serial.println("[WiFi] Proceeding with cached image (if available)");
    }
}

/**
 * Parse EPBM bitmap header from 8-byte buffer
 * Returns true if valid, false otherwise
 */
bool parse_bitmap_header(uint8_t* header, uint16_t* width, uint16_t* height) {
    // Check magic number "EPBM"
    if (memcmp(header, "EPBM", 4) != 0) {
        Serial.println("[Bitmap] Invalid magic number");
        return false;
    }

    // Read width (big-endian)
    *width = (header[4] << 8) | header[5];

    // Read height (big-endian)
    *height = (header[6] << 8) | header[7];

    Serial.printf("[Bitmap] Header: %dx%d\n", *width, *height);

    // Validate dimensions
    if (*width != EPD_WIDTH || *height != EPD_HEIGHT) {
        Serial.printf("[Bitmap] Dimension mismatch: expected %dx%d\n",
                      EPD_WIDTH, EPD_HEIGHT);
        return false;
    }

    return true;
}

/**
 * Map EPBM color value to GxEPD color constant
 */
uint16_t map_epbm_color(uint8_t color) {
    switch (color) {
        case 0:
            return GxEPD_BLACK;
        case 1:
            return GxEPD_WHITE;
        case 2:
            return GxEPD_GREEN;
        case 3:
            return GxEPD_BLUE;
        case 4:
            return GxEPD_RED;
        case 5:
            return GxEPD_YELLOW;
        default:
            return GxEPD_WHITE;  // Default to white for unknown colors
    }
}

/**
 * Download and render raw bitmap image using streaming (no full buffer)
 * Format: EPBM header (8 bytes) + pixel data (1 byte per pixel)
 * This streams the image directly to the display to avoid running out of memory
 */
void download_and_render_image() {
    if (WiFi.status() != WL_CONNECTED) {
        Serial.println("[Stream] WiFi not connected, skipping download");
        return;
    }

    HTTPClient http;
    String url = String("http://") + SERVER_HOST + ":" + SERVER_PORT + "/" +
                 IMAGE_ENDPOINT;

    Serial.printf("[Stream] Fetching: %s\n", url.c_str());

    http.begin(url);
    http.setTimeout(30000);  // 30 second timeout

    int http_code = http.GET();
    Serial.printf("[Stream] HTTP response code: %d\n", http_code);

    if (http_code != HTTP_CODE_OK) {
        Serial.printf("[Stream] Failed: %s\n",
                      http.errorToString(http_code).c_str());
        http.end();
        return;
    }

    int total_len = http.getSize();
    Serial.printf("[Stream] Content-Length: %d bytes\n", total_len);

    // Expected size: 8 bytes header + 800*480 bytes data = 384008 bytes
    int expected_size = 8 + (EPD_WIDTH * EPD_HEIGHT);
    if (total_len != expected_size) {
        Serial.printf("[Stream] Size mismatch: expected %d bytes\n",
                      expected_size);
        http.end();
        return;
    }

    WiFiClient* stream = http.getStreamPtr();
    unsigned long start_time = millis();

    // Step 1: Read and parse header (8 bytes)
    uint8_t header[8];
    int header_bytes = 0;
    while (http.connected() && header_bytes < 8) {
        if (stream->available()) {
            header[header_bytes++] = stream->read();
        } else {
            delay(1);
        }
        if (millis() - start_time > 5000) {
            Serial.println("[Stream] Timeout reading header");
            http.end();
            return;
        }
    }

    uint16_t width, height;
    if (!parse_bitmap_header(header, &width, &height)) {
        Serial.println("[Stream] Invalid header");
        http.end();
        return;
    }

    // Step 2: Prepare display for streaming
    Serial.println("[Stream] Starting streaming render...");
    display.setFullWindow();
    display.firstPage();

    // Step 3: Stream pixel data directly to display
    // Allocate small buffer for streaming chunks
    uint8_t* pixel_buffer = (uint8_t*)malloc(STREAM_BUFFER_SIZE);
    if (pixel_buffer == nullptr) {
        Serial.printf("[Stream] Failed to allocate %d byte buffer\n",
                      STREAM_BUFFER_SIZE);
        http.end();
        return;
    }

    int total_pixels = width * height;
    int pixels_rendered = 0;
    int bytes_downloaded = 8;  // Already read header

    Serial.print("[Stream] Progress: ");
    int last_pct = 0;

    while (http.connected() && pixels_rendered < total_pixels) {
        // Read chunk of pixel data
        size_t available = stream->available();
        if (available) {
            int to_read =
                min((int)available,
                    min(STREAM_BUFFER_SIZE, total_pixels - pixels_rendered));
            int chunk_size = stream->readBytes(pixel_buffer, to_read);
            bytes_downloaded += chunk_size;

            // Render pixels from this chunk
            for (int i = 0; i < chunk_size; i++) {
                uint8_t color_value = pixel_buffer[i];
                uint16_t gxepd_color = map_epbm_color(color_value);

                int x = pixels_rendered % width;
                int y = pixels_rendered / width;

                display.drawPixel(x, y, gxepd_color);
                pixels_rendered++;
            }

            // Progress indicator every 10%
            int pct = (pixels_rendered * 100) / total_pixels;
            if (pct >= last_pct + 10) {
                Serial.print(".");
                last_pct = pct;
            }
        } else {
            delay(10);
        }

        // Timeout check
        if (millis() - start_time >
            60000) {  // 60 second timeout for full download+render
            Serial.println("\n[Stream] Timeout!");
            free(pixel_buffer);
            http.end();
            return;
        }
    }

    free(pixel_buffer);
    http.end();

    Serial.printf(
        " 100%%\n[Stream] Rendered %d pixels, downloaded %d bytes in %lu ms\n",
        pixels_rendered, bytes_downloaded, millis() - start_time);

    // Step 4: Trigger display refresh
    display.nextPage();

    // E-ink display refresh takes several seconds
    Serial.println("[Stream] Display refresh in progress...");
    delay(5000);
    Serial.println("[Stream] Complete");
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
