# IoT Image Server Deployment

This directory contains files for deploying the IoT Image Server as a systemd service on a Raspberry Pi.

## Files

- `iot-image-server.service` - systemd service unit file
- `weather.env.example` - Environment variable template
- `install.sh` - Automated installation script
- `README.md` - This file

## Quick Installation

### Automated Deployment (Recommended)

Build and deploy in one command from the server directory:

```bash
cd server
cross build --release --target aarch64-unknown-linux-gnu && ../deploy/deploy_server.sh
```

The deployment script will:
- Stop the service on the Pi (if running)
- Copy all bundle files and the binary to the Pi
- Run the installation script
- Start/restart the service

**Note:** The deploy script can be run from any directory - it automatically finds the project root and binary location.

### Manual Installation

1. Build for Raspberry Pi (cross-compile):
   ```bash
   cd server
   cross build --release --target aarch64-unknown-linux-gnu
   cd ..
   ```

2. Copy the bundle to the Pi:
   ```bash
   scp -r deploy/bundle pi@pidev.local:~/
   scp server/target/aarch64-unknown-linux-gnu/release/iot-image-server pi@pidev.local:~/bundle/
   ```

3. SSH to the Pi and run the installation script as user 'pi':
   ```bash
   ssh pi@pidev.local
   cd ~/bundle
   ./install.sh
   ```

4. Verify your configuration file exists:
   ```bash
   cat ~/bin/env.txt
   ```
   (Note: The service expects `~/bin/env.txt` to already exist with your credentials)

5. Start the service:
   ```bash
   systemctl --user start iot-image-server
   ```

## Manual Installation

If you prefer to install manually:

1. Create the installation directory:
   ```bash
   mkdir -p ~/bin
   ```

2. Copy the binary:
   ```bash
   cp iot-image-server ~/bin/
   chmod +x ~/bin/iot-image-server
   ```

3. Ensure the environment file exists:
   ```bash
   # Verify env.txt exists with your credentials
   cat ~/bin/env.txt
   ```

4. Install the systemd user service:
   ```bash
   mkdir -p ~/.config/systemd/user
   cp iot-image-server.service ~/.config/systemd/user/
   ```

5. Enable lingering and start the service:
   ```bash
   # Enable lingering so the service runs without login
   sudo loginctl enable-linger pi

   # Reload and enable the service
   systemctl --user daemon-reload
   systemctl --user enable iot-image-server
   systemctl --user start iot-image-server
   ```

## Configuration

The service reads configuration from `~/bin/env.txt`. This file should contain:

```bash
OPEN_WEATHER_LAT=37.7749          # Your latitude
OPEN_WEATHER_LON=-122.4194        # Your longitude
OPEN_WEATHER_KEY=your_api_key     # Your OpenWeatherMap API key
PORT=8080                          # Server port (default: 8080)
```

Get an OpenWeatherMap API key at: https://openweathermap.org/api

**Note:** The installation script expects `env.txt` to already exist in `~/bin/`. Make sure this file is present before starting the service.

## Service Management

Start the service:
```bash
systemctl --user start iot-image-server
```

Stop the service:
```bash
systemctl --user stop iot-image-server
```

Restart the service:
```bash
systemctl --user restart iot-image-server
```

Check status:
```bash
systemctl --user status iot-image-server
```

View logs:
```bash
# Follow live logs
journalctl --user -u iot-image-server -f

# View recent logs
journalctl --user -u iot-image-server -n 100

# View logs since boot
journalctl --user -u iot-image-server -b
```

Enable on boot:
```bash
systemctl --user enable iot-image-server
```

Disable on boot:
```bash
systemctl --user disable iot-image-server
```

## Testing

Once the service is running, test the endpoint:

```bash
# Download the bitmap
curl http://localhost:8080/weather/seed-e1002.bin -o test.bin

# Check the file size (should be ~192000 bytes for 800x480 display)
ls -lh test.bin
```

## Updating

### Automated Update (Recommended)

Simply rebuild and re-run the deployment script in one command:

```bash
cd server
cross build --release --target aarch64-unknown-linux-gnu && ../deploy/deploy_server.sh
```

The deployment script handles stopping, updating, and restarting the service automatically.

### Manual Update

1. Build and copy the new binary from your workstation:
   ```bash
   # On workstation
   cd server
   cross build --release --target aarch64-unknown-linux-gnu
   scp target/aarch64-unknown-linux-gnu/release/iot-image-server pi@pidev.local:~/bundle/
   ```

2. On the Pi, stop the service:
   ```bash
   systemctl --user stop iot-image-server
   ```

3. Replace the binary:
   ```bash
   cp ~/bundle/iot-image-server ~/bin/
   ```

4. Start the service:
   ```bash
   systemctl --user start iot-image-server
   ```

Or simply re-run the installation script:
```bash
cd ~/bundle
./install.sh
systemctl --user restart iot-image-server
```

## Uninstallation

To completely remove the service:

```bash
# Stop and disable the service
systemctl --user stop iot-image-server
systemctl --user disable iot-image-server

# Remove the service file
rm ~/.config/systemd/user/iot-image-server.service

# Reload systemd
systemctl --user daemon-reload

# Optionally disable lingering if you don't need it for other services
sudo loginctl disable-linger pi

# Remove the installation directory (keep env.txt if you want to preserve credentials)
rm -f ~/bin/iot-image-server
```

## Troubleshooting

### Service won't start

Check the logs for errors:
```bash
journalctl --user -u iot-image-server -n 50
```

Common issues:
- Missing or incorrect API credentials in `env.txt`
- Port 8080 already in use (change PORT in `env.txt`)
- Binary not executable (run `chmod +x ~/bin/iot-image-server`)
- Lingering not enabled (run `sudo loginctl enable-linger pi`)
- Environment file not found (ensure `~/bin/env.txt` exists)

### Connection refused

Ensure the service is running:
```bash
systemctl --user status iot-image-server
```

Check if the port is listening:
```bash
ss -tlnp | grep 8080
```

### Weather data not updating

Check API key validity and network connectivity:
```bash
# Test API manually
curl "https://api.openweathermap.org/data/3.0/onecall?lat=37.7749&lon=-122.4194&units=imperial&exclude=minutely,hourly&appid=YOUR_KEY"
```

## Security Notes

The systemd user service includes several security hardening features:
- Runs as the pi user (no root required)
- No new privileges
- Private /tmp directory
- Protected system directories
- Read-only home directory access (except /tmp)

To modify these settings, edit `~/.config/systemd/user/iot-image-server.service` and reload systemd with `systemctl --user daemon-reload`.
