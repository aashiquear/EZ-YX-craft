//! Find machines: serial ports and Wi-Fi controllers on the local network.

use crate::transport::Endpoint;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Serial ports likely to be CNC controllers (CH340/CP210x/FTDI/ACM).
pub fn serial_ports() -> Vec<Endpoint> {
    #[cfg(feature = "serial")]
    {
        crate::transport::SerialTransport::list().into_iter().map(|port| Endpoint::Serial { port, baud: 115_200 }).collect()
    }
    #[cfg(not(feature = "serial"))]
    {
        vec![]
    }
}

/// Ports commonly used by Wi-Fi GRBL bridges: 23 (telnet), 81 (ESP3D WebSocket), 8080/80 (web UI).
pub const WIFI_PORTS: [u16; 3] = [23, 81, 8080];

/// Scan `prefix.1 … prefix.254` (e.g. "192.168.1") for open controller ports.
/// Runs in parallel threads; ~1–2 s on a typical LAN.
pub fn scan_subnet(prefix: &str, timeout_ms: u64) -> Vec<Endpoint> {
    let prefix = prefix.trim_end_matches('.').to_string();
    let handles: Vec<_> = (1..=254u8)
        .map(|host| {
            let prefix = prefix.clone();
            std::thread::spawn(move || {
                let mut found = Vec::new();
                for port in WIFI_PORTS {
                    let ip = format!("{prefix}.{host}");
                    let Ok(addr) = format!("{ip}:{port}").parse::<SocketAddr>() else { continue };
                    if TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok() {
                        found.push(match port {
                            81 => Endpoint::WebSocket { url: format!("ws://{ip}:81/") },
                            _ => Endpoint::Tcp { host: ip, port },
                        });
                    }
                }
                found
            })
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
}

/// Check whether a TCP endpoint speaks GRBL: send `$I` and look for a GRBL-ish reply.
pub fn probe_grbl_tcp(host: &str, port: u16) -> Option<String> {
    let addr: SocketAddr = format!("{host}:{port}").parse().ok()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    s.write_all(b"\r\n$I\r\n").ok()?;
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).ok()?;
    let txt = String::from_utf8_lossy(&buf[..n]).to_string();
    (txt.contains("VER") || txt.contains("Grbl") || txt.contains("ok") || txt.contains("FluidNC")).then_some(txt)
}
