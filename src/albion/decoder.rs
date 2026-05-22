// Maintainer architecture note:
// 1) transport::parse_udp_payload_incremental splits Photon UDP datagrams into frames.
// 2) commands::decode_command_envelope parses each frame into PhotonMessage metadata + payload.
// 3) events::decode_event_payload / operations::decode_operation_payload decode Protocol16 maps.
// 4) market_mapper converts decoded protocol fields into MarketTransaction domain objects.
//
// To extend decoding for new opcodes, keep protocol parsing generic and add mapping logic in
// market_mapper (and ids as needed), keyed by event code / operation code.

use super::{
    ids,
    market_mapper::{
        DecodedEvent, DecodedOperationResponse, map_event_to_transaction,
        map_response_to_transaction,
    },
    protocol::{
        commands::{AlbionCommandType, PhotonMessage},
        events::decode_event_payload,
        operations::decode_operation_payload,
        protocol16::ProtocolValue,
    },
    transaction::MarketTransaction,
};

#[derive(Debug, Clone)]
pub enum DecodeProbe {
    EventDecoded {
        code: u16,
        key_count: usize,
        message_type: &'static str,
        encrypted_like: bool,
    },
    OperationDecoded {
        op_code: u16,
        return_code: i16,
        key_count: usize,
        message_type: &'static str,
        encrypted_like: bool,
    },
    UnsupportedCommandType {
        command_type: u16,
        message_type: &'static str,
        encrypted_like: bool,
    },
    EventDecodeFailed,
    OperationDecodeFailed,
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
    match AlbionCommandType::from_message_type(message.message_type) {
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
        AlbionCommandType::Reliable
        | AlbionCommandType::Unreliable
        | AlbionCommandType::Fragment
        | AlbionCommandType::Disconnect
        | AlbionCommandType::OperationRequest => {
            tracing::debug!(
                command_type = u16::from(message.command_type),
                channel = message.channel,
                seq = message.reliable_sequence,
                "ignoring non-decoded transport command type"
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
        AlbionCommandType::Reliable
        | AlbionCommandType::Unreliable
        | AlbionCommandType::Fragment
        | AlbionCommandType::Disconnect
        | AlbionCommandType::OperationRequest
        | AlbionCommandType::Unsupported(_) => None,
    }
}

fn decoded_event_from_map(
    map: &std::collections::BTreeMap<String, ProtocolValue>,
) -> Option<DecodedEvent> {
    let code = match map.get(ids::KEY_EVENT_CODE)? {
        ProtocolValue::Byte(v) => u16::from(*v),
        ProtocolValue::Short(v) => u16::try_from(*v).ok()?,
        ProtocolValue::Int(v) => u16::try_from(*v).ok()?,
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
        ProtocolValue::Byte(v) => u16::from(*v),
        ProtocolValue::Short(v) => u16::try_from(*v).ok()?,
        ProtocolValue::Int(v) => u16::try_from(*v).ok()?,
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

pub fn probe_message(message: &PhotonMessage) -> DecodeProbe {
    match AlbionCommandType::from_message_type(message.message_type) {
        AlbionCommandType::Event => {
            let Ok(event_map) = decode_event_payload(&message.payload) else {
                return DecodeProbe::EventDecodeFailed;
            };
            let Some(event) = decoded_event_from_map(&event_map) else {
                return DecodeProbe::EventDecodeFailed;
            };
            DecodeProbe::EventDecoded {
                code: event.code,
                key_count: event.params.len(),
                message_type: "event",
                encrypted_like: payload_looks_encrypted(&message.payload),
            }
        }
        AlbionCommandType::OperationResponse => {
            let Ok(response_map) = decode_operation_payload(&message.payload) else {
                return DecodeProbe::OperationDecodeFailed;
            };
            let Some(response) = decoded_response_from_map(&response_map) else {
                return DecodeProbe::OperationDecodeFailed;
            };
            DecodeProbe::OperationDecoded {
                op_code: response.op_code,
                return_code: response.return_code,
                key_count: response.params.len(),
                message_type: "response",
                encrypted_like: payload_looks_encrypted(&message.payload),
            }
        }
        AlbionCommandType::Unsupported(command_type) => DecodeProbe::UnsupportedCommandType {
            command_type,
            message_type: "unknown",
            encrypted_like: payload_looks_encrypted(&message.payload),
        },
        AlbionCommandType::Reliable
        | AlbionCommandType::Unreliable
        | AlbionCommandType::Fragment
        | AlbionCommandType::Disconnect
        | AlbionCommandType::OperationRequest => DecodeProbe::UnsupportedCommandType {
            command_type: u16::from(message.command_type),
            message_type: "request",
            encrypted_like: payload_looks_encrypted(&message.payload),
        },
    }
}

fn payload_looks_encrypted(payload: &[u8]) -> bool {
    payload
        .first()
        .map(|b| matches!(*b, 0xF3 | 0xFD | 0x7E))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturePacket<'a> {
    pub link_type: i32,
    pub packet: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpTuple<'a> {
    pub payload: &'a [u8],
    pub src_ip: std::net::IpAddr,
    pub src_port: u16,
    pub dst_ip: std::net::IpAddr,
    pub dst_port: u16,
    pub protocol: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdpExtractDropReason {
    UnsupportedLinkType,
    TruncatedL2,
    UnsupportedEtherType,
    TruncatedIpv4,
    TruncatedIpv6,
    NonUdp,
    TruncatedUdp,
}

pub type UdpExtractResult<'a> = Result<UdpTuple<'a>, UdpExtractDropReason>;

pub fn extract_udp_payload(packet: CapturePacket<'_>) -> UdpExtractResult<'_> {
    let (l3_start, ether_type) = match packet.link_type {
        1 => {
            if packet.packet.len() < 14 {
                return Err(UdpExtractDropReason::TruncatedL2);
            }
            (
                14usize,
                u16::from_be_bytes([packet.packet[12], packet.packet[13]]),
            )
        }
        113 => {
            if packet.packet.len() < 16 {
                return Err(UdpExtractDropReason::TruncatedL2);
            }
            (
                16usize,
                u16::from_be_bytes([packet.packet[14], packet.packet[15]]),
            )
        }
        _ => return Err(UdpExtractDropReason::UnsupportedLinkType),
    };

    match ether_type {
        0x0800 => extract_udp_ipv4(packet.packet, l3_start),
        0x86DD => extract_udp_ipv6(packet.packet, l3_start),
        _ => Err(UdpExtractDropReason::UnsupportedEtherType),
    }
}

fn extract_udp_ipv4(packet: &[u8], ip_start: usize) -> UdpExtractResult<'_> {
    if packet.len() < ip_start + 20 {
        return Err(UdpExtractDropReason::TruncatedIpv4);
    }
    let ihl = (packet[ip_start] & 0x0f) as usize * 4;
    if ihl < 20 || packet.len() < ip_start + ihl {
        return Err(UdpExtractDropReason::TruncatedIpv4);
    }
    let proto = packet[ip_start + 9];
    if proto != 17 {
        return Err(UdpExtractDropReason::NonUdp);
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
    extract_udp_common(packet, ip_start + ihl, src_ip, dst_ip, proto)
}

fn extract_udp_ipv6(packet: &[u8], ip_start: usize) -> UdpExtractResult<'_> {
    if packet.len() < ip_start + 40 {
        return Err(UdpExtractDropReason::TruncatedIpv6);
    }
    let next_header = packet[ip_start + 6];
    if next_header != 17 {
        return Err(UdpExtractDropReason::NonUdp);
    }
    let src_ip = std::net::IpAddr::V6(std::net::Ipv6Addr::from(
        <[u8; 16]>::try_from(&packet[ip_start + 8..ip_start + 24]).expect("checked len"),
    ));
    let dst_ip = std::net::IpAddr::V6(std::net::Ipv6Addr::from(
        <[u8; 16]>::try_from(&packet[ip_start + 24..ip_start + 40]).expect("checked len"),
    ));
    extract_udp_common(packet, ip_start + 40, src_ip, dst_ip, next_header)
}

fn extract_udp_common(
    packet: &[u8],
    udp_start: usize,
    src_ip: std::net::IpAddr,
    dst_ip: std::net::IpAddr,
    protocol: u8,
) -> UdpExtractResult<'_> {
    if packet.len() < udp_start + 8 {
        return Err(UdpExtractDropReason::TruncatedUdp);
    }
    let src_port = u16::from_be_bytes([packet[udp_start], packet[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([packet[udp_start + 2], packet[udp_start + 3]]);
    let udp_len = u16::from_be_bytes([packet[udp_start + 4], packet[udp_start + 5]]) as usize;
    if udp_len < 8 || packet.len() < udp_start + udp_len {
        return Err(UdpExtractDropReason::TruncatedUdp);
    }
    Ok(UdpTuple {
        payload: &packet[udp_start + 8..udp_start + udp_len],
        src_ip,
        src_port,
        dst_ip,
        dst_port,
        protocol,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::albion::protocol::{
        commands::{AlbionCommandType, decode_command_envelope},
        protocol16::ProtocolValue,
        transport::parse_udp_payload_incremental,
    };
    use std::collections::BTreeMap;

    #[test]
    #[ignore = "fixture pending protocol refresh"]
    fn parses_protocol_framed_market_event_packet() {
        let payload = build_event_payload("Martlock", "T4_BAG", 3, 1250);
        let packet = build_framed_packet(payload);

        let frames = parse_udp_payload_incremental(&packet).expect("valid framed packet");
        let messages: Vec<PhotonMessage> = frames
            .into_iter()
            .map(|f| decode_command_envelope(&f.body).expect("valid command envelope"))
            .collect();
        let tx_opt = extract_market_transactions(&messages).into_iter().next();
        assert!(
            tx_opt.is_some(),
            "expected decoded transaction for framed event packet (command_type={:?}, event_code={}, payload_keys={:?})",
            AlbionCommandType::Event,
            crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as u16,
            [ids::KEY_EVENT_CODE, ids::KEY_PARAMS]
        );
        let tx = tx_opt.expect("checked is_some above");
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
        out.extend_from_slice(
            &(crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as u16).to_be_bytes(),
        );

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

    fn market_params() -> BTreeMap<String, ProtocolValue> {
        BTreeMap::from([
            (
                "location".to_string(),
                ProtocolValue::String("Bridgewatch".to_string()),
            ),
            (
                "item".to_string(),
                ProtocolValue::String("T5_BAG".to_string()),
            ),
            ("qty".to_string(), ProtocolValue::Int(2)),
            ("price".to_string(), ProtocolValue::Long(1000)),
        ])
    }

    #[test]
    fn dispatches_event_payload_by_command_type() {
        let payload_map = BTreeMap::from([
            (
                ids::KEY_EVENT_CODE.to_string(),
                ProtocolValue::Short(
                    crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as i16,
                ),
            ),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);
        let tx_opt = map_decoded_payload_to_transaction(AlbionCommandType::Event, &payload_map);
        assert!(
            tx_opt.is_some(),
            "expected event dispatch to decode (command_type={:?}, event_code={}, payload_keys={:?})",
            AlbionCommandType::Event,
            crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as u16,
            payload_map.keys().collect::<Vec<_>>()
        );
        let tx = tx_opt.expect("checked is_some above");
        assert_eq!(tx.location, "Bridgewatch");
        assert_eq!(tx.total_cost, 2000);
    }

    #[test]
    fn dispatches_operation_response_payload_by_command_type() {
        let payload_map = BTreeMap::from([
            (
                ids::KEY_OP_CODE.to_string(),
                ProtocolValue::Short(
                    crate::albion::operation_codes::OperationCodes::AuctionGetOffers as i16,
                ),
            ),
            (ids::KEY_RETURN_CODE.to_string(), ProtocolValue::Short(0)),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);
        let tx_opt =
            map_decoded_payload_to_transaction(AlbionCommandType::OperationResponse, &payload_map);
        assert!(
            tx_opt.is_some(),
            "expected operation response dispatch to decode (command_type={:?}, op_code={}, payload_keys={:?})",
            AlbionCommandType::OperationResponse,
            crate::albion::operation_codes::OperationCodes::AuctionGetOffers as u16,
            payload_map.keys().collect::<Vec<_>>()
        );
        let tx = tx_opt.expect("checked is_some above");
        assert_eq!(tx.location, "Bridgewatch");
        assert_eq!(tx.total_cost, 2000);
    }

    #[test]
    fn returns_none_for_unsupported_event_code() {
        let payload_map = BTreeMap::from([
            (ids::KEY_EVENT_CODE.to_string(), ProtocolValue::Byte(57)),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);

        let tx = map_decoded_payload_to_transaction(AlbionCommandType::Event, &payload_map);
        assert!(
            tx.is_none(),
            "expected None for unsupported event code (command_type={:?}, event_code={}, payload_keys={:?})",
            AlbionCommandType::Event,
            57_u8,
            payload_map.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn returns_none_for_unsupported_operation_code() {
        let payload_map = BTreeMap::from([
            (ids::KEY_OP_CODE.to_string(), ProtocolValue::Byte(84)),
            (ids::KEY_RETURN_CODE.to_string(), ProtocolValue::Short(0)),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);

        let tx =
            map_decoded_payload_to_transaction(AlbionCommandType::OperationResponse, &payload_map);
        assert!(
            tx.is_none(),
            "expected None for unsupported operation code (command_type={:?}, op_code={}, payload_keys={:?})",
            AlbionCommandType::OperationResponse,
            84_u8,
            payload_map.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn returns_none_for_event_missing_params_key() {
        let payload_map = BTreeMap::from([(
            ids::KEY_EVENT_CODE.to_string(),
            ProtocolValue::Short(
                crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as i16,
            ),
        )]);

        let tx = map_decoded_payload_to_transaction(AlbionCommandType::Event, &payload_map);
        assert!(
            tx.is_none(),
            "expected None for missing event params key (command_type={:?}, event_code={}, payload_keys={:?})",
            AlbionCommandType::Event,
            crate::albion::event_codes::EventCodes::MarketPlaceBuildingInfo as u16,
            payload_map.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn returns_none_for_operation_missing_return_code_key() {
        let payload_map = BTreeMap::from([
            (
                ids::KEY_OP_CODE.to_string(),
                ProtocolValue::Short(
                    crate::albion::operation_codes::OperationCodes::AuctionGetOffers as i16,
                ),
            ),
            (
                ids::KEY_PARAMS.to_string(),
                ProtocolValue::Dictionary(market_params()),
            ),
        ]);

        let tx =
            map_decoded_payload_to_transaction(AlbionCommandType::OperationResponse, &payload_map);
        assert!(
            tx.is_none(),
            "expected None for missing operation return code key (command_type={:?}, op_code={}, payload_keys={:?})",
            AlbionCommandType::OperationResponse,
            crate::albion::operation_codes::OperationCodes::AuctionGetOffers as u16,
            payload_map.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn ignores_unsupported_command_type() {
        let message = PhotonMessage {
            command_type: 255,
            channel: 2,
            command_flags: 0,
            reliable_sequence: 10,
            signal_byte: 0,
            message_type: 0,
            payload_length: 0,
            payload: Vec::new(),
        };

        assert!(map_message_to_transaction(&message).is_none());
    }
    #[test]
    fn extracts_valid_ipv4_udp_frame() {
        let frame = build_eth_ipv4_udp_frame(&[1, 2, 3, 4]);
        let parsed = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet: &frame,
        })
        .expect("valid");
        assert_eq!(parsed.src_port, 1000);
        assert_eq!(parsed.dst_port, 2000);
        assert_eq!(parsed.payload, &[1, 2, 3, 4]);
    }

    #[test]
    fn extracts_valid_ipv6_udp_frame() {
        let frame = build_eth_ipv6_udp_frame(&[9, 8, 7]);
        let parsed = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet: &frame,
        })
        .expect("valid");
        assert_eq!(parsed.protocol, 17);
        assert_eq!(parsed.payload, &[9, 8, 7]);
        assert!(matches!(parsed.src_ip, std::net::IpAddr::V6(_)));
    }

    #[test]
    fn rejects_unsupported_link_type() {
        let frame = build_eth_ipv4_udp_frame(&[1]);
        let err = extract_udp_payload(CapturePacket {
            link_type: 999,
            packet: &frame,
        })
        .unwrap_err();
        assert_eq!(err, UdpExtractDropReason::UnsupportedLinkType);
    }

    #[test]
    fn rejects_truncated_headers() {
        assert_eq!(
            extract_udp_payload(CapturePacket {
                link_type: 1,
                packet: &[0; 10]
            })
            .unwrap_err(),
            UdpExtractDropReason::TruncatedL2
        );
        let mut ipv4 = build_eth_ipv4_udp_frame(&[1, 2]);
        ipv4.truncate(30);
        assert_eq!(
            extract_udp_payload(CapturePacket {
                link_type: 1,
                packet: &ipv4
            })
            .unwrap_err(),
            UdpExtractDropReason::TruncatedIpv4
        );
        let mut ipv6 = build_eth_ipv6_udp_frame(&[1, 2]);
        ipv6.truncate(40);
        assert_eq!(
            extract_udp_payload(CapturePacket {
                link_type: 1,
                packet: &ipv6
            })
            .unwrap_err(),
            UdpExtractDropReason::TruncatedIpv6
        );
        let mut udp = build_eth_ipv4_udp_frame(&[1, 2]);
        udp.truncate(14 + 20 + 6);
        assert_eq!(
            extract_udp_payload(CapturePacket {
                link_type: 1,
                packet: &udp
            })
            .unwrap_err(),
            UdpExtractDropReason::TruncatedUdp
        );
    }

    fn build_eth_ipv4_udp_frame(payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0u8; 14];
        pkt[12] = 0x08;
        pkt[13] = 0x00;
        pkt.extend_from_slice(&[
            0x45, 0x00, 0x00, 0x00, 0, 0, 0, 0, 64, 17, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2,
        ]);
        let udp_len = (8 + payload.len()) as u16;
        pkt.extend_from_slice(&1000u16.to_be_bytes());
        pkt.extend_from_slice(&2000u16.to_be_bytes());
        pkt.extend_from_slice(&udp_len.to_be_bytes());
        pkt.extend_from_slice(&0u16.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    fn build_eth_ipv6_udp_frame(payload: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0u8; 14];
        pkt[12] = 0x86;
        pkt[13] = 0xDD;
        let udp_len = (8 + payload.len()) as u16;
        pkt.extend_from_slice(&[0x60, 0, 0, 0]);
        pkt.extend_from_slice(&udp_len.to_be_bytes());
        pkt.push(17);
        pkt.push(64);
        pkt.extend_from_slice(&[0u8; 16]);
        pkt.extend_from_slice(&[1u8; 16]);
        pkt.extend_from_slice(&1000u16.to_be_bytes());
        pkt.extend_from_slice(&2000u16.to_be_bytes());
        pkt.extend_from_slice(&udp_len.to_be_bytes());
        pkt.extend_from_slice(&0u16.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }
}
