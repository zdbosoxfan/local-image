//! Opt-in, bounded LAN discovery. Only read ComfyUI's API; never queue work or upload images.
use std::net::{Ipv4Addr, SocketAddrV4};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

const WORKERS: usize = 16;
const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub address: Ipv4Addr,
    pub port: u16,
    pub device: String,
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub running: bool,
    pub checked: usize,
    pub total: usize,
    pub networks: Vec<String>,
    pub servers: Vec<Server>,
    pub error: Option<String>,
}

pub struct Scan {
    state: Arc<Mutex<Snapshot>>,
    cancelled: Arc<AtomicBool>,
}

impl Scan {
    /// Discovery starts only when requested. The configured port and default 8188 are checked.
    pub fn start(port: u16) -> std::io::Result<Self> {
        let scan = Self { state: Arc::new(Mutex::new(Snapshot { running: true, ..Default::default() })), cancelled: Arc::new(AtomicBool::new(false)) };
        let state = scan.state.clone();
        let cancelled = scan.cancelled.clone();
        std::thread::Builder::new().name("comfy-lan-discovery".into()).spawn(move || {
            lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
            let result = local_networks().map(|networks| {
                let targets = targets(&networks, port);
                if let Ok(mut s) = state.lock() {
                    s.total = targets.len();
                    s.networks = networks.iter().map(|(ip, prefix)| format!("{ip}/{prefix}")).collect();
                }
                scan_targets(&targets, &state, &cancelled, DEADLINE);
            });
            if let Ok(mut s) = state.lock() {
                s.running = false;
                if let Err(error) = result {
                    s.error = Some(error);
                }
            }
        })?;
        Ok(scan)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn command(program: &str, args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Reading the local network timed out".into());
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Could not read the local network".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn local_networks() -> Result<Vec<(Ipv4Addr, u8)>, String> {
    #[cfg(target_os = "macos")]
    let networks = parse_ifconfig(&command("/sbin/ifconfig", &[])?);
    #[cfg(target_os = "linux")]
    let networks = parse_ip(&command("ip", &["-o", "-4", "addr", "show", "up"])?);
    #[cfg(target_os = "windows")]
    let networks = parse_windows(&command(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-NetIPConfiguration | Where-Object {$_.NetAdapter.Status -eq 'Up' -and $_.IPv4DefaultGateway} | ForEach-Object {$_.IPv4Address | Select-Object IPAddress,PrefixLength} | ConvertTo-Json -Compress",
        ],
    )?);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let networks = Vec::new();
    let networks = bounded_networks(networks);
    if networks.is_empty() {
        return Err("No active private IPv4 LAN found. Enter the server address manually.".into());
    }
    Ok(networks)
}

fn bounded_networks(networks: Vec<(Ipv4Addr, u8)>) -> Vec<(Ipv4Addr, u8)> {
    let mut result = Vec::new();
    for (ip, prefix) in networks {
        if !ip.is_private() || prefix > 30 {
            continue;
        }
        // On a large LAN search the adjacent /24, never expand to tens of thousands of hosts.
        let prefix = prefix.max(24);
        let network = Ipv4Addr::from(u32::from(ip) & (u32::MAX << (32 - prefix)));
        if !result.contains(&(network, prefix)) {
            result.push((network, prefix));
        }
        if result.len() == 2 {
            break;
        }
    }
    result
}

#[cfg(any(target_os = "macos", test))]
fn parse_ifconfig(text: &str) -> Vec<(Ipv4Addr, u8)> {
    let mut result = Vec::new();
    let mut addresses = Vec::new();
    let mut active = false;
    let mut ethernet = false;
    for line in text.lines().chain(std::iter::once("end:")) {
        if !line.starts_with(char::is_whitespace) {
            if active && ethernet {
                result.append(&mut addresses);
            }
            addresses.clear();
            active = false;
            ethernet = false;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first() == Some(&"ether") {
            ethernet = true;
        }
        if fields.as_slice() == ["status:", "active"] {
            active = true;
        }
        if fields.first() == Some(&"inet")
            && let (Some(ip), Some(mask)) = (fields.get(1).and_then(|v| v.parse().ok()), fields.get(3))
        {
            let mask = mask.trim_start_matches("0x");
            if let Ok(mask) = u32::from_str_radix(mask, 16) {
                let prefix = mask.leading_ones() as u8;
                if mask == u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0) {
                    addresses.push((ip, prefix));
                }
            }
        }
    }
    result
}

#[cfg(any(target_os = "linux", test))]
fn parse_ip(text: &str) -> Vec<(Ipv4Addr, u8)> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            let at = fields.iter().position(|v| *v == "inet")?;
            let (ip, prefix) = fields.get(at + 1)?.split_once('/')?;
            Some((ip.parse().ok()?, prefix.parse().ok()?))
        })
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows(text: &str) -> Vec<(Ipv4Addr, u8)> {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let rows = match value {
        Value::Array(rows) => rows,
        row => vec![row],
    };
    rows.iter().filter_map(|row| Some((row.get("IPAddress")?.as_str()?.parse().ok()?, row.get("PrefixLength")?.as_u64()?.try_into().ok()?))).collect()
}

