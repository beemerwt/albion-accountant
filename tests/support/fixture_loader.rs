use std::{fs, path::PathBuf};

use albion_accountant::albion::protocol::protocol16::ProtocolValue;
use serde_json::{Map, Value};

pub fn load_hex_fixture(name: &str) -> Vec<u8> {
    let path = fixture_path(name);
    let raw = fs::read_to_string(path).expect("hex fixture readable");
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.len() % 2 == 0, "hex fixture must have even length");

    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("utf8 hex"), 16).expect("hex byte")
        })
        .collect()
}

pub fn load_json_fixture(name: &str) -> Value {
    let path = fixture_path(name);
    let raw = fs::read_to_string(path).expect("json fixture readable");
    serde_json::from_str(&raw).expect("valid json fixture")
}

pub fn protocol_value_to_json(value: &ProtocolValue) -> Value {
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

pub fn assert_json_golden_eq(actual: &Value, expected_fixture: &str) {
    let expected = load_json_fixture(expected_fixture);
    assert_eq!(actual, &expected, "golden snapshot mismatch for {expected_fixture}");
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
