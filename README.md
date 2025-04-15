# i2pd-webconsole-exporter

A Prometheus exporter for i2pd, written in Rust.

**WARNING:** This exporter works by **scraping the HTML content** of the i2pd web console. This method is inherently fragile and may break if the structure or content of the web console changes in future i2pd versions.

## Features

- Collects metrics by parsing the i2pd web console HTML page.
- Exposes metrics in Prometheus format on port 9700 (configurable).
- Low memory footprint and efficient performance.
- Metrics include network status, tunnel success rate, data transfer stats, router capabilities, network counts, and service statuses.

## Building

The primary way to build the exporter is using the standard Rust toolchain:

```bash
cargo build --release
```

The resulting binary will be located at `target/release/i2pd-webconsole-exporter`.

### Building Static Linux Binary (via Docker)

For convenience, a script is provided to build a static Linux binary suitable for deployment on `x86_64-unknown-linux-gnu` targets using Docker:

1.  Ensure Docker is installed and running.
2.  Run the build script:

```bash
./build-static-linux-docker.sh
```

This script builds the binary using a Docker container and copies the compiled static binary to the `./dist/` directory within this project.

## Configuration

The exporter is configured through environment variables:

- `I2PD_WEB_CONSOLE`: URL of the i2pd web console (default: "http://127.0.0.1:7070")
- `METRICS_LISTEN_ADDR`: Address and port to expose metrics on (default: "0.0.0.0:9700")
  - Note: When deployed via Ansible, this is configured to use port 9447
- `HTTP_TIMEOUT_SECONDS`: Timeout for HTTP requests in seconds (default: 60)

## Metrics

The exporter provides the following metrics (parsed from HTML):

- `i2p_network_status_v4{status="..."}`: IPv4 network status (1=OK, 0=Other)
- `i2p_network_status_v6{status="..."}`: IPv6 network status (1=OK, 0=Other)
- `i2p_tunnel_creation_success_rate`: Percentage of successful tunnel creations
- `i2p_data_received_bytes`: Total data received in bytes (counter)
- `i2p_data_sent_bytes`: Total data sent in bytes (counter)
- `i2p_data_transit_bytes`: Total transit data in bytes (counter)
- `i2p_data_rate_bytes_per_second{direction="received|sent|transit"}`: Data transfer rate
- `i2p_router_capabilities{capabilities="..."}`: Router capabilities string (gauge=1)
- `i2p_external_address{protocol="...", address="..."}`: External addresses (gauge=1)
- `i2p_network_routers`: Count of routers in the network
- `i2p_network_floodfills`: Count of floodfill routers in the network
- `i2p_network_leasesets`: Count of leasesets in the network
- `i2p_client_tunnels`: Count of client tunnels
- `i2p_transit_tunnels`: Count of transit tunnels
- `i2p_service_status{service="..."}`: Status of i2pd services (1=enabled, 0=disabled)
- `i2pd_webconsole_exporter_version_info{version="..."}`: Version of the exporter itself

## Deployment

The compiled binary can be found in the `./dist/` directory after running the `build-static-linux-docker.sh` script, or in `target/release/` after a native build.

Deployment instructions depend on your specific environment. You will typically need to:

1. Copy the binary to your target server (e.g., `/usr/local/bin/i2pd-webconsole-exporter`).
2. Configure it to run as a service (e.g., using systemd).
3. Provide the necessary environment variables for configuration (see Configuration section above).

### Systemd Service Example

Here is an example systemd service file (`/etc/systemd/system/i2pd-webconsole-exporter.service`). Adjust paths, user, group, and environment variables as needed.

```ini
[Unit]
Description=I2Pd Web Metrics Exporter
# Ensure i2pd is started before this service
After=i2pd.service
Requires=i2pd.service

[Service]
Type=simple
# Optional: Add startup delay to ensure i2pd is fully initialized
# ExecStartPre=/bin/sleep 3
ExecStart=/usr/local/bin/i2pd-webconsole-exporter
# Adjust these environment variables according to your i2pd setup
Environment="I2PD_WEB_CONSOLE=http://127.0.0.1:7070"
Environment="METRICS_LISTEN_ADDR=0.0.0.0:9447"
Environment="RUST_LOG=info"
# Improved restart policy with backoff
Restart=on-failure
RestartSec=10
# Run as the i2pd user (or another dedicated user)
User=i2pd
Group=i2pd

[Install]
WantedBy=multi-user.target
```

After creating the file, enable and start the service:

```bash
sudo systemctl enable i2pd-webconsole-exporter.service
sudo systemctl start i2pd-webconsole-exporter.service
sudo systemctl status i2pd-webconsole-exporter.service
```
