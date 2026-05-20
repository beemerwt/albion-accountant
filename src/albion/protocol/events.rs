use std::collections::BTreeMap;

use super::{
    error::DecodeResult,
    protocol16::{ProtocolValue, decode_typed_value},
};

pub fn decode_event_payload(payload: &[u8]) -> DecodeResult<BTreeMap<String, ProtocolValue>> {
    let mut cursor = 0;
    let root = decode_typed_value(payload, &mut cursor)?;
    match root {
        ProtocolValue::Dictionary(map) | ProtocolValue::Hashtable(map) => Ok(map),
        other => {
            let mut map = BTreeMap::new();
            map.insert("value".to_string(), other);
            Ok(map)
        }
    }
}
