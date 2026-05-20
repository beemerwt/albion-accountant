use super::{
    ids,
    market_mapper::{
        DecodedEvent, DecodedOperationResponse, map_event_to_transaction,
        map_response_to_transaction,
    },
    protocol::{
        commands::{PhotonMessage, decode_command_envelope},
        events::decode_event_payload,
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
    // Compile-time guard: only protocol-decoded mappings are supported here;
    // non-protocol text fallbacks are intentionally unsupported.
    messages
        .iter()
        .filter_map(map_message_to_transaction)
        .collect()
}

fn map_message_to_transaction(message: &PhotonMessage) -> Option<MarketTransaction> {
    let event_map = decode_event_payload(&message.payload).ok()?;
    if let Some(event) = decoded_event_from_map(&event_map) {
        if let Some(tx) = map_event_to_transaction(&event) {
            return Some(tx);
        }
    }
    if let Some(response) = decoded_response_from_map(&event_map) {
        if let Some(tx) = map_response_to_transaction(&response) {
            return Some(tx);
        }
    }
    None
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

    #[test]
    fn parses_protocol_framed_market_event_packet() {
        let payload = build_event_payload("Martlock", "T4_BAG", 3, 1250);
        let packet = build_framed_packet(payload);

        let tx = decode_transaction(&packet).unwrap();
        assert_eq!(tx.location, "Martlock");
        assert_eq!(tx.total_cost, 3750);
    }

    fn build_framed_packet(payload: Vec<u8>) -> Vec<u8> {
        let mut body = Vec::new();
        body.push(7);
        body.push(0);
        body.extend_from_slice(&1u16.to_be_bytes());
        body.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        body.extend_from_slice(&payload);

        let mut packet = Vec::new();
        packet.extend_from_slice(&(body.len() as u16).to_be_bytes());
        packet.extend_from_slice(&body);
        packet
    }

    fn build_event_payload(location: &str, item: &str, qty: i32, price: i32) -> Vec<u8> {
        let mut out = Vec::new();

        out.push(b'd');
        out.extend_from_slice(&2u16.to_be_bytes());

        write_string(&mut out, ids::KEY_EVENT_CODE);
        out.push(b'b');
        out.push(0x2a);

        write_string(&mut out, ids::KEY_PARAMS);
        out.push(b'd');
        out.extend_from_slice(&4u16.to_be_bytes());

        write_string_value(&mut out, "location", location);
        write_string_value(&mut out, "item", item);
        write_int_value(&mut out, "qty", qty);
        write_int_value(&mut out, "price", price);

        out
    }

    fn write_string_value(out: &mut Vec<u8>, key: &str, value: &str) {
        write_string(out, key);
        out.push(b't');
        write_string(out, value);
    }

    fn write_int_value(out: &mut Vec<u8>, key: &str, value: i32) {
        write_string(out, key);
        out.push(b'i');
        out.extend_from_slice(&value.to_be_bytes());
    }

    fn write_string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u16).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }
}
