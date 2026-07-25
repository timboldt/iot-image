# IoT Image Server Deployment

Files for running the server as a systemd **system** service on Linux, installed
under `/opt/iot-image-server`.

## Files

- `iot-image-server.service` — systemd unit
- `env.txt.example` — configuration template
- `README.md` — this file

## Install / Update

From a clean checkout, build and deploy in one step:

```bash
cd server
cargo build --release && ../deploy/deploy_server.sh
```

`deploy/deploy_server.sh` is idempotent — the same command installs the service
the first time and updates it thereafter. It will:

1. Stop the service if it is running
2. Copy the release binary to `/opt/iot-image-server/iot-image-server`
3. Copy `assets/` to `/opt/iot-image-server/assets`
4. Seed `/opt/iot-image-server/env.txt` from `env.txt.example` if absent
5. Install the unit, `daemon-reload`, enable it, and start it

It uses `sudo` for the steps that write outside your home directory.

The `assets/` copy matters: weather icons are loaded relative to the working
directory, and the unit sets `WorkingDirectory=/opt/iot-image-server`.

## Configuration

Settings live in `/opt/iot-image-server/env.txt`, which the unit reads via
`EnvironmentFile` and expands into command-line flags. On a first install the
deploy script copies the template there; fill it in before starting the service:

```bash
sudo cp deploy/bundle/env.txt.example /opt/iot-image-server/env.txt
sudo "$EDITOR" /opt/iot-image-server/env.txt
sudo systemctl restart iot-image-server
```

API keys are free: [OpenWeatherMap](https://openweathermap.org/api),
[Twelve Data](https://twelvedata.com/),
[FRED](https://fred.stlouisfed.org/docs/api/api_key.html).

`env.txt` is read by a root-installed unit — keep it non-world-readable
(`sudo chmod 600 /opt/iot-image-server/env.txt`).

## Service Management

```bash
sudo systemctl {start,stop,restart,status} iot-image-server
sudo systemctl {enable,disable} iot-image-server     # on/off at boot
sudo journalctl -u iot-image-server -f               # follow logs
```

## Testing

```bash
curl -o test.bin http://localhost:8080/weather/seed-e1002.bin
ls -l test.bin      # expect exactly 384008 bytes (8 header + 800*480 pixels)
```

For a human-readable check, open `http://localhost:8080/weather/svg` in a
browser. Every endpoint has a matching `/svg` form.

## Uninstall

```bash
sudo systemctl stop iot-image-server
sudo systemctl disable iot-image-server
sudo rm /etc/systemd/system/iot-image-server.service
sudo systemctl daemon-reload
sudo rm -rf /opt/iot-image-server        # also removes env.txt
```

## Troubleshooting

**Service won't start** — `sudo journalctl -u iot-image-server -n 50`. Usually a
missing value in `env.txt` (every CLI argument is mandatory) or port 8080 already
in use.

**Connection refused** — confirm it is listening: `ss -tlnp | grep 8080`.

**Weather icons missing, everything else fine** — `assets/` is not present in
`/opt/iot-image-server`, or the unit's `WorkingDirectory` was changed. Re-run the
deploy script.

**Upstream data looks stale or empty** — the server does not cache, so this is an
API problem. Test the key directly:

```bash
curl "https://api.openweathermap.org/data/3.0/onecall?lat=37.7749&lon=-122.4194&units=imperial&exclude=minutely,hourly&appid=YOUR_KEY"
```

## Security Notes

The unit applies `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=strict` and
`ProtectHome=read-only`. Weight CSVs are read from `WEIGHT_DATA_DIR`, which is
reachable under `ProtectHome=read-only`; if you move that directory somewhere the
sandbox blocks, add a matching `ReadOnlyPaths=` to the unit.