fn targets(networks: &[(Ipv4Addr, u8)], port: u16) -> Vec<SocketAddrV4> {
    networks
        .iter()
        .flat_map(|(ip, prefix)| {
            let start = u32::from(*ip);
            let count = 1u32 << (32 - prefix);
            (1..count - 1).flat_map(move |offset| {
                [8188, port]
                    .into_iter()
                    .enumerate()
                    .filter(move |(i, p)| *p != 0 && (*i == 0 || port != 8188))
                    .map(move |(_, p)| SocketAddrV4::new(Ipv4Addr::from(start + offset), p))
            })
        })
        .collect()
}

fn probe(agent: &ureq::Agent, address: SocketAddrV4) -> Option<Server> {
    let mut response = agent.get(format!("http://{address}/system_stats")).call().ok()?;
    if response.status() != 200 {
        return None;
    }
    let stats: Value = response.body_mut().with_config().limit(65536).read_json().ok()?;
    stats.get("system")?.get("comfyui_version")?.as_str()?;
    let devices = stats.get("devices")?.as_array()?;
    let device = devices.iter().filter_map(|v| v.get("name").and_then(Value::as_str)).collect::<Vec<_>>().join(", ");
    Some(Server { address: *address.ip(), port: address.port(), device })
}

fn scan_targets(targets: &[SocketAddrV4], state: &Mutex<Snapshot>, cancelled: &AtomicBool, limit: Duration) {
    let next = AtomicUsize::new(0);
    let started = Instant::now();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_millis(400)))
        .timeout_global(Some(Duration::from_millis(900)))
        .build()
        .into();
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(targets.len()) {
            let agent = agent.clone();
            let next = &next;
            scope.spawn(move || {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                while !cancelled.load(Ordering::Relaxed) && started.elapsed() < limit {
                    let Some(address) = targets.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    let found = probe(&agent, *address);
                    if let Ok(mut s) = state.lock() {
                        s.checked += 1;
                        if let Some(server) = found {
                            s.servers.push(server);
                            s.servers.sort_by_key(|s| (s.address, s.port));
                        }
                    }
                }
            });
        }
    });
    if let Ok(mut s) = state.lock() {
        s.error = if cancelled.load(Ordering::Relaxed) {
            Some("Search cancelled".into())
        } else if s.checked < targets.len() {
            Some("Search time limit reached. Enter another server address manually.".into())
        } else {
            None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subnet_selection_deduplicates_and_caps_multiple_interfaces() {
        let networks = bounded_networks(vec![
            ("10.0.1.2".parse().unwrap(), 24),
            ("10.0.1.3".parse().unwrap(), 24),
            ("192.168.1.2".parse().unwrap(), 28),
            ("172.16.1.2".parse().unwrap(), 24),
        ]);
        assert_eq!(networks.len(), 2);
        assert_eq!(targets(&networks, 8188).len(), 254 + 14);
    }

    #[test]
    fn scan_collects_api_results_and_finishes_every_target() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let std::net::SocketAddr::V4(address) = server.server_addr().to_ip().unwrap() else {
            panic!("IPv4 fixture");
        };
        let fixture = std::thread::spawn(move || {
            for _ in 0..32 {
                let request = server.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
                assert_eq!(request.method(), &tiny_http::Method::Get);
                assert_eq!(request.url(), "/system_stats");
                request.respond(tiny_http::Response::from_string(r#"{"system":{"comfyui_version":"0.3"},"devices":[{"name":"Fixture GPU"}]}"#)).unwrap();
            }
        });
        let state = Mutex::new(Snapshot::default());
        scan_targets(&[address; 32], &state, &AtomicBool::new(false), DEADLINE);
        let state = state.lock().unwrap();
        assert_eq!(state.checked, 32);
        assert_eq!(state.servers.len(), 32);
        assert!(state.servers.iter().all(|s| s.device == "Fixture GPU"));
        assert!(state.error.is_none());
        fixture.join().unwrap();
    }

    #[test]
    #[ignore = "opt-in scan of the current private LAN; never run during normal tests"]
    fn native_lan_search_finishes_bounded() {
        let scan = Scan::start(8188).unwrap();
        let started = Instant::now();
        while scan.snapshot().running && started.elapsed() < Duration::from_secs(35) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let result = scan.snapshot();
        eprintln!("LAN networks={:?}, checked={}/{}, found={:?}, status={:?}", result.networks, result.checked, result.total, result.servers, result.error);
        assert!(!result.running);
        assert!(result.total > 0, "{}", result.error.unwrap_or_default());
        assert!(result.total <= 1016);
    }
    #[test]
    fn subnet_and_port_limits_exclude_public_loopback_network_and_broadcast() {
        let networks = bounded_networks(vec![(Ipv4Addr::LOCALHOST, 8), (Ipv4Addr::new(8, 8, 8, 8), 24), (Ipv4Addr::new(192, 168, 0, 119), 16)]);
        assert_eq!(networks, vec![(Ipv4Addr::new(192, 168, 0, 0), 24)]);
        let hosts = targets(&networks, 8188);
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], "192.168.0.1:8188".parse().unwrap());
        assert_eq!(hosts[253], "192.168.0.254:8188".parse().unwrap());
        assert_eq!(targets(&networks, 8000).len(), 508);
        assert_eq!(bounded_networks(vec![(Ipv4Addr::new(10, 0, 0, 1), 31)]), vec![]);
    }
    #[test]
    fn native_network_formats_and_inactive_interfaces() {
        let mac = "en0: flags\n\tether ab:cd\n\tinet 192.168.0.119 netmask 0xffffff00 broadcast 192.168.0.255\n\tstatus: active\nen1: flags\n\tether ab:cd\n\tinet 192.168.1.2 netmask 0xffffff00\n\tstatus: inactive\nutun0: flags\n\tinet 10.2.0.1 netmask 0xffffff00\n\tstatus: active\n";
        assert_eq!(parse_ifconfig(mac), vec![(Ipv4Addr::new(192, 168, 0, 119), 24)]);
        assert_eq!(parse_ip("2: eth0 inet 10.1.2.3/23 brd 10.1.3.255 scope global"), vec![(Ipv4Addr::new(10, 1, 2, 3), 23)]);
        assert_eq!(parse_windows(r#"{"IPAddress":"172.16.0.2","PrefixLength":24}"#), vec![(Ipv4Addr::new(172, 16, 0, 2), 24)]);
    }
    #[test]
    fn cancellation_does_not_probe_hosts() {
        let state = Mutex::new(Snapshot::default());
        scan_targets(&["127.0.0.1:1".parse().unwrap()], &state, &AtomicBool::new(true), DEADLINE);
        let state = state.lock().unwrap();
        assert_eq!(state.checked, 0);
        assert_eq!(state.error.as_deref(), Some("Search cancelled"));
    }

    #[test]
    fn deadline_never_reports_unchecked_hosts_as_complete() {
        let state = Mutex::new(Snapshot::default());
        scan_targets(&["127.0.0.1:1".parse().unwrap()], &state, &AtomicBool::new(false), Duration::ZERO);
        let state = state.lock().unwrap();
        assert_eq!(state.checked, 0);
        assert!(state.error.as_deref().unwrap().contains("time limit"));
    }
    #[test]
    fn verifies_comfy_api_and_rejects_unrelated_http() {
        for (body, expected) in [(r#"{"system":{"comfyui_version":"0.3.0"},"devices":[{"name":"Test GPU"}]}"#, true), (r#"{"status":"ok"}"#, false)] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let address = server.server_addr().to_ip().unwrap();
            let thread = std::thread::spawn(move || {
                let request = server.recv().unwrap();
                assert_eq!(request.url(), "/system_stats");
                request.respond(tiny_http::Response::from_string(body)).unwrap();
            });
            let agent: ureq::Agent = ureq::Agent::config_builder().proxy(None).build().into();
            let std::net::SocketAddr::V4(address) = address else {
                panic!("IPv4 fixture");
            };
            assert_eq!(probe(&agent, address).is_some(), expected);
            thread.join().unwrap();
        }
    }
}
