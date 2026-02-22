use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser; // Added for CLI argument parsing
use log::{debug, error, info};
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;
use tokio::signal;
use warp::Filter;

// --- CLI Arguments ---

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)] // Automatically uses version from Cargo.toml
struct Cli {}

// -------------------------------------------------------------------------
// Pre‑compiled regular expressions – created once at startup
// -------------------------------------------------------------------------
static IPV4_STATUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Network status:</b> ([^<]+)").unwrap());
static IPV6_STATUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Network status v6:</b> ([^<]+)").unwrap());
static TUNNEL_CREATION_RATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Tunnel creation success rate:</b> (\d+)%").unwrap());
static DATA_SIZE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+\.\d+|\d+)\s*([KMGT]iB|B)").unwrap());
static DATA_RATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\d+\.\d+|\d+)\s*([KMGT]iB/s|B/s)").unwrap());
static RECEIVED_BYTES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Received:</b> ([^<]+)<br>").unwrap());
static SENT_BYTES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<b>Sent:</b> ([^<]+)<br>").unwrap());
static TRANSIT_BYTES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Transit:</b> ([^<]+)<br>").unwrap());
static ROUTER_CAPS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<b>Router Caps:</b> ([A-Za-z0-9~]+)<br>").unwrap());
static EXT_ADDR_ROW_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<tr>\s*<td>([^<]+)</td>\s*<td>([^<]+)</td>\s*</tr>").unwrap());
static NET_COUNTS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<b>Routers:</b> (\d+) <b>Floodfills:</b> (\d+) <b>LeaseSets:</b> (\d+)").unwrap()
});
static TUNNEL_COUNTS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<b>Client Tunnels:</b> (\d+) <b>Transit Tunnels:</b> (\d+)").unwrap()
});
static SERVICE_ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<tr><td>([^<]+)</td><td class='(enabled|disabled)'>([^<]+)</td></tr>").unwrap()
});
// -------------------------------------------------------------------------

// Struct to hold parsed data metrics
#[derive(Debug, Default)]
struct DataMetrics {
    received_bytes: Option<u64>,
    sent_bytes: Option<u64>,
    transit_bytes: Option<u64>,
    received_rate: Option<f64>,
    sent_rate: Option<f64>,
    transit_rate: Option<f64>,
}

// Application state
struct AppState {
    web_client: reqwest::Client,
    web_console_url: String,
}

impl AppState {
    // --- HTML Parsing Functions (using Regex) ---
    // WARNING: HTML scraping is fragile and might break with i2pd updates.

    fn new(web_client: reqwest::Client, web_console_url: String) -> Self {
        AppState {
            web_client,
            web_console_url,
        }
    }

