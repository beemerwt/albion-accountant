use std::path::Path;

use anyhow::{Result, bail};
#[cfg(feature = "pcap")]
use anyhow::Context;
#[cfg(feature = "pcap")]
use tracing::info;
use tracing::warn;

use crate::{albion::hosts, config::FilterMode};

fn format_host_filter_term(host: &str) -> String {
    if let Some((addr, prefix)) = host.split_once('/') {
        if let (Ok(ip), Ok(prefix_len)) = (addr.parse::<std::net::IpAddr>(), prefix.parse::<u8>()) {
            let max_prefix = match ip {
                std::net::IpAddr::V4(_) => 32,
                std::net::IpAddr::V6(_) => 128,
            };
            if prefix_len <= max_prefix {
                return format!("net {addr}/{prefix_len}");
            }
        }
    }
    format!("host {host}")
}

pub fn build_filter_expression(
    mode: FilterMode,
    bpf_override: Option<&str>,
    albion_hosts_file: Option<&Path>,
    albion_port_expr: Option<&str>,
) -> String {
    if let Some(expr) = bpf_override {
        return expr.to_string();
    }

    match mode {
        FilterMode::Broad => "udp".to_string(),
        FilterMode::Custom => {
            warn!("custom filter mode selected without --bpf; falling back to broad udp filter");
            "udp".to_string()
        }
        FilterMode::Albion => {
            let hosts = match hosts::load_hosts(albion_hosts_file) {
                Ok(hosts) if !hosts.is_empty() => hosts,
                Ok(_) => {
                    warn!("Albion hosts list is empty; falling back to broad udp filter");
                    return "udp".to_string();
                }
                Err(err) => {
                    warn!(error = %err, "Albion hosts list unavailable; falling back to broad udp filter");
                    return "udp".to_string();
                }
            };
            let hosts_expr = hosts
                .into_iter()
                .map(|host| format_host_filter_term(&host))
                .collect::<Vec<_>>()
                .join(" or ");
            let port_expr = albion_port_expr.unwrap_or("port 5056 or port 5057");
            format!("({hosts_expr}) and ({port_expr})")
        }
    }
}

#[cfg(feature = "pcap")]
pub fn list_interfaces() -> Result<Vec<String>> {
    Ok(pcap::Device::list()?
        .into_iter()
        .map(|d| d.name)
        .collect::<Vec<_>>())
}

#[cfg(not(feature = "pcap"))]
pub fn list_interfaces() -> Result<Vec<String>> {
    bail!("pcap support is disabled at compile time")
}

#[cfg(feature = "pcap")]
pub fn list_non_loopback_interfaces() -> Result<Vec<String>> {
    Ok(pcap::Device::list()?
        .into_iter()
        .filter(|d| !d.flags.is_loopback())
        .map(|d| d.name)
        .collect::<Vec<_>>())
}

#[cfg(not(feature = "pcap"))]
pub fn list_non_loopback_interfaces() -> Result<Vec<String>> {
    bail!("pcap support is disabled at compile time")
}

#[cfg(feature = "pcap")]
pub fn pick_interface(configured: Vec<String>) -> Result<String> {
    if let Some(name) = configured.first() {
        return Ok(name.clone());
    }
    let devices = pcap::Device::list()?;
    let best = devices
        .iter()
        .find(|d| !d.flags.is_loopback())
        .or_else(|| devices.first())
        .context("no capture interfaces found")?;
    Ok(best.name.clone())
}

#[cfg(not(feature = "pcap"))]
pub fn pick_interface(_configured: Vec<String>) -> Result<String> {
    bail!("pcap support is disabled at compile time")
}

#[cfg(feature = "pcap")]
pub fn open_capture_file(path: &Path) -> Result<pcap::Capture<pcap::Offline>> {
    pcap::Capture::from_file(path)
        .with_context(|| format!("failed to open capture file {}", path.display()))
}

#[cfg(not(feature = "pcap"))]
pub struct OfflineCaptureStub;
#[cfg(not(feature = "pcap"))]
pub struct ActiveCaptureStub;
#[cfg(not(feature = "pcap"))]
pub struct CapturePacketStub {
    pub data: &'static [u8],
}
#[cfg(not(feature = "pcap"))]
pub struct DataLinkStub(pub i32);
#[cfg(not(feature = "pcap"))]
impl OfflineCaptureStub {
    pub fn get_datalink(&self) -> DataLinkStub {
        DataLinkStub(1)
    }
    pub fn next_packet(&mut self) -> Result<CapturePacketStub> {
        bail!("pcap support is disabled at compile time")
    }
}
#[cfg(not(feature = "pcap"))]
impl ActiveCaptureStub {
    pub fn get_datalink(&self) -> DataLinkStub {
        DataLinkStub(1)
    }
    pub fn next_packet(&mut self) -> Result<CapturePacketStub> {
        bail!("pcap support is disabled at compile time")
    }
}
#[cfg(not(feature = "pcap"))]
pub fn open_capture_file(_path: &Path) -> Result<OfflineCaptureStub> {
    bail!("pcap support is disabled at compile time")
}

#[cfg(feature = "pcap")]
pub fn open_capture_handle(
    interface: &str,
    filter_expr: &str,
) -> Result<pcap::Capture<pcap::Active>> {
    let mut cap = pcap::Capture::from_device(interface)
        .with_context(|| format!("interface {interface} not found"))?
        .promisc(true)
        .immediate_mode(true)
        .open()?;
    cap.filter(filter_expr, true)?;
    info!(interface = %interface, filter = %filter_expr, "pcap filter applied");
    Ok(cap)
}

#[cfg(not(feature = "pcap"))]
pub fn open_capture_handle(_interface: &str, _filter_expr: &str) -> Result<ActiveCaptureStub> {
    bail!("pcap support is disabled at compile time")
}

#[cfg(feature = "pcap")]
pub fn spawn_capture_thread<F>(
    interface: String,
    filter_expr: String,
    mut on_packet: F,
) -> std::thread::JoinHandle<()>
where
    F: FnMut(&str, &[u8]) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut cap = match open_capture_handle(&interface, &filter_expr) {
            Ok(cap) => cap,
            Err(_) => return,
        };
        loop {
            match cap.next_packet() {
                Ok(packet) => {
                    if packet.data.is_empty() {
                        continue;
                    }
                    on_packet(&interface, packet.data);
                }
                Err(_) => return,
            }
        }
    })
}
