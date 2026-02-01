#include <GxEPD2_7C.h>  // 7.3" color e-ink
#include <HTTPClient.h>
#include <WiFi.h>

#include "config.h"

// Display modes
enum DisplayMode { MODE_WEATHER = 0, MODE_STOCKS = 1, MODE_FRED = 2 };

// RTC memory to persist display mode across deep sleep
RTC_DATA_ATTR DisplayMode current_mode = MODE_WEATHER;

// Display object for reTerminal e1002 (7.3" color, GDEP073E01)
GxEPD2_7C<GxEPD2_730c_GDEP073E01, GxEPD2_730c_GDEP073E01::HEIGHT> display(
    GxEPD2_730c_GDEP073E01(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN));

// SPI instance for display
SPIClass hspi(HSPI);

// Forward declarations
void setup_wifi();
bool sync_ntp_time();
void setup_buttons();
void check_wake_reason();
void download_and_render_image();
uint32_t calculate_sleep_duration();
void deep_sleep();

void setup() {
    // Use USB Serial for debugging (Serial, not Serial1)
    Serial.begin(115200);
    delay(1000);

    Serial.println("\n\n=== iot-image Client Starting ===");
    Serial.printf("Compiled: %s %s\n", __DATE__, __TIME__);

    // Configure ADC for battery monitoring
    analogReadResolution(12);  // 12-bit resolution (0-4095)
    analogSetPinAttenuation(
        BATTERY_ADC_PIN, ADC_11db);  // 11dB attenuation for up to ~3.6V input

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

    // Setup buttons and check wake reason
    setup_buttons();
    check_wake_reason();

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

        // Sync time with NTP servers
        sync_ntp_time();
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
 * Sync time with NTP servers
 * Returns true if successful, false otherwise
 */
bool sync_ntp_time() {
    Serial.println("[NTP] Syncing time...");

    // Configure NTP with timezone and DST
    configTime(TIMEZONE_OFFSET_SEC, DST_OFFSET_SEC,
               "pool.ntp.org", "time.nist.gov", "time.google.com");

    // Wait for time to be set (max 10 seconds)
    int retry = 0;
    const int max_retries = 20;
    while (time(nullptr) < 100000 && retry < max_retries) {
        delay(500);
        Serial.print(".");
        retry++;
    }
    Serial.println();

    if (time(nullptr) < 100000) {
        Serial.println("[NTP] Failed to sync time!");
        return false;
    }

    // Log current time for debugging
    time_t now = time(nullptr);
    struct tm* timeinfo = localtime(&now);
    Serial.printf("[NTP] Time synced: %04d-%02d-%02d %02d:%02d:%02d\n",
                  timeinfo->tm_year + 1900, timeinfo->tm_mon + 1,
                  timeinfo->tm_mday, timeinfo->tm_hour,
                  timeinfo->tm_min, timeinfo->tm_sec);

    return true;
}

/**
 * Setup button pins with pullup resistors
 * Buttons are active-low (LOW when pressed)
 */
void setup_buttons() {
    pinMode(BUTTON_KEY0, INPUT_PULLUP);  // Button 1 (Green - FRED)
    pinMode(BUTTON_KEY1, INPUT_PULLUP);  // Button 2 (Middle - Stocks)
    pinMode(BUTTON_KEY2, INPUT_PULLUP);  // Button 3 (Left - Weather)

    Serial.println("[Buttons] Initialized with pullups");
}

/**
 * Check wake reason and update display mode if button was pressed
 * Buttons are active-low, so we wake on LOW level
 */
void check_wake_reason() {
    esp_sleep_wakeup_cause_t wakeup_reason = esp_sleep_get_wakeup_cause();

    switch (wakeup_reason) {
        case ESP_SLEEP_WAKEUP_EXT1: {
            Serial.println("[Wake] Woke up from button press");

            // Get which GPIO pin(s) caused the wakeup (more reliable than
            // digitalRead)
            uint64_t wakeup_pin_mask = esp_sleep_get_ext1_wakeup_status();

            // Check which button triggered the wakeup
            if (wakeup_pin_mask & (1ULL << BUTTON_KEY0)) {
                Serial.println(
                    "[Wake] Button 1 (Green) pressed - switching to FRED");
                current_mode = MODE_FRED;
            } else if (wakeup_pin_mask & (1ULL << BUTTON_KEY1)) {
                Serial.println(
                    "[Wake] Button 2 (Middle) pressed - switching to STOCKS");
                current_mode = MODE_STOCKS;
            } else if (wakeup_pin_mask & (1ULL << BUTTON_KEY2)) {
                Serial.println(
                    "[Wake] Button 3 (Left) pressed - switching to WEATHER");
                current_mode = MODE_WEATHER;
            }

            delay(200);  // Debounce
            break;
        }

        case ESP_SLEEP_WAKEUP_TIMER:
            Serial.println("[Wake] Woke up from timer");
            Serial.printf(
                "[Wake] Current mode: %s\n",
                current_mode == MODE_WEATHER
                    ? "WEATHER"
                    : (current_mode == MODE_STOCKS ? "STOCKS" : "FRED"));
            break;

        default:
            Serial.printf("[Wake] First boot or reset (reason: %d)\n",
                          wakeup_reason);
            Serial.printf(
                "[Wake] Current mode: %s\n",
                current_mode == MODE_WEATHER
                    ? "WEATHER"
                    : (current_mode == MODE_STOCKS ? "STOCKS" : "FRED"));
            break;
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
 * Display a white screen with red border to indicate error/stale data
 * Uses full screen refresh for reliability after deep sleep
 */
void show_error_screen() {
    Serial.println("[Display] Drawing error screen");

    display.setFullWindow();
    display.firstPage();

    do {
        // Draw red border (10 pixels thick)
        // Top border
        display.fillRect(0, 0, 800, 10, GxEPD_RED);
        // Bottom border
        display.fillRect(0, 470, 800, 10, GxEPD_RED);
        // Left border
        display.fillRect(0, 0, 10, 480, GxEPD_RED);
        // Right border
        display.fillRect(790, 0, 10, 480, GxEPD_RED);
    } while (display.nextPage());

    Serial.println("[Display] Error screen complete");
}

/**
 * Get battery percentage from ESP32 ADC
 * Returns 0-100, or -1 if unavailable
 */
int get_battery_percentage() {
    // Enable battery monitoring circuit
    pinMode(BATTERY_ENABLE_PIN, OUTPUT);
    digitalWrite(BATTERY_ENABLE_PIN, HIGH);
    delay(10);  // Allow voltage to stabilize

    // Read battery voltage from ADC (returns millivolts)
    int mv = analogReadMilliVolts(BATTERY_ADC_PIN);

    // Disable battery monitoring to save power
    digitalWrite(BATTERY_ENABLE_PIN, LOW);

    // Convert to actual battery voltage
    // Voltage divider: Vbat -> R1(10k) -> ADC -> R2(10k) -> GND
    // ADC sees Vbat/2, so multiply by 2 to get actual battery voltage
    float voltage = (mv / 1000.0) * 2.0;

    // Convert voltage to percentage
    // LiPo battery: 4.2V = 100%, 3.0V = 0%
    float min_voltage = 3.0;
    float max_voltage = 4.2;
    int percentage =
        ((voltage - min_voltage) / (max_voltage - min_voltage)) * 100;

    // Clamp to 0-100
    if (percentage > 100) percentage = 100;
    if (percentage < 0) percentage = 0;

    Serial.printf("[Battery] mV: %d, Voltage: %.2fV, Percentage: %d%%\n", mv,
                  voltage, percentage);
    return percentage;
}

/**
 * Download and render raw bitmap image using streaming (no full buffer)
 * Format: EPBM header (8 bytes) + pixel data (1 byte per pixel)
 * This streams the image directly to the display to avoid running out of memory
 */
void download_and_render_image() {
    if (WiFi.status() != WL_CONNECTED) {
        Serial.println("[Stream] WiFi not connected, skipping download");
        show_error_screen();
        return;
    }

    // Select endpoint based on current display mode
    const char* endpoint;
    if (current_mode == MODE_WEATHER) {
        endpoint = "weather/seed-e1002.bin";
    } else if (current_mode == MODE_STOCKS) {
        endpoint = "stocks/seed-e1002.bin";
    } else {
        endpoint = "fred/seed-e1002.bin";
    }

    HTTPClient http;
    String url =
        String("http://") + SERVER_HOST + ":" + SERVER_PORT + "/" + endpoint;

    // Add battery percentage query parameter.
    int battery_pct = get_battery_percentage();
    if (battery_pct >= 0) {
        url += "?battery_pct=" + String(battery_pct);
    }

    Serial.printf("[Stream] Mode: %s\n",
                  current_mode == MODE_WEATHER
                      ? "WEATHER"
                      : (current_mode == MODE_STOCKS ? "STOCKS" : "FRED"));
    Serial.printf("[Stream] Fetching: %s\n", url.c_str());

    http.begin(url);
    http.setTimeout(30000);  // 30 second timeout

    int http_code = http.GET();
    Serial.printf("[Stream] HTTP response code: %d\n", http_code);

    if (http_code != HTTP_CODE_OK) {
        Serial.printf("[Stream] Failed: %s\n",
                      http.errorToString(http_code).c_str());
        http.end();
        show_error_screen();
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
        show_error_screen();
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
            show_error_screen();
            return;
        }
    }

    uint16_t width, height;
    if (!parse_bitmap_header(header, &width, &height)) {
        Serial.println("[Stream] Invalid header");
        http.end();
        show_error_screen();
        return;
    }

    // Step 2: Prepare display for streaming
    Serial.println("[Stream] Starting streaming render...");
    display.setFullWindow();
    display.fillScreen(GxEPD_WHITE);
    display.firstPage();

    // Step 3: Stream pixel data directly to display
    // Allocate small buffer for streaming chunks
    uint8_t* pixel_buffer = (uint8_t*)malloc(STREAM_BUFFER_SIZE);
    if (pixel_buffer == nullptr) {
        Serial.printf("[Stream] Failed to allocate %d byte buffer\n",
                      STREAM_BUFFER_SIZE);
        http.end();
        show_error_screen();
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
            show_error_screen();
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
 * Calculate seconds until next scheduled wake time
 * Wake times: 6am, 12pm, 6pm, midnight (local time)
 * Returns: seconds to sleep, or FALLBACK_SLEEP_SEC if time not synced
 */
uint32_t calculate_sleep_duration() {
    time_t now = time(nullptr);

    // Check if time is valid (synced)
    if (now < 100000) {
        Serial.println("[Sleep] Time not synced, using fallback (1 hour)");
        return FALLBACK_SLEEP_SEC;
    }

    struct tm* current = localtime(&now);
    struct tm next_wake = *current;

    // Scheduled wake hours in ascending order
    const int wake_hours[] = {WAKE_HOUR_1, WAKE_HOUR_2, WAKE_HOUR_3};
    const int num_wake_times = sizeof(wake_hours) / sizeof(wake_hours[0]);

    // Current time in minutes since midnight
    int current_minutes = current->tm_hour * 60 + current->tm_min;

    // Find next wake time (must be at least 5 minutes away to avoid double-refresh)
    bool found = false;
    for (int i = 0; i < num_wake_times; i++) {
        int wake_minutes = wake_hours[i] * 60;

        if (wake_minutes > current_minutes) {
            // Check if this wake time is far enough in the future
            struct tm temp_wake = *current;
            temp_wake.tm_hour = wake_hours[i];
            temp_wake.tm_min = 0;
            temp_wake.tm_sec = 0;

            time_t temp_wake_time = mktime(&temp_wake);
            int32_t temp_sleep = difftime(temp_wake_time, now);

            // Only use this wake time if it's at least 5 minutes away
            // This prevents double-refresh when waking slightly before scheduled time
            if (temp_sleep >= 300) {
                next_wake.tm_hour = wake_hours[i];
                next_wake.tm_min = 0;
                next_wake.tm_sec = 0;
                found = true;
                break;
            }
        }
    }

    if (!found) {
        // All wake times today have passed, wake at first time tomorrow
        next_wake.tm_mday += 1;
        next_wake.tm_hour = wake_hours[0];
        next_wake.tm_min = 0;
        next_wake.tm_sec = 0;
    }

    // Convert to time_t and calculate difference
    time_t next_wake_time = mktime(&next_wake);
    int32_t sleep_seconds = difftime(next_wake_time, now);

    // Safety check: ensure positive sleep duration
    if (sleep_seconds <= 0) {
        Serial.printf("[Sleep] Invalid duration: %d sec, using fallback\n",
                      sleep_seconds);
        return FALLBACK_SLEEP_SEC;
    }

    // Log sleep schedule
    Serial.printf("[Sleep] Current time: %02d:%02d:%02d\n",
                  current->tm_hour, current->tm_min, current->tm_sec);
    Serial.printf("[Sleep] Next wake: %02d:%02d:%02d (%d seconds / %.2f hours)\n",
                  next_wake.tm_hour, next_wake.tm_min, next_wake.tm_sec,
                  sleep_seconds, sleep_seconds / 3600.0);

    return (uint32_t)sleep_seconds;
}

/**
 * Deep sleep until next update
 * Wakes automatically from timer OR button press
 */
void deep_sleep() {
    // Calculate sleep duration to next scheduled wake time
    uint32_t sleep_duration_sec = calculate_sleep_duration();
    Serial.printf("[Sleep] Sleeping for %u seconds\n", sleep_duration_sec);
    Serial.printf("[Sleep] Current mode: %s (will persist on timer wake)\n",
                  current_mode == MODE_WEATHER
                      ? "WEATHER"
                      : (current_mode == MODE_STOCKS ? "STOCKS" : "FRED"));

    // Calculate wake time in microseconds
    uint64_t sleep_duration_us = sleep_duration_sec * 1000000ULL;

    // Enable wake on timer
    esp_sleep_enable_timer_wakeup(sleep_duration_us);

    // Enable wake on button press (ext1 with multiple pins)
    // Buttons are active-low, so wake when ANY button goes LOW
    uint64_t button_mask =
        (1ULL << BUTTON_KEY0) | (1ULL << BUTTON_KEY1) | (1ULL << BUTTON_KEY2);
    esp_sleep_enable_ext1_wakeup(button_mask, ESP_EXT1_WAKEUP_ANY_LOW);

    Serial.println("[Sleep] Wake sources: timer + buttons (any button press)");

    // Gracefully print before sleeping
    Serial.flush();
    delay(100);

    // Enter deep sleep
    esp_deep_sleep_start();

    // Fallback: regular delay (for testing/development)
    // Should never reach here in production
    delay(FALLBACK_SLEEP_SEC * 1000);
    Serial.println("[Sleep] Woke up!");
}
