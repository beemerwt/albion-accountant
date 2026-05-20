mod support {
    pub mod fixture_loader;
}

use albion_accountant::albion::{
    decoder::{decode_packet, extract_market_transactions},
    protocol::{commands::decode_command_envelope, events::decode_event_payload, transport::parse_udp_payload},
};
use serde_json::json;
use support::fixture_loader::{assert_json_golden_eq, load_hex_fixture, protocol_value_to_json};

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
