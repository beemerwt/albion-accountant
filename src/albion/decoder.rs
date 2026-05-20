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
