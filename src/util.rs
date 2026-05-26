use crate::error::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn params_to_json(params: &BTreeMap<u8, Value>) -> BTreeMap<String, Value> {
    params
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

pub fn value_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(value) => Some(i64::from(*value)),
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

pub fn to_signed_short(value: i64) -> i32 {
    let mut value = (value & 0xffff) as i32;
    if value >= 0x8000 {
        value -= 0x10000;
    }
    value
}

pub fn json_key(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

pub fn bytes_value(bytes: &[u8]) -> Value {
    json!({"bytes_hex": hex_lower(bytes)})
}

pub fn read_u16(data: &[u8], offset: usize, little: bool) -> Result<u16> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or("Unexpected end of data")?
        .try_into()
        .unwrap();
    Ok(if little {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

pub fn read_u32(data: &[u8], offset: usize, little: bool) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or("Unexpected end of data")?
        .try_into()
        .unwrap();
    Ok(if little {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

pub fn read_i32_be(data: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or("Unexpected end of data")?
            .try_into()
            .unwrap(),
    ))
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hex_upper(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
