use std::collections::BTreeMap;

use super::{ids, protocol::protocol16::ProtocolValue, transaction::MarketTransaction};

#[derive(Debug, Clone)]
pub struct DecodedEvent {
    pub code: u8,
    pub params: BTreeMap<String, ProtocolValue>,
}

#[derive(Debug, Clone)]
pub struct DecodedOperationResponse {
    pub op_code: u8,
    pub return_code: i16,
    pub params: BTreeMap<String, ProtocolValue>,
}

pub fn map_event_to_transaction(event: &DecodedEvent) -> Option<MarketTransaction> {
    if !ids::MARKET_EVENT_CODES.contains(&event.code) {
        return None;
    }
    build_transaction(&event.params)
}

pub fn map_response_to_transaction(
    response: &DecodedOperationResponse,
) -> Option<MarketTransaction> {
    if !ids::MARKET_OPERATION_CODES.contains(&response.op_code)
        || !ids::SUCCESS_RETURN_CODES.contains(&response.return_code)
    {
        return None;
    }
    build_transaction(&response.params)
}

fn build_transaction(params: &BTreeMap<String, ProtocolValue>) -> Option<MarketTransaction> {
    let location = read_string_any(params, ids::LOCATION_KEYS)?;
    let item = read_string_any(params, ids::ITEM_ID_KEYS)?;
    let quantity = read_u32_any(params, ids::QUANTITY_KEYS)?;
    let per_item_cost = read_u64_any(params, ids::SILVER_KEYS)?;
    MarketTransaction::new(location, item, quantity, per_item_cost, None).ok()
}

fn read_string_any(params: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match params.get(*k)? {
        ProtocolValue::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    })
}

fn read_u32_any(params: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|k| {
        as_u64(params.get(*k)?)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
    })
}

fn read_u64_any(params: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|k| as_u64(params.get(*k)?))
        .filter(|v| *v > 0)
}

fn as_u64(v: &ProtocolValue) -> Option<u64> {
    match v {
        ProtocolValue::Byte(x) => Some(u64::from(*x)),
        ProtocolValue::Short(x) => u64::try_from(*x).ok(),
        ProtocolValue::Int(x) => u64::try_from(*x).ok(),
        ProtocolValue::Long(x) => u64::try_from(*x).ok(),
        ProtocolValue::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}
