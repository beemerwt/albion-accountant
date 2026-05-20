use regex::Regex;

use super::{
    protocol::{
        commands::{PhotonMessage, decode_command_envelope},
        transport::parse_udp_payload,
    },
    transaction::MarketTransaction,
};

pub fn decode_packet(packet: &[u8]) -> Vec<PhotonMessage> {
    match parse_udp_payload(packet) {
        Ok(frames) => frames
            .into_iter()
            .filter_map(|f| decode_command_envelope(&f.body).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn extract_market_transactions(messages: &[PhotonMessage]) -> Vec<MarketTransaction> {
    let re = match Regex::new(
        r#"location=(?P<location>[A-Za-z_]+);item=(?P<item>[A-Z0-9_@.]+);qty=(?P<qty>\d+);price=(?P<price>\d+)"#,
    ) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    messages
        .iter()
        .filter_map(|m| std::str::from_utf8(&m.payload).ok())
        .filter_map(|text| re.captures(text))
        .filter_map(|captures| {
            let location = captures.name("location")?.as_str().to_string();
            let item = captures.name("item")?.as_str().to_string();
            let qty: u32 = captures.name("qty")?.as_str().parse().ok()?;
            let price: u64 = captures.name("price")?.as_str().parse().ok()?;
            MarketTransaction::new(location, item, qty, price, None).ok()
        })
        .collect()
}

pub fn decode_transaction(packet: &[u8]) -> Option<MarketTransaction> {
    let messages = decode_packet(packet);
    extract_market_transactions(&messages).into_iter().next()
}

pub fn extract_udp_payload_ipv4(packet: &[u8]) -> Option<(&[u8], std::net::IpAddr, u16, std::net::IpAddr, u16, u8)> {
    if packet.len() < 14 {
        return None;
    }
    let ether_type = u16::from_be_bytes([packet[12], packet[13]]);
    if ether_type != 0x0800 {
        return None;
    }
    let ip_start = 14usize;
    if packet.len() < ip_start + 20 {
        return None;
    }
    let ihl = (packet[ip_start] & 0x0f) as usize * 4;
    if ihl < 20 || packet.len() < ip_start + ihl {
        return None;
    }
    let proto = packet[ip_start + 9];
    if proto != 17 {
        return None;
    }
    let src_ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(
        packet[ip_start + 12],
        packet[ip_start + 13],
        packet[ip_start + 14],
        packet[ip_start + 15],
    ));
    let dst_ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(
        packet[ip_start + 16],
        packet[ip_start + 17],
        packet[ip_start + 18],
        packet[ip_start + 19],
    ));
    let udp_start = ip_start + ihl;
    if packet.len() < udp_start + 8 {
        return None;
    }
    let src_port = u16::from_be_bytes([packet[udp_start], packet[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([packet[udp_start + 2], packet[udp_start + 3]]);
    let udp_len = u16::from_be_bytes([packet[udp_start + 4], packet[udp_start + 5]]) as usize;
    if udp_len < 8 || packet.len() < udp_start + udp_len {
        return None;
    }
    let payload = &packet[udp_start + 8..udp_start + udp_len];
    Some((payload, src_ip, src_port, dst_ip, dst_port, proto))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_packet() {
        let payload = b"location=Martlock;item=T4_BAG;qty=3;price=1250";
        let mut packet = Vec::new();
        let frame_len = 6 + payload.len();
        packet.extend_from_slice(&(frame_len as u16).to_be_bytes());
        packet.push(7);
        packet.push(0);
        packet.extend_from_slice(&1u16.to_be_bytes());
        packet.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        packet.extend_from_slice(payload);

        let tx = decode_transaction(&packet).unwrap();
        assert_eq!(tx.location, "Martlock");
        assert_eq!(tx.total_cost, 3750);
    }
}
