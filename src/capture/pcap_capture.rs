use anyhow::{Context, Result};

pub fn list_interfaces() -> Result<Vec<String>> {
    Ok(pcap::Device::list()?
        .into_iter()
        .map(|d| d.name)
        .collect::<Vec<_>>())
}

pub fn list_non_loopback_interfaces() -> Result<Vec<String>> {
    Ok(pcap::Device::list()?
        .into_iter()
        .filter(|d| !d.flags.is_loopback())
        .map(|d| d.name)
        .collect::<Vec<_>>())
}

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

pub fn open_capture_handle(interface: &str) -> Result<pcap::Capture<pcap::Active>> {
    let mut cap = pcap::Capture::from_device(interface)
        .with_context(|| format!("interface {interface} not found"))?
        .promisc(true)
        .immediate_mode(true)
        .open()?;
    cap.filter("udp", true)?;
    Ok(cap)
}

pub fn spawn_capture_thread<F>(interface: String, mut on_packet: F) -> std::thread::JoinHandle<()>
where
    F: FnMut(&str, &[u8]) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut cap = match open_capture_handle(&interface) {
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
