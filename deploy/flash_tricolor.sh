#!/bin/sh
# Build, flash and monitor the tri-colour weather display (Feather ESP32-S2).
set -e
cd "$(dirname "$0")/../client-tricolor"
FQBN=esp32:esp32:adafruit_feather_esp32s2
PORT=${PORT:-/dev/ttyACM0}
arduino-cli compile --fqbn "$FQBN" .
arduino-cli upload -p "$PORT" --fqbn "$FQBN" .
arduino-cli monitor -p "$PORT" -c baudrate=115200 --fqbn "$FQBN"
