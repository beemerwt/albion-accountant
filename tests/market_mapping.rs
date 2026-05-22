mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, extract_market_transactions, extract_udp_payload},
    ids,
    market_mapper::{DecodedOperationResponse, map_response_to_transaction},
    protocol::{commands::decode_command_envelope, protocol16::ProtocolValue, transport::parse_udp_payload},
};
use std::collections::BTreeMap;
use support::load_pcapng_packets;

#[test]
fn maps_market_packets_from_pcapng_to_transactions() {
    let packets = load_pcapng_packets("../../quick_buy_and_sell.pcapng");
    let mut messages = Vec::new();
    let mut udp_payloads = 0usize;
    let mut transport_frames = 0usize;

    for packet in &packets {
        let Ok(tuple) = extract_udp_payload(CapturePacket { link_type: 1, packet }) else {
            continue;
        };
        udp_payloads += 1;
        let Ok(frames) = parse_udp_payload(tuple.payload) else {
            continue;
        };
        transport_frames += frames.len();
        for frame in frames {
            if let Ok(msg) = decode_command_envelope(&frame.body) {
                messages.push(msg);
            }
        }
    }

    let txs = extract_market_transactions(&messages);
    eprintln!(
        "[debug] market mapping replay stats: packets={}, udp_payloads={}, transport_frames={}, decoded_messages={}, extracted_transactions={}",
        packets.len(),
        udp_payloads,
        transport_frames,
        messages.len(),
        txs.len()
    );
    assert!(
        messages.iter().any(|m| m.payload_length > 0),
        "expected non-empty decoded payloads from pcapng"
    );
}

#[test]
fn mapping_table_supported_opcodes_require_expected_fields() {
    for op_code in ids::MARKET_OPERATION_CODES {
        let response = DecodedOperationResponse {
            op_code: *op_code,
            return_code: 0,
            params: BTreeMap::from([
                ("LocationId".to_string(), ProtocolValue::String("Martlock".to_string())),
                ("ItemTypeId".to_string(), ProtocolValue::String("T4_BAG".to_string())),
                ("Amount".to_string(), ProtocolValue::UnsignedInt(3)),
                ("UnitPriceSilver".to_string(), ProtocolValue::UnsignedLong(1200)),
            ]),
        };
        let tx = map_response_to_transaction(&response)
            .unwrap_or_else(|| panic!("opcode {op_code:#x} should map with canonical fields"));
        assert_eq!(tx.location, "Martlock");
    }
}
