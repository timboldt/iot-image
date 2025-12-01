#!/bin/bash
set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Configuration
PI_HOST="${PI_HOST:-pidev.local}"
PI_USER="${PI_USER:-pi}"
REMOTE_DIR="~/bundle"
SERVER_BINARY="${PROJECT_ROOT}/server/target/aarch64-unknown-linux-gnu/release/iot-image-server"
BUNDLE_DIR="${SCRIPT_DIR}/bundle"

echo "=== IoT Image Server Deployment ==="
echo "Target: ${PI_USER}@${PI_HOST}"
echo

# Check if the binary exists
if [ ! -f "${SERVER_BINARY}" ]; then
    echo "Error: Server binary not found at ${SERVER_BINARY}"
    echo "Please build the server first:"
    echo "  cd server && cross build --release --target aarch64-unknown-linux-gnu"
    exit 1
fi

# Step 1: Stop the service on the Pi (if it exists)
echo "Step 1: Stopping service on the Pi (if running)..."
ssh "${PI_USER}@${PI_HOST}" "systemctl --user stop iot-image-server 2>/dev/null || true"
echo "  Service stopped (or wasn't running)"
echo

# Step 2: Copy bundle files and binary to the Pi
echo "Step 2: Copying files to ${PI_HOST}..."
echo "  Creating remote bundle directory..."
ssh "${PI_USER}@${PI_HOST}" "mkdir -p ${REMOTE_DIR}"

echo "  Copying bundle files..."
scp -r "${BUNDLE_DIR}"/* "${PI_USER}@${PI_HOST}:${REMOTE_DIR}/"

echo "  Copying server binary..."
scp "${SERVER_BINARY}" "${PI_USER}@${PI_HOST}:${REMOTE_DIR}/iot-image-server"
echo "  Files copied successfully"
echo

# Step 3: Run the installation script and restart the service
echo "Step 3: Installing and restarting service..."
ssh "${PI_USER}@${PI_HOST}" "cd ${REMOTE_DIR} && ./install.sh"
echo

echo "Step 4: Starting the service..."
ssh "${PI_USER}@${PI_HOST}" "systemctl --user start iot-image-server"
echo

# Check service status
echo "=== Deployment Complete ==="
echo
echo "Checking service status..."
ssh "${PI_USER}@${PI_HOST}" "systemctl --user status iot-image-server --no-pager" || true
echo
echo "To view live logs:"
echo "  ssh ${PI_USER}@${PI_HOST} journalctl --user -u iot-image-server -f"
echo
echo "The server should be available at:"
echo "  http://${PI_HOST}:8080/weather/seed-e1002.bin"
echo