    // Parse network status for IPv4 and IPv6
    fn parse_network_status(&self, html: &str) -> (Option<String>, Option<String>) {
        let v4 = IPV4_STATUS_RE
            .captures(html)
            .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()));
        let v6 = IPV6_STATUS_RE
            .captures(html)
            .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()));
        (v4, v6)
    }

    // Parse tunnel creation success rate
    fn parse_tunnel_creation_rate(&self, html: &str) -> Option<f64> {
        TUNNEL_CREATION_RATE_RE
            .captures(html)
            .and_then(|c| c.get(1)?.as_str().parse::<f64>().ok())
    }

    // Parses data sizes like "1.23 GiB" or "500 MiB" into bytes (u64).
    fn parse_data_size(&self, s: &str) -> Option<u64> {
        let caps = DATA_SIZE_RE.captures(s)?;
        let value: f64 = caps[1].parse().ok()?;
        let mult = match &caps[2] {
            "B" => 1,
            "KiB" => 1024,
            "MiB" => 1024 * 1024,
            "GiB" => 1024 * 1024 * 1024,
            "TiB" => 1024_u64.pow(4),
            _ => return None,
        };
        Some((value * mult as f64) as u64)
    }

    // Parses data rates like "100.5 KiB/s" into bytes/second (f64).
    fn parse_data_rate(&self, rate_str: &str) -> Option<f64> {
        let caps = DATA_RATE_RE.captures(rate_str)?;
        let value: f64 = caps[1].parse().ok()?;
        let mult = match &caps[2] {
            "B/s" => 1.0,
            "KiB/s" => 1024.0,
            "MiB/s" => 1024.0 * 1024.0,
            "GiB/s" => 1024.0 * 1024.0 * 1024.0,
            "TiB/s" => 1024.0_f64.powi(4), // Use powi for integer exponent
            _ => return None,
        };
        Some(value * mult)
    }

    // Parse received, sent and transit data
    fn parse_data_metrics(&self, html: &str) -> DataMetrics {
        let mut metrics = DataMetrics::default();

        let received_str = RECEIVED_BYTES_RE
            .captures(html)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
        let sent_str = SENT_BYTES_RE
            .captures(html)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
        let transit_str = TRANSIT_BYTES_RE
            .captures(html)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));

        let (received_bytes, received_rate) = if let Some(s) = received_str {
            let parts: Vec<&str> = s.split(" (").collect();
            let total = if !parts.is_empty() {
                self.parse_data_size(parts[0])
            } else {
                None
            };
            let rate = if parts.len() > 1 {
                let rate_part = parts[1].trim_end_matches(')');
                self.parse_data_rate(rate_part)
            } else {
                None
            };
            (total, rate)
        } else {
            (None, None)
        };

        let (sent_bytes, sent_rate) = if let Some(s) = sent_str {
            let parts: Vec<&str> = s.split(" (").collect();
            let total = if !parts.is_empty() {
                self.parse_data_size(parts[0])
            } else {
                None
            };
            let rate = if parts.len() > 1 {
                let rate_part = parts[1].trim_end_matches(')');
                self.parse_data_rate(rate_part)
            } else {
                None
            };
            (total, rate)
        } else {
            (None, None)
        };

        let (transit_bytes, transit_rate) = if let Some(s) = transit_str {
            let parts: Vec<&str> = s.split(" (").collect();
            let total = if !parts.is_empty() {
                self.parse_data_size(parts[0])
            } else {
                None
            };
            let rate = if parts.len() > 1 {
                let rate_part = parts[1].trim_end_matches(')');
                self.parse_data_rate(rate_part)
            } else {
                None
            };
            (total, rate)
        } else {
            (None, None)
        };

        metrics.received_bytes = received_bytes;
        metrics.received_rate = received_rate;
        metrics.sent_bytes = sent_bytes;
        metrics.sent_rate = sent_rate;
        metrics.transit_bytes = transit_bytes;
        metrics.transit_rate = transit_rate;

        metrics
    }

    // Parse router capabilities
    fn parse_router_capabilities(&self, html: &str) -> Option<String> {
        ROUTER_CAPS_RE
            .captures(html)
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    // Parse external addresses
    fn parse_external_addresses(&self, html: &str) -> Vec<(String, String)> {
        let mut addresses = Vec::new();

        if let Some(start_idx) = html.find("<b>Our external address:</b>") {
            if let Some(table_start) = html[start_idx..].find("<table class=\"extaddr\">") {
                if let Some(table_end) = html[start_idx + table_start..].find("</table>") {
                    let table_html =
                        &html[start_idx + table_start..(start_idx + table_start + table_end + 8)];

                    for cap in EXT_ADDR_ROW_RE.captures_iter(table_html) {
                        if let (Some(protocol), Some(address)) = (cap.get(1), cap.get(2)) {
                            addresses.push((
                                protocol.as_str().to_string(),
                                address.as_str().to_string(),
                            ));
                        }
                    }
                }
            }
        }

        addresses
    }

    // Parse network counts (routers, floodfills, leasesets)
    fn parse_network_counts(&self, html: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
        if let Some(caps) = NET_COUNTS_RE.captures(html) {
            let routers = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok());
            let floodfills = caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
            let leasesets = caps.get(3).and_then(|m| m.as_str().parse::<u64>().ok());
            return (routers, floodfills, leasesets);
        }
        (None, None, None)
    }

    // Parse tunnel counts (client and transit)
    fn parse_tunnel_counts(&self, html: &str) -> (Option<u64>, Option<u64>) {
        if let Some(caps) = TUNNEL_COUNTS_RE.captures(html) {
            let client = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok());
            let transit = caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
            return (client, transit);
        }
        (None, None)
    }

    // Parse service statuses
    fn parse_service_statuses(&self, html: &str) -> HashMap<String, bool> {
        let mut services = HashMap::new();

        if let Some(table_start) = html.find("<table class=\"services\">") {
            if let Some(table_end) = html[table_start..].find("</table>") {
                let table_html = &html[table_start..(table_start + table_end + 8)];

                for cap in SERVICE_ROW_RE.captures_iter(table_html) {
                    if let (Some(service), Some(status_class)) = (cap.get(1), cap.get(2)) {
                        let is_enabled = status_class.as_str() == "enabled";
                        let service_name = service.as_str().to_lowercase().replace(" ", "_");
                        services.insert(service_name, is_enabled);
                    }
                }
            }
        }

        services
    }

    // --- Main Metrics Fetching Logic ---

    // Fetches the web console HTML, calls parsing functions, and formats metrics for Prometheus.
    async fn fetch_metrics(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Fetch the HTML content from the configured URL
        let uri = &self.web_console_url;
        debug!("Fetching web console from: {}", uri);

        let response = self
            .web_client
            .get(uri)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to fetch HTML content: HTTP {}", response.status()).into());
        }

        let html = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        // Build metrics output
        let mut output = String::with_capacity(2048);

        // Parse network status
        let (ipv4_status, ipv6_status) = self.parse_network_status(&html);
        if let Some(status) = ipv4_status {
            output += "# HELP i2p_network_status_v4 IPv4 network status as string\n";
            output += "# TYPE i2p_network_status_v4 gauge\n";
            let status_value = if status == "OK" { 1 } else { 0 };
            output += &format!(
                "i2p_network_status_v4{{status=\"{}\"}} {}\n",
                status, status_value
            );
        }
        if let Some(status) = ipv6_status {
            output += "# HELP i2p_network_status_v6 IPv6 network status as string\n";
            output += "# TYPE i2p_network_status_v6 gauge\n";
            let status_value = if status == "OK" { 1 } else { 0 };
            output += &format!(
                "i2p_network_status_v6{{status=\"{}\"}} {}\n",
                status, status_value
            );
        }

        // Parse tunnel creation success rate
        if let Some(rate) = self.parse_tunnel_creation_rate(&html) {
            output += "# HELP i2p_tunnel_creation_success_rate Percentage of successful tunnel creations\n";
            output += "# TYPE i2p_tunnel_creation_success_rate gauge\n";
            output += &format!("i2p_tunnel_creation_success_rate {}\n", rate);
        }

        // Parse data metrics (received, sent, transit)
        let data_metrics = self.parse_data_metrics(&html);

        if let Some(bytes) = data_metrics.received_bytes {
            output += "# HELP i2p_data_received_bytes Total data received in bytes\n";
            output += "# TYPE i2p_data_received_bytes counter\n";
            output += &format!("i2p_data_received_bytes {}\n", bytes);
        }
        if let Some(bytes) = data_metrics.sent_bytes {
            output += "# HELP i2p_data_sent_bytes Total data sent in bytes\n";
            output += "# TYPE i2p_data_sent_bytes counter\n";
            output += &format!("i2p_data_sent_bytes {}\n", bytes);
        }
        if let Some(bytes) = data_metrics.transit_bytes {
            output += "# HELP i2p_data_transit_bytes Total transit data in bytes\n";
            output += "# TYPE i2p_data_transit_bytes counter\n";
            output += &format!("i2p_data_transit_bytes {}\n", bytes);
        }

        // Add data rate metrics
        if data_metrics.received_rate.is_some()
            || data_metrics.sent_rate.is_some()
            || data_metrics.transit_rate.is_some()
        {
            output += "# HELP i2p_data_rate_bytes_per_second Data transfer rate in bytes/second\n";
            output += "# TYPE i2p_data_rate_bytes_per_second gauge\n";

            if let Some(rate) = data_metrics.received_rate {
                output += &format!(
                    "i2p_data_rate_bytes_per_second{{direction=\"received\"}} {}\n",
                    rate
                );
            }
            if let Some(rate) = data_metrics.sent_rate {
                output += &format!(
                    "i2p_data_rate_bytes_per_second{{direction=\"sent\"}} {}\n",
                    rate
                );
            }
            if let Some(rate) = data_metrics.transit_rate {
                output += &format!(
                    "i2p_data_rate_bytes_per_second{{direction=\"transit\"}} {}\n",
                    rate
                );
            }
        }

        // Parse router capabilities
        if let Some(caps) = self.parse_router_capabilities(&html) {
            output += "# HELP i2p_router_capabilities Router capabilities\n";
            output += "# TYPE i2p_router_capabilities gauge\n";
            output += &format!("i2p_router_capabilities{{capabilities=\"{}\"}} 1\n", caps);
        }

        // Parse external addresses
        let addresses = self.parse_external_addresses(&html);
        if !addresses.is_empty() {
            output += "# HELP i2p_external_address External addresses the router is reachable at\n";
            output += "# TYPE i2p_external_address gauge\n";
            for (protocol, address) in addresses {
                output += &format!(
                    "i2p_external_address{{protocol=\"{}\", address=\"{}\"}} 1\n",
                    protocol, address
                );
            }
        }

        // Parse network counts
        let network_counts = self.parse_network_counts(&html);
        if let Some(count) = network_counts.0 {
            output += "# HELP i2p_network_routers Count of routers in the network\n";
            output += "# TYPE i2p_network_routers gauge\n";
            output += &format!("i2p_network_routers {}\n", count);
        }
        if let Some(count) = network_counts.1 {
            output += "# HELP i2p_network_floodfills Count of floodfill routers in the network\n";
            output += "# TYPE i2p_network_floodfills gauge\n";
            output += &format!("i2p_network_floodfills {}\n", count);
        }
        if let Some(count) = network_counts.2 {
            output += "# HELP i2p_network_leasesets Count of leasesets in the network\n";
            output += "# TYPE i2p_network_leasesets gauge\n";
            output += &format!("i2p_network_leasesets {}\n", count);
        }

        // Parse tunnel counts
        let tunnel_counts = self.parse_tunnel_counts(&html);
        let client_tunnels = tunnel_counts.0;
        let transit_tunnels = tunnel_counts.1;

        if let Some(count) = client_tunnels {
            output += "# HELP i2p_client_tunnels Count of client tunnels\n";
            output += "# TYPE i2p_client_tunnels gauge\n";
            output += &format!("i2p_client_tunnels {}\n", count);
        }
        if let Some(count) = transit_tunnels {
            output += "# HELP i2p_transit_tunnels Count of transit tunnels\n";
            output += "# TYPE i2p_transit_tunnels gauge\n";
            output += &format!("i2p_transit_tunnels {}\n", count);
        }

        // Parse service statuses
        let services = self.parse_service_statuses(&html);
        if !services.is_empty() {
            output += "# HELP i2p_service_status Status of i2pd services (1=enabled, 0=disabled)\n";
            output += "# TYPE i2p_service_status gauge\n";
            for (service, enabled) in services {
                output += &format!(
                    "i2p_service_status{{service=\"{}\"}} {}\n",
                    service,
                    if enabled { 1 } else { 0 }
                );
            }
        }

        // Add exporter version info
        output +=
            "# HELP i2pd_webconsole_exporter_version_info I2P webconsole exporter version info\n";
        output += "# TYPE i2pd_webconsole_exporter_version_info gauge\n";
        output += &format!(
            "i2pd_webconsole_exporter_version_info{{version=\"{}\"}} 1\n",
            env!("CARGO_PKG_VERSION")
        );

        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments (handles --version automatically)
    let _cli = Cli::parse();

    env_logger::init();

    // Configuration from environment variables
    let web_console_url =
        std::env::var("I2PD_WEB_CONSOLE").unwrap_or_else(|_| "http://127.0.0.1:7070".to_string());
    let listen_addr =
        std::env::var("METRICS_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9700".to_string());
    let http_timeout = std::env::var("HTTP_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .unwrap_or(60);

    let listen_addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!(
        "Starting i2pd webconsole exporter on {} (target: {})",
        listen_addr, web_console_url
    );

    // Build HTTP client for web console
    let web_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(http_timeout))
        .build()?;

    let state = Arc::new(AppState::new(web_client, web_console_url));

    // Define a small async handler function for /metrics
    async fn metrics_handler(st: Arc<AppState>) -> Result<impl warp::Reply, warp::Rejection> {
        match st.fetch_metrics().await {
            Ok(metrics) => {
                let reply = warp::reply::with_status(metrics, warp::http::StatusCode::OK);
                let reply =
                    warp::reply::with_header(reply, "Content-Type", "text/plain; version=0.0.4");
                Ok(reply)
            }
            Err(err) => {
                error!("Failed to fetch metrics: {}", err);
                let error_body = "Error retrieving metrics".to_string();
                let reply = warp::reply::with_status(
                    error_body,
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                );
                let reply =
                    warp::reply::with_header(reply, "Content-Type", "text/plain; version=0.0.4");
                Ok(reply)
            }
        }
    }

    // Warp filter for GET /metrics
    let route_metrics = warp::path("metrics")
        .and(warp::any().map(move || state.clone()))
        .and_then(metrics_handler);

    // Fallback 404 for anything else
    let route_404 = warp::any()
        .map(|| warp::reply::with_status("Not Found", warp::http::StatusCode::NOT_FOUND));

    // Combine
    let routes = route_metrics.or(route_404);

    info!("Listening on http://{}", listen_addr);

    let server = warp::serve(routes).bind_with_graceful_shutdown(listen_addr, async {
        if let Err(e) = signal::ctrl_c().await {
            error!("Failed to listen for shutdown signal: {}", e);
        }
        info!("Shutdown signal received, shutting down...");
    });

    // The bind_with_graceful_shutdown function returns a tuple (SocketAddr, Future)
    // We only need to await the future part (the second element).
    let (_addr, future) = server;
    future.await;

    Ok(())
}
