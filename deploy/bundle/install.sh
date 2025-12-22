#!/bin/bash
set -e

echo "=== IoT Image Server Installation ==="
echo

# Ensure we're not running as root
if [ "$(whoami)" = "root" ]; then
    echo "Error: Please run this script as a regular user (not as root)"
    echo "Usage: ./install.sh"
    exit 1
fi

# Configuration
INSTALL_DIR="${HOME}/bin"
SERVICE_NAME="iot-image-server"
BINARY_NAME="iot-image-server"
USER_SYSTEMD_DIR="${HOME}/.config/systemd/user"

# Verify the binary exists in the bundle directory
if [ ! -f "${BINARY_NAME}" ]; then
    echo "Error: Binary not found in current directory: ${BINARY_NAME}"
    echo "Please ensure you're running this script from the bundle directory"
    exit 1
fi

# Create installation directory
echo "Creating installation directory: ${INSTALL_DIR}"
mkdir -p "${INSTALL_DIR}"

# Copy binary
echo "Installing binary..."
cp "${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

# Copy assets
if [ -d "assets" ]; then
    echo "Installing assets..."
    cp -r assets "${INSTALL_DIR}/"
else
    echo "WARNING: Assets directory not found. Weather icons may not display correctly."
fi

# Check for environment file
if [ ! -f "${INSTALL_DIR}/env.txt" ]; then
    echo "WARNING: Environment file not found at ${INSTALL_DIR}/env.txt"
    echo "Please ensure env.txt exists with your credentials before starting the service."
    echo
else
    echo "Environment file found at ${INSTALL_DIR}/env.txt"
fi

# Create user systemd directory
echo "Creating user systemd directory..."
mkdir -p "${USER_SYSTEMD_DIR}"

# Install systemd user service
echo "Installing systemd user service..."
cp "${SERVICE_NAME}.service" "${USER_SYSTEMD_DIR}/${SERVICE_NAME}.service"

# Enable lingering for the current user so service runs without login
CURRENT_USER="$(whoami)"
echo "Enabling lingering for user '${CURRENT_USER}'..."
if ! loginctl show-user "${CURRENT_USER}" | grep -q "Linger=yes"; then
    sudo loginctl enable-linger "${CURRENT_USER}"
    echo "Lingering enabled (requires sudo)"
else
    echo "Lingering already enabled"
fi

# Reload systemd user daemon
echo "Reloading systemd user daemon..."
systemctl --user daemon-reload

# Enable service
echo "Enabling service to start on boot..."
systemctl --user enable "${SERVICE_NAME}"

echo
echo "=== Installation Complete ==="
echo
echo "Next steps:"
echo "1. Verify the configuration file exists:"
echo "   cat ${INSTALL_DIR}/env.txt"
echo
echo "2. Start the service:"
echo "   systemctl --user start ${SERVICE_NAME}"
echo
echo "3. Check service status:"
echo "   systemctl --user status ${SERVICE_NAME}"
echo
echo "4. View logs:"
echo "   journalctl --user -u ${SERVICE_NAME} -f"
echo
echo "The server will be available at:"
echo "   http://localhost:8080/weather/seed-e1002.bin"
echo
