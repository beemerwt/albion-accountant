mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, decode_packet, extract_market_transactions, extract_udp_payload},
    market_mapper::{DecodedOperationResponse, map_response_to_transaction},
    protocol::{
        commands::{AlbionCommandType, decode_command_envelope},
        operations::decode_operation_payload,
        protocol16::ProtocolValue,
        transport::parse_udp_payload,
    },
    transaction::MarketTransaction,
};
use serde_json::Value;
use std::{collections::BTreeMap, net::IpAddr};
use support::load_hex_fixture;

#[test]
fn albion_live_fixtures_cover_full_market_pipeline() {
    let manifest = load_json_fixture("albion_live/coverage_manifest.json");
    let fixtures = manifest["fixtures"].as_array().expect("fixtures array");
    assert!(
        !fixtures.is_empty(),
        "coverage manifest must include fixtures"
    );

    for fixture in fixtures {
        let name = fixture["name"].as_str().expect("name");
        let frame_fixture = fixture["raw_frame"].as_str().expect("raw_frame");
        let expected_fixture = fixture["expected"].as_str().expect("expected");

        let raw_frame = load_hex_fixture(&format!("albion_live/{frame_fixture}"));
        let expected = load_json_fixture(&format!("albion_live/{expected_fixture}"));

        let tuple = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet: &raw_frame,
        })
        .expect("valid ipv4/udp packet");
        let (udp_payload, src_ip, src_port, dst_ip, dst_port, proto) = (
            tuple.payload,
            tuple.src_ip,
            tuple.src_port,
            tuple.dst_ip,
            tuple.dst_port,
            tuple.protocol,
        );
        assert_transport(&expected, src_ip, src_port, dst_ip, dst_port, proto);

        let frames = parse_udp_payload(udp_payload).expect("transport frame parses");
        assert_eq!(frames.len(), 1, "{name}: expected single transport frame");

        let message = decode_command_envelope(&frames[0].body).expect("command envelope parses");
        assert_eq!(
            AlbionCommandType::from(message.command_type),
            AlbionCommandType::OperationResponse,
            "{name}: must decode operation response command"
        );
        assert_eq!(
            message.payload.len(),
            usize::from(message.payload_length),
            "{name}: payload length mismatch"
        );
        assert_eq!(
            message.command_type,
            expected["command_type"].as_u64().unwrap() as u8
        );

        let payload_map =
            decode_operation_payload(&message.payload).expect("operation payload parses");
        let decoded_response =
            decoded_response_from_map(&payload_map).expect("response fields available");
        assert_eq!(
            decoded_response.op_code as u64,
            expected["opcode"].as_u64().unwrap(),
            "{name}: opcode mismatch"
        );

        let tx = map_response_to_transaction(&decoded_response).expect("maps to MarketTransaction");
        assert_market_transaction(&tx, &expected["transaction"], name);

        let messages = decode_packet(udp_payload);
        let final_txs = extract_market_transactions(&messages);
        assert_eq!(
            final_txs.len(),
            1,
            "{name}: expected exactly one final transaction"
        );
        assert_eq!(final_txs[0], tx, "{name}: mapping pipeline changed output");
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

fn assert_transport(
    expected: &Value,
    src_ip: IpAddr,
    src_port: u16,
    dst_ip: IpAddr,
    dst_port: u16,
    proto: u8,
) {
    let t = &expected["transport"];
    assert_eq!(src_ip.to_string(), t["src_ip"].as_str().unwrap());
    assert_eq!(src_port as u64, t["src_port"].as_u64().unwrap());
    assert_eq!(dst_ip.to_string(), t["dst_ip"].as_str().unwrap());
    assert_eq!(dst_port as u64, t["dst_port"].as_u64().unwrap());
    assert_eq!(proto as u64, t["proto"].as_u64().unwrap());
}

fn assert_market_transaction(actual: &MarketTransaction, expected: &Value, name: &str) {
    assert_eq!(
        actual.location,
        expected["location"].as_str().unwrap(),
        "{name}: location mismatch"
    );
    assert_eq!(
        actual.item,
        expected["item"].as_str().unwrap(),
        "{name}: item mismatch"
    );
    assert_eq!(
        actual.quantity as u64,
        expected["quantity"].as_u64().unwrap(),
        "{name}: quantity mismatch"
    );
    assert_eq!(
        actual.per_item_cost,
        expected["per_item_cost"].as_u64().unwrap(),
        "{name}: per-item cost mismatch"
    );
    assert_eq!(
        actual.total_cost,
        expected["total_cost"].as_u64().unwrap(),
        "{name}: total cost mismatch"
    );
}

use std::{fs, path::PathBuf};
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
