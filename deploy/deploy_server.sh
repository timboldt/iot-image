#!/bin/bash
set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Configuration for local deployment
INSTALL_DIR="/opt/iot-image-server"
SERVER_BINARY="${PROJECT_ROOT}/server/target/release/iot-image-server"
BUNDLE_DIR="${SCRIPT_DIR}/bundle"
SERVICE_NAME="iot-image-server"
SYSTEM_SYSTEMD_DIR="/etc/systemd/system"

echo "=== IoT Image Server Local Deployment ==="
echo "Target: localhost"
echo

# Check if the binary exists
if [ ! -f "${SERVER_BINARY}" ]; then
    echo "Error: Server binary not found at ${SERVER_BINARY}"
    echo "Please build the server first:"
    echo "  cd server && cargo build --release"
    exit 1
fi

# Step 1: Stop the service (if it exists)
echo "Step 1: Stopping service (if running)..."
sudo systemctl stop iot-image-server 2>/dev/null || true
echo "  Service stopped (or wasn't running)"
echo

# Step 2: Create installation directory
echo "Step 2: Installing files..."
echo "  Creating installation directory: ${INSTALL_DIR}"
sudo mkdir -p "${INSTALL_DIR}"

# Step 3: Copy binary
echo "  Installing binary..."
sudo cp "${SERVER_BINARY}" "${INSTALL_DIR}/iot-image-server"
sudo chmod +x "${INSTALL_DIR}/iot-image-server"

# Step 4: Copy assets
echo "  Installing assets..."
if [ -d "${PROJECT_ROOT}/assets" ]; then
    sudo cp -r "${PROJECT_ROOT}/assets" "${INSTALL_DIR}/"
else
    echo "WARNING: Assets directory not found. Weather icons may not display correctly."
fi

# Step 5: Check for environment file
if [ ! -f "${INSTALL_DIR}/env.txt" ]; then
    echo ""
    echo "WARNING: Environment file not found at ${INSTALL_DIR}/env.txt"
    echo "Creating example env.txt from template..."
    if [ -f "${BUNDLE_DIR}/env.txt.example" ]; then
        sudo cp "${BUNDLE_DIR}/env.txt.example" "${INSTALL_DIR}/env.txt"
        echo "Please edit ${INSTALL_DIR}/env.txt with your API keys before starting the service."
    fi
    echo
else
    echo "  Environment file found at ${INSTALL_DIR}/env.txt"
fi

# Step 6: Install systemd system service
echo "Step 3: Installing systemd system service..."
sudo cp "${BUNDLE_DIR}/${SERVICE_NAME}.service" "${SYSTEM_SYSTEMD_DIR}/${SERVICE_NAME}.service"

# Step 7: Reload systemd daemon
echo "  Reloading systemd daemon..."
sudo systemctl daemon-reload

# Step 8: Enable service
echo "  Enabling service to start on boot..."
sudo systemctl enable "${SERVICE_NAME}"

# Step 9: Start the service
echo "Step 4: Starting the service..."
sudo systemctl start iot-image-server
echo

# Check service status
echo "=== Deployment Complete ==="
echo
echo "Checking service status..."
sudo systemctl status iot-image-server --no-pager || true
echo
echo "To view live logs:"
echo "  sudo journalctl -u iot-image-server -f"
echo
echo "The server should be available at:"
echo "  http://localhost:8080/weather/seed-e1002.bin"
echo "  http://`hostname`.local:8080/weather/seed-e1002.bin"
echo
