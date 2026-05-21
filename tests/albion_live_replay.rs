mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, DecodeProbe, extract_udp_payload, probe_message},
    market_mapper::{DecodedOperationResponse, map_response_to_transaction},
    protocol::{
        commands::{AlbionCommandType, decode_command_envelope},
        operations::decode_operation_payload,
        protocol16::ProtocolValue,
        transport::parse_udp_payload,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};
use support::load_hex_fixture;

#[test]
fn replay_albion_live_fixtures_across_decode_stages() {
    let manifest = load_json_fixture("albion_live/replay_manifest.json");
    let fixtures = manifest["fixtures"].as_array().expect("fixtures array");
    assert!(
        !fixtures.is_empty(),
        "replay manifest must include fixtures"
    );

    for fixture in fixtures {
        let name = fixture["name"].as_str().expect("name");
        let frame_fixture = fixture["raw_frame"].as_str().expect("raw_frame");
        let raw_frame = load_hex_fixture(&format!("albion_live/{frame_fixture}"));

        let tuple = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet: &raw_frame,
        })
        .expect("valid ipv4/udp packet");

        let frames = parse_udp_payload(tuple.payload).expect("transport frame parses");
        assert_eq!(frames.len(), 1, "{name}: expected a single transport frame");

        let envelope = decode_command_envelope(&frames[0].body).expect("command envelope decodes");
        assert_command_stage(name, fixture, &envelope);

        match AlbionCommandType::from(envelope.command_type) {
            AlbionCommandType::OperationResponse => {
                let op_map = decode_operation_payload(&envelope.payload)
                    .expect("operation response payload decodes");
                let probe = probe_message(&envelope);
                assert_probe_stage(name, fixture, &probe);

                let decoded =
                    decoded_response_from_map(&op_map).expect("operation response core fields");
                let mapped = map_response_to_transaction(&decoded);
                assert_mapping_stage(name, fixture, mapped);
            }
            other => panic!("{name}: unsupported command type in replay fixture: {other:?}"),
        }
    }
}

fn assert_command_stage(
    name: &str,
    fixture: &Value,
    envelope: &albion_accountant::albion::protocol::commands::PhotonMessage,
) {
    let expected = &fixture["expected"]["command_envelope"];
    assert_eq!(
        envelope.command_type as u64,
        expected["command_type"]
            .as_u64()
            .expect("expected command_type"),
        "{name}: command type mismatch"
    );
}

fn assert_probe_stage(name: &str, fixture: &Value, probe: &DecodeProbe) {
    let expected = &fixture["expected"]["message_probe"];
    let expected_status = expected["status"].as_str().expect("probe status");

    match (expected_status, probe) {
        ("operation_decoded", DecodeProbe::OperationDecoded { op_code, .. }) => {
            assert_eq!(
                *op_code as u64,
                expected["op_code"].as_u64().expect("expected op_code"),
                "{name}: probe op_code mismatch"
            );
        }
        ("operation_decode_failed", DecodeProbe::OperationDecodeFailed) => {}
        (other, actual) => panic!("{name}: probe mismatch expected {other}, got {actual:?}"),
    }
}

fn assert_mapping_stage(
    name: &str,
    fixture: &Value,
    mapped: Option<albion_accountant::albion::transaction::MarketTransaction>,
) {
    let expected = &fixture["expected"]["transaction_mapper"];
    let expected_status = expected["status"].as_str().expect("mapping status");

    match expected_status {
        "mapped" => {
            let tx = mapped.unwrap_or_else(|| panic!("{name}: expected mapped transaction"));
            let tx_expected = expected["transaction"]
                .as_object()
                .expect("transaction object");
            assert_eq!(tx.location, tx_expected["location"].as_str().unwrap());
            assert_eq!(tx.item, tx_expected["item"].as_str().unwrap());
            assert_eq!(
                tx.quantity as u64,
                tx_expected["quantity"].as_u64().unwrap()
            );
            assert_eq!(
                tx.per_item_cost,
                tx_expected["per_item_cost"].as_u64().unwrap()
            );
            assert_eq!(tx.total_cost, tx_expected["total_cost"].as_u64().unwrap());
        }
        "not_mapped" => {
            assert!(mapped.is_none(), "{name}: expected mapper to return none");
            let reason = expected["reason"].as_str().unwrap_or("unspecified");
            assert!(
                !reason.is_empty(),
                "{name}: not_mapped reason must be non-empty"
            );
        }
        other => panic!("{name}: unsupported mapping status {other}"),
    }
}

fn decoded_response_from_map(
    map: &BTreeMap<String, ProtocolValue>,
) -> Option<DecodedOperationResponse> {
    let op_code = match map.get("op_code")? {
        ProtocolValue::Byte(v) => u16::try_from(*v).ok()?,
        ProtocolValue::Short(v) => u16::try_from(*v).ok()?,
        ProtocolValue::Int(v) => u16::try_from(*v).ok()?,
        _ => return None,
    };
    let return_code = match map.get("return_code")? {
        ProtocolValue::Short(v) => *v,
        ProtocolValue::Int(v) => i16::try_from(*v).ok()?,
        _ => return None,
    };
    let params = match map.get("params")? {
        ProtocolValue::Dictionary(v) | ProtocolValue::Hashtable(v) => v.clone(),
        _ => return None,
    };

    Some(DecodedOperationResponse {
        op_code,
        return_code,
        params,
    })
}

fn load_json_fixture(name: &str) -> Value {
    let raw = fs::read_to_string(fixture_path(name)).expect("json fixture readable");
    serde_json::from_str(&raw).expect("valid json fixture")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
