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
fn decodes_all_live_packet_tags_found_in_fixtures() {
    let bytes = load_hex_fixture("market_packet_valid.hex");
    assert!(bytes.contains(&b'd'));
    assert!(bytes.contains(&b'b'));
    assert!(bytes.contains(&b't'));
    assert!(bytes.contains(&b'i'));

    let bytes = load_hex_fixture("protocol16_complex.hex");
    assert!(bytes.contains(&b'd'));
    assert!(bytes.contains(&b'a'));
    assert!(bytes.contains(&b'o'));
    assert!(bytes.contains(&b'b'));
}

#[test]
fn decodes_each_newly_supported_type_tag() {
    let fixtures = [
        ("protocol16_tag_unsigned_byte.hex", ProtocolValue::UnsignedByte(250)),
        ("protocol16_tag_unsigned_short.hex", ProtocolValue::UnsignedShort(50000)),
        ("protocol16_tag_unsigned_int.hex", ProtocolValue::UnsignedInt(3_000_000_000)),
        ("protocol16_tag_unsigned_long.hex", ProtocolValue::UnsignedLong(9_000_000_000)),
        ("protocol16_tag_float.hex", ProtocolValue::Float(3.5)),
        ("protocol16_tag_double.hex", ProtocolValue::Double(42.25)),
        ("protocol16_tag_byte_array.hex", ProtocolValue::ByteArray(vec![0xde, 0xad, 0xbe, 0xef])),
        (
            "protocol16_tag_custom_wrapper.hex",
            ProtocolValue::Custom(7, Box::new(ProtocolValue::String("hi".to_string()))),
        ),
        (
            "protocol16_tag_object_wrapper.hex",
            ProtocolValue::Object(Box::new(ProtocolValue::Int(123))),
        ),
    ];

    for (name, expected) in fixtures {
        let bytes = load_hex_fixture(name);
        let mut cursor = 0usize;
        let actual = decode_typed_value(&bytes, &mut cursor).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(actual, expected, "fixture {name}");
        assert_eq!(cursor, bytes.len(), "cursor accounting for {name}");
    }
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

#[test]
fn rejects_truncated_or_malformed_new_types() {
    for name in [
        "protocol16_bad_unsigned_short_truncated.hex",
        "protocol16_bad_unsigned_int_truncated.hex",
        "protocol16_bad_unsigned_long_truncated.hex",
        "protocol16_bad_float_truncated.hex",
        "protocol16_bad_double_truncated.hex",
        "protocol16_bad_byte_array_length.hex",
        "protocol16_bad_custom_missing_wrapped.hex",
        "protocol16_bad_object_missing_wrapped.hex",
    ] {
        let bytes = load_hex_fixture(name);
        let mut cursor = 0usize;
        decode_typed_value(&bytes, &mut cursor).expect_err(name);
    }
}
