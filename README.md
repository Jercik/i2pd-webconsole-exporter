# i2pd‑webconsole‑exporter

A **tiny, pure‑Rust** Prometheus exporter that turns the i2pd web console into metrics.

> **Heads‑up:** the exporter scrapes raw HTML—an i2pd update that changes the console can break it.

---

## Highlights

- Reads the web console and serves metrics on **:9700** (configurable).
- Negligible memory & CPU footprint.
- Metrics cover network health, tunnel success, traffic, router capabilities, counts, service flags and exporter version.

---

## Quick start

```bash
cargo build --release               # native build
./target/release/i2pd-webconsole-exporter --version # Check version
./target/release/i2pd-webconsole-exporter      # Run the exporter
```

### Static Linux (Docker)

```bash
./build-static-linux-docker.sh      # outputs to ./dist/
```

---

## Releases

GitHub releases include pre-compiled static Linux binaries (`.tar.gz`) for `x86_64` and `aarch64`, plus macOS binaries. Each release also provides a `sha256sums.txt` file for verifying archive integrity.

---

## Configuration

Set environment variables:

| Variable               | Default                 | Purpose                        |
| ---------------------- | ----------------------- | ------------------------------ |
| `I2PD_WEB_CONSOLE`     | `http://127.0.0.1:7070` | i2pd web console URL           |
| `METRICS_LISTEN_ADDR`  | `0.0.0.0:9700`          | Address:port for metrics       |
| `HTTP_TIMEOUT_SECONDS` | `60`                    | HTTP request timeout (seconds) |

---

## Metrics cheat‑sheet

- `i2p_network_status_v4{status}`, `i2p_network_status_v6{status}`
- `i2p_tunnel_creation_success_rate`
- `i2p_data_received_bytes`, `i2p_data_sent_bytes`, `i2p_data_transit_bytes`
- `i2p_data_rate_bytes_per_second{direction}`
- `i2p_router_capabilities`
- `i2p_external_address{protocol,address}`
- `i2p_network_{routers,floodfills,leasesets}`
- `i2p_{client,transit}_tunnels`
- `i2p_service_status{service}`
- `i2pd_webconsole_exporter_version_info{version}`

---

## systemd unit (example)

```ini
[Unit]
Description=I2Pd Web Metrics Exporter
Requires=i2pd.service
After=i2pd.service

[Service]
Type=simple
ExecStart=/usr/local/bin/i2pd-webconsole-exporter
Environment="I2PD_WEB_CONSOLE=http://127.0.0.1:7070"
Environment="METRICS_LISTEN_ADDR=0.0.0.0:9447"
Environment="RUST_LOG=info"
Restart=on-failure
RestartSec=10
User=i2pd
Group=i2pd

[Install]
WantedBy=multi-user.target
```

Enable and launch:

```bash
sudo systemctl enable i2pd-webconsole-exporter.service
sudo systemctl start i2pd-webconsole-exporter.service
sudo systemctl status i2pd-webconsole-exporter.service
```
