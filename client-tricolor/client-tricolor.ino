// Tri-colour weather display: wake, fetch a 296x128 EPBM bitmap from the
// iot-image server, show it, and deep-sleep until the next scheduled hour.
//
// The bitmap (37,896 bytes) is downloaded and validated in full before the
// panel is touched, so a failed update leaves the previous image on screen.

#include <GxEPD2_3C.h>
#include <HTTPClient.h>
#include <WiFi.h>
#include <Wire.h>

#include "config.h"

GxEPD2_3C<GxEPD2_290c, GxEPD2_290c::HEIGHT> display(
    GxEPD2_290c(EPD_CS_PIN, EPD_DC_PIN, EPD_RES_PIN, EPD_BUSY_PIN));

bool setup_wifi();
bool sync_ntp_time();
int get_battery_percentage();
uint8_t* download_bitmap(int battery_pct);
void show_bitmap(const uint8_t* bitmap);
void show_low_battery(int battery_pct);
uint32_t seconds_until_next_wake();
void deep_sleep(uint32_t seconds);

void setup() {
    Serial.begin(115200);
    delay(1000);
    Serial.println("\n\n=== iot-image Tri-colour Client Starting ===");
    Serial.printf("Compiled: %s %s\n", __DATE__, __TIME__);

    int battery_pct = get_battery_percentage();
    if (battery_pct >= 0 && battery_pct <= LOW_BATTERY_PCT) {
        // The clock is not synced without WiFi, so sleep a whole day rather
        // than aim for a time of day.
        Serial.printf("[Battery] Low (%d%%), skipping WiFi\n", battery_pct);
        show_low_battery(battery_pct);
        deep_sleep(LOW_BATTERY_SLEEP_SEC);
    }

    uint32_t sleep_sec = ERROR_RETRY_SEC;
    if (setup_wifi()) {
        bool synced = sync_ntp_time();
        uint8_t* bitmap = download_bitmap(battery_pct);
        WiFi.disconnect(true);  // Radio off before the slow panel refresh
        if (bitmap != nullptr) {
            show_bitmap(bitmap);
            free(bitmap);
            sleep_sec = synced ? seconds_until_next_wake() : FALLBACK_SLEEP_SEC;
        }
    }
    WiFi.disconnect(true);
    deep_sleep(sleep_sec);
}

void loop() {
    // Not used: every wake runs setup() and ends in deep sleep.
}

bool setup_wifi() {
    Serial.printf("[WiFi] Connecting to %s\n", WIFI_SSID);
    WiFi.mode(WIFI_STA);
    WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

    const unsigned long deadline = millis() + 20000;
    while (WiFi.status() != WL_CONNECTED && millis() < deadline) {
        delay(250);
    }
    if (WiFi.status() != WL_CONNECTED) {
        Serial.printf("[WiFi] Failed (status %d)\n", WiFi.status());
        return false;
    }
    Serial.printf("[WiFi] Connected, IP %s, RSSI %d dBm\n",
                  WiFi.localIP().toString().c_str(), WiFi.RSSI());
    return true;
}

bool sync_ntp_time() {
    configTzTime(TZ_INFO, "pool.ntp.org", "time.nist.gov", "time.google.com");
    const unsigned long deadline = millis() + 10000;
    while (time(nullptr) < 100000 && millis() < deadline) {
        delay(250);
    }
    if (time(nullptr) < 100000) {
        Serial.println("[NTP] Failed to sync time");
        return false;
    }
    time_t now = time(nullptr);
    struct tm* t = localtime(&now);
    Serial.printf("[NTP] %04d-%02d-%02d %02d:%02d:%02d\n", t->tm_year + 1900,
                  t->tm_mon + 1, t->tm_mday, t->tm_hour, t->tm_min, t->tm_sec);
    return true;
}

/**
 * State of charge from the MAX17048 (register 0x04: whole percent in the high
 * byte, 1/256ths in the low byte). Returns -1 if the gauge does not answer.
 */
int get_battery_percentage() {
    Wire.begin();
    Wire.beginTransmission(MAX17048_ADDR);
    Wire.write(MAX17048_REG_SOC);
    if (Wire.endTransmission(false) != 0 ||
        Wire.requestFrom(MAX17048_ADDR, 2) != 2) {
        Serial.println("[Battery] Fuel gauge not responding");
        return -1;
    }
    uint8_t whole = Wire.read();
    uint8_t frac = Wire.read();
    int pct = whole + (frac >= 128 ? 1 : 0);
    if (pct > 100) pct = 100;
    Serial.printf("[Battery] %d%%\n", pct);
    return pct;
}

/**
 * Downloads and validates the whole bitmap. Returns a malloc'd buffer of
 * EPBM_SIZE bytes, or nullptr on any failure.
 */
