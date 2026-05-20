mod support {
    pub mod fixture_loader;
}

use albion_accountant::albion::protocol::protocol16::{decode_typed_value, ProtocolValue};
use support::fixture_loader::{assert_json_golden_eq, load_hex_fixture, protocol_value_to_json};

#[test]
fn decodes_typed_container_fixture_and_matches_golden_snapshot() {
    let bytes = load_hex_fixture("protocol16_complex.hex");
    let mut cursor = 0usize;
    let value = decode_typed_value(&bytes, &mut cursor).expect("fixture decodes");
    assert_eq!(cursor, bytes.len());

    let json = protocol_value_to_json(&value);
    assert_json_golden_eq(&json, "protocol16_complex.expected.json");
}

#[test]
fn rejects_unknown_type_tag_deterministically() {
    let bytes = load_hex_fixture("unknown_type_tag.hex");
    let mut cursor = 0usize;
    let err = decode_typed_value(&bytes, &mut cursor).expect_err("must fail");
    let rendered = err.to_string();
    assert!(rendered.contains("offset 0"));
    assert!(rendered.contains("unknown type tag 'z'"));
}

#[test]
fn rejects_bad_string_length_deterministically() {
    let bytes = load_hex_fixture("bad_string_length.hex");
    let mut cursor = 0usize;
    let err = decode_typed_value(&bytes, &mut cursor).expect_err("must fail");
    let rendered = err.to_string();
    assert!(rendered.contains("offset 3"));
    assert!(rendered.contains("string length 5 exceeds available 2"));
}
