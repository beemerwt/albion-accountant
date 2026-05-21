mod support;

use albion_accountant::albion::{
    decoder::{decode_packet, extract_market_transactions},
    protocol::{
        commands::decode_command_envelope, events::decode_event_payload,
        transport::parse_udp_payload,
    },
};
use albion_accountant::albion::{
    ids,
    market_mapper::{DecodedOperationResponse, map_response_to_transaction},
    protocol::protocol16::ProtocolValue,
};
use serde_json::json;
use std::collections::BTreeMap;
use support::load_hex_fixture;

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

    let actual = protocol_value_to_json(
        &albion_accountant::albion::protocol::protocol16::ProtocolValue::Dictionary(event_map),
    );
    assert_json_golden_eq(&actual, "protocol16_complex.expected.json");
}

#[test]
fn unsupported_opcode_is_ignored_deterministically() {
    let packet = load_hex_fixture("unsupported_opcode_packet.hex");
    let messages = decode_packet(&packet);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].command_type, 9);

    let txs = extract_market_transactions(&messages);
    assert!(
        txs.is_empty(),
        "unsupported opcode must not map to market transactions"
    );
}

#[test]
fn mapping_table_supported_opcodes_require_expected_fields() {
    for op_code in ids::MARKET_OPERATION_CODES {
        let response = DecodedOperationResponse {
            op_code: *op_code,
            return_code: 0,
            params: BTreeMap::from([
                (
                    "LocationId".to_string(),
                    ProtocolValue::String("Martlock".to_string()),
                ),
                (
                    "ItemTypeId".to_string(),
                    ProtocolValue::String("T4_BAG".to_string()),
                ),
                ("Amount".to_string(), ProtocolValue::UnsignedInt(3)),
                (
                    "UnitPriceSilver".to_string(),
                    ProtocolValue::UnsignedLong(1200),
                ),
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

use serde_json::{Map, Value};
use std::{fs, path::PathBuf};
fn load_json_fixture(name: &str) -> Value {
    let raw = fs::read_to_string(fixture_path(name)).expect("json fixture readable");
    serde_json::from_str(&raw).expect("valid json fixture")
}
fn assert_json_golden_eq(actual: &Value, expected_fixture: &str) {
    assert_eq!(
        actual,
        &load_json_fixture(expected_fixture),
        "golden snapshot mismatch for {expected_fixture}"
    );
}
fn protocol_value_to_json(value: &ProtocolValue) -> Value {
    match value {
        ProtocolValue::UnsignedByte(v) | ProtocolValue::Byte(v) => Value::from(*v),
        ProtocolValue::UnsignedShort(v) => Value::from(*v),
        ProtocolValue::Short(v) => Value::from(*v),
        ProtocolValue::UnsignedInt(v) => Value::from(*v),
        ProtocolValue::Int(v) => Value::from(*v),
        ProtocolValue::UnsignedLong(v) => Value::from(*v),
        ProtocolValue::Long(v) => Value::from(*v),
        ProtocolValue::Float(v) => Value::from(*v),
        ProtocolValue::Double(v) => Value::from(*v),
        ProtocolValue::String(v) => Value::from(v.clone()),
        ProtocolValue::Bool(v) => Value::from(*v),
        ProtocolValue::ByteArray(v) => Value::Array(v.iter().map(|b| Value::from(*b)).collect()),
        ProtocolValue::Custom(tag, wrapped) => {
            let mut out = Map::new();
            out.insert("custom_tag".to_string(), Value::from(*tag));
            out.insert("value".to_string(), protocol_value_to_json(wrapped));
            Value::Object(out)
        }
        ProtocolValue::Object(wrapped) => protocol_value_to_json(wrapped),
        ProtocolValue::Array(v) => Value::Array(v.iter().map(protocol_value_to_json).collect()),
        ProtocolValue::Dictionary(v) | ProtocolValue::Hashtable(v) => {
            let mut out = Map::new();
            for (k, item) in v {
                out.insert(k.clone(), protocol_value_to_json(item));
            }
            Value::Object(out)
        }
    }
}
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
