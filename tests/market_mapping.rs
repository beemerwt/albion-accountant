mod support {
    pub mod fixture_loader;
}

use albion_accountant::albion::{
    decoder::{decode_packet, extract_market_transactions},
    protocol::{commands::decode_command_envelope, events::decode_event_payload, transport::parse_udp_payload},
};
use serde_json::json;
use support::fixture_loader::{assert_json_golden_eq, load_hex_fixture, protocol_value_to_json};
use std::collections::BTreeMap;
use albion_accountant::albion::{
    ids,
    market_mapper::{map_response_to_transaction, DecodedOperationResponse},
    protocol::protocol16::ProtocolValue,
};

#[test]
fn maps_market_packet_to_transaction_and_matches_golden() {
    let packet = load_hex_fixture("market_packet_valid.hex");
    let messages = decode_packet(&packet);
    assert_eq!(messages.len(), 1);

    let txs = extract_market_transactions(&messages);
    assert_eq!(txs.len(), 1);
    let tx = &txs[0];

    let actual = json!({
        "location": tx.location,
        "item": tx.item,
        "quantity": tx.quantity,
        "per_item_cost": tx.per_item_cost,
        "total_cost": tx.total_cost
    });
    assert_json_golden_eq(&actual, "market_packet_valid.expected.json");
}

#[test]
fn decoded_event_map_matches_snapshot() {
    let packet = load_hex_fixture("market_packet_valid.hex");
    let frames = parse_udp_payload(&packet).expect("valid frame");
    let cmd = decode_command_envelope(&frames[0].body).expect("valid envelope");
    let event_map = decode_event_payload(&cmd.payload).expect("valid event payload");

    let actual = protocol_value_to_json(&albion_accountant::albion::protocol::protocol16::ProtocolValue::Dictionary(event_map));
    assert_json_golden_eq(&actual, "protocol16_complex.expected.json");
}

#[test]
fn unsupported_opcode_is_ignored_deterministically() {
    let packet = load_hex_fixture("unsupported_opcode_packet.hex");
    let messages = decode_packet(&packet);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].command_type, 9);

    let txs = extract_market_transactions(&messages);
    assert!(txs.is_empty(), "unsupported opcode must not map to market transactions");
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
        assert_eq!(tx.item, "T4_BAG");
        assert_eq!(tx.quantity, 3);
        assert_eq!(tx.per_item_cost, 1200);
    }
}
