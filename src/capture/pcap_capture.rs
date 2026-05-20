use anyhow::{Context, Result};

pub fn list_interfaces() -> Result<Vec<String>> {
    Ok(pcap::Device::list()?
        .into_iter()
        .map(|d| d.name)
        .collect::<Vec<_>>())
}

pub fn pick_interface(configured: Option<String>) -> Result<String> {
    if let Some(name) = configured {
        return Ok(name);
    }
    let devices = pcap::Device::list()?;
    let best = devices
        .iter()
        .find(|d| !d.flags.is_loopback())
        .or_else(|| devices.first())
        .context("no capture interfaces found")?;
    Ok(best.name.clone())
}

pub fn capture_loop<F>(interface: &str, mut on_packet: F) -> Result<()>
where
    F: FnMut(&[u8]),
{
    let mut cap = pcap::Capture::from_device(interface)
        .with_context(|| format!("interface {interface} not found"))?
        .promisc(true)
        .immediate_mode(true)
        .open()?;

    cap.filter("udp", true)?;

    loop {
        let packet = cap.next_packet()?;
        if packet.data.is_empty() {
            continue;
        }
        on_packet(packet.data);
    }
}
