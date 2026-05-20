use regex::Regex;

use super::{
    ids,
    market_mapper::{
        DecodedEvent, DecodedOperationResponse, map_event_to_transaction,
        map_response_to_transaction,
    },
    protocol::{
        commands::{AlbionCommandType, PhotonMessage, decode_command_envelope},
        events::decode_event_payload,
        operations::decode_operation_payload,
        protocol16::ProtocolValue,
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
        .filter_map(|m| map_message_to_transaction(m).or_else(|| map_text_fallback(m, &re)))
        .collect()
}

fn map_message_to_transaction(message: &PhotonMessage) -> Option<MarketTransaction> {
    match AlbionCommandType::from(message.command_type) {
        AlbionCommandType::Event => {
            let event_map = decode_event_payload(&message.payload).ok()?;
            return map_decoded_payload_to_transaction(AlbionCommandType::Event, &event_map);
        }
        AlbionCommandType::OperationResponse => {
            let response_map = decode_operation_payload(&message.payload).ok()?;
            return map_decoded_payload_to_transaction(
                AlbionCommandType::OperationResponse,
                &response_map,
            );
        }
        AlbionCommandType::Unsupported(command_type) => {
            tracing::debug!(
                command_type,
                channel = message.channel,
                seq = message.reliable_sequence,
                "ignoring unsupported command type"
            );
        }
    }
    None
}

fn map_decoded_payload_to_transaction(
    command_type: AlbionCommandType,
    map: &std::collections::BTreeMap<String, ProtocolValue>,
) -> Option<MarketTransaction> {
    match command_type {
        AlbionCommandType::Event => {
            let event = decoded_event_from_map(map)?;
            map_event_to_transaction(&event)
        }
        AlbionCommandType::OperationResponse => {
            let response = decoded_response_from_map(map)?;
            map_response_to_transaction(&response)
        }
        AlbionCommandType::Unsupported(_) => None,
    }
}

fn decoded_event_from_map(
    map: &std::collections::BTreeMap<String, ProtocolValue>,
) -> Option<DecodedEvent> {
    let code = match map.get(ids::KEY_EVENT_CODE)? {
        ProtocolValue::Byte(v) => *v,
        ProtocolValue::Short(v) => u8::try_from(*v).ok()?,
        ProtocolValue::Int(v) => u8::try_from(*v).ok()?,
        _ => return None,
    };
    let params = match map.get(ids::KEY_PARAMS)? {
        ProtocolValue::Dictionary(v) | ProtocolValue::Hashtable(v) => v.clone(),
        _ => return None,
    };
    Some(DecodedEvent { code, params })
}

fn decoded_response_from_map(
    map: &std::collections::BTreeMap<String, ProtocolValue>,
) -> Option<DecodedOperationResponse> {
    let op_code = match map.get(ids::KEY_OP_CODE)? {
        ProtocolValue::Byte(v) => *v,
        ProtocolValue::Short(v) => u8::try_from(*v).ok()?,
        ProtocolValue::Int(v) => u8::try_from(*v).ok()?,
        _ => return None,
    };
    let return_code = match map.get(ids::KEY_RETURN_CODE)? {
        ProtocolValue::Short(v) => *v,
        ProtocolValue::Int(v) => i16::try_from(*v).ok()?,
        _ => return None,
    };
    let params = match map.get(ids::KEY_PARAMS)? {
        ProtocolValue::Dictionary(v) | ProtocolValue::Hashtable(v) => v.clone(),
        _ => return None,
    };
    Some(DecodedOperationResponse {
        op_code,
        return_code,
        params,
    })
}

fn map_text_fallback(message: &PhotonMessage, re: &Regex) -> Option<MarketTransaction> {
    let text = std::str::from_utf8(&message.payload).ok()?;
    let captures = re.captures(text)?;
    let location = captures.name("location")?.as_str().to_string();
    let item = captures.name("item")?.as_str().to_string();
    let qty: u32 = captures.name("qty")?.as_str().parse().ok()?;
    let price: u64 = captures.name("price")?.as_str().parse().ok()?;
    MarketTransaction::new(location, item, qty, price, None).ok()
}

pub fn decode_transaction(packet: &[u8]) -> Option<MarketTransaction> {
    let messages = decode_packet(packet);
    extract_market_transactions(&messages).into_iter().next()
}

pub fn extract_udp_payload_ipv4(
    packet: &[u8],
) -> Option<(&[u8], std::net::IpAddr, u16, std::net::IpAddr, u16, u8)> {
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
    use crate::albion::protocol::{
        commands::AlbionCommandType,
        protocol16::ProtocolValue,
    };
    use std::collections::BTreeMap;

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

    fn market_params() -> BTreeMap<String, ProtocolValue> {
        BTreeMap::from([
            ("location".to_string(), ProtocolValue::String("Bridgewatch".to_string())),
            ("item".to_string(), ProtocolValue::String("T5_BAG".to_string())),
            ("qty".to_string(), ProtocolValue::Int(2)),
            ("price".to_string(), ProtocolValue::Long(1000)),
        ])
    }

    #[test]
    fn dispatches_event_payload_by_command_type() {
        let payload_map = BTreeMap::from([
            (ids::KEY_EVENT_CODE.to_string(), ProtocolValue::Byte(0x2a)),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);
        let tx = map_decoded_payload_to_transaction(AlbionCommandType::Event, &payload_map).unwrap();
        assert_eq!(tx.location, "Bridgewatch");
        assert_eq!(tx.total_cost, 2000);
    }

    #[test]
    fn dispatches_operation_response_payload_by_command_type() {
        let payload_map = BTreeMap::from([
            (
                ids::KEY_OP_CODE.to_string(),
                ProtocolValue::Byte(0x5a),
            ),
            (ids::KEY_RETURN_CODE.to_string(), ProtocolValue::Short(0)),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);
        let tx = map_decoded_payload_to_transaction(
            AlbionCommandType::OperationResponse,
            &payload_map,
        )
        .unwrap();
        assert_eq!(tx.location, "Bridgewatch");
        assert_eq!(tx.total_cost, 2000);
    }

    #[test]
    fn ignores_unsupported_command_type() {
        let message = PhotonMessage {
            command_type: 255,
            channel: 2,
            reliable_sequence: 10,
            payload_length: 0,
            payload: Vec::new(),
        };

        assert!(map_message_to_transaction(&message).is_none());
    }
}
