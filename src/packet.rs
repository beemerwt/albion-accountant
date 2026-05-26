use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
pub struct DecodedPacket {
    pub file: String,
    pub packet_number: usize,
    pub direction: String,
    pub source: String,
    pub destination: String,
    pub message_type: String,
    pub code: i32,
    pub name: String,
    pub return_code: Option<i16>,
    pub debug_message: String,
    pub parameters: BTreeMap<String, Value>,
    pub extracted: Option<Value>,
}