uint8_t* download_bitmap(int battery_pct) {
    String url = String("http://") + SERVER_HOST + ":" + SERVER_PORT + "/" +
                 SERVER_ENDPOINT;
    if (battery_pct >= 0) {
        url += "?battery_pct=" + String(battery_pct);
    }
    Serial.printf("[HTTP] GET %s\n", url.c_str());

    HTTPClient http;
    http.begin(url);
    http.setTimeout(30000);
    int code = http.GET();
    if (code != HTTP_CODE_OK) {
        Serial.printf("[HTTP] Failed: %d %s\n", code,
                      http.errorToString(code).c_str());
        http.end();
        return nullptr;
    }
    int len = http.getSize();
    if (len != EPBM_SIZE) {
        Serial.printf("[HTTP] Size %d, expected %d\n", len, EPBM_SIZE);
        http.end();
        return nullptr;
    }

    uint8_t* buf = (uint8_t*)malloc(EPBM_SIZE);
    if (buf == nullptr) {
        Serial.println("[HTTP] Out of memory");
        http.end();
        return nullptr;
    }
    WiFiClient* stream = http.getStreamPtr();
    stream->setTimeout(5000);
    size_t got = 0;
    const unsigned long deadline = millis() + 30000;
    while (got < EPBM_SIZE && http.connected() && millis() < deadline) {
        got += stream->readBytes(buf + got, EPBM_SIZE - got);
    }
    http.end();

    uint16_t width = (buf[4] << 8) | buf[5];
    uint16_t height = (buf[6] << 8) | buf[7];
    if (got != EPBM_SIZE || memcmp(buf, "EPBM", 4) != 0 ||
        width != EPD_WIDTH || height != EPD_HEIGHT) {
        Serial.printf("[HTTP] Bad bitmap: %u of %d bytes, %ux%u\n",
                      (unsigned)got, EPBM_SIZE, width, height);
        free(buf);
        return nullptr;
    }
    Serial.printf("[HTTP] Received %d bytes\n", EPBM_SIZE);
    return buf;
}

void init_display() {
    display.init(115200);
    display.setRotation(EPD_ROTATION);
    display.setFullWindow();
    display.fillScreen(GxEPD_WHITE);
}

/** EPBM palette indices: 0 black, 4 red; anything else is drawn white. */
void show_bitmap(const uint8_t* bitmap) {
    init_display();
    const uint8_t* pixels = bitmap + EPBM_HEADER_SIZE;
    for (int y = 0; y < EPD_HEIGHT; y++) {
        for (int x = 0; x < EPD_WIDTH; x++) {
            uint8_t c = pixels[y * EPD_WIDTH + x];
            if (c == 0) {
                display.drawPixel(x, y, GxEPD_BLACK);
            } else if (c == 4) {
                display.drawPixel(x, y, GxEPD_RED);
            }
        }
    }
    display.display(false);  // Full refresh; blocks until the panel is done
    display.hibernate();
    Serial.println("[Display] Refreshed");
}

void show_low_battery(int battery_pct) {
    init_display();
    display.fillRect(0, 0, EPD_WIDTH, 40, GxEPD_RED);
    display.setTextColor(GxEPD_WHITE);
    display.setTextSize(2);
    display.setCursor(8, 13);
    display.printf("Battery low: %d%%", battery_pct);
    display.setTextColor(GxEPD_BLACK);
    display.setTextSize(1);
    display.setCursor(8, 56);
    display.print("Charge to resume weather updates.");
    display.display(false);
    display.hibernate();
}

/** Seconds until the next hour in WAKE_HOURS that is at least 5 minutes away. */
uint32_t seconds_until_next_wake() {
    const int wake_hours[] = WAKE_HOURS;
    const int count = sizeof(wake_hours) / sizeof(wake_hours[0]);
    time_t now = time(nullptr);
    struct tm today = *localtime(&now);

    for (int day = 0; day <= 1; day++) {
        for (int i = 0; i < count; i++) {
            struct tm wake = today;
            wake.tm_mday += day;
            wake.tm_hour = wake_hours[i];
            wake.tm_min = 0;
            wake.tm_sec = 0;
            wake.tm_isdst = -1;  // Let mktime work out DST for that day
            double secs = difftime(mktime(&wake), now);
            if (secs >= 300) {
                Serial.printf("[Sleep] Next wake at %02d:00 in %.0f s\n",
                              wake_hours[i], secs);
                return (uint32_t)secs;
            }
        }
    }
    return FALLBACK_SLEEP_SEC;
}

void deep_sleep(uint32_t seconds) {
    Serial.printf("[Sleep] Sleeping for %u seconds\n", seconds);
    Serial.flush();
    // Cut power to the I2C bus (fuel gauge pull-ups) while asleep.
#ifdef PIN_I2C_POWER
    pinMode(PIN_I2C_POWER, INPUT);
#endif
    esp_sleep_enable_timer_wakeup((uint64_t)seconds * 1000000ULL);
    esp_deep_sleep_start();
}
