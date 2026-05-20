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
    let location = read_required_string(
        params,
        ids::LOCATION_KEY,
        ids::LOCATION_KEY_ALIASES,
        "location",
    )?;
    let item = read_required_string(params, ids::ITEM_ID_KEY, ids::ITEM_ID_KEY_ALIASES, "item")?;
    let quantity = read_required_u32(
        params,
        ids::QUANTITY_KEY,
        ids::QUANTITY_KEY_ALIASES,
        "qty",
    )?;
    let per_item_cost =
        read_required_u64(params, ids::SILVER_KEY, ids::SILVER_KEY_ALIASES, "price")?;
    MarketTransaction::new(location, item, quantity, per_item_cost, None).ok()
}

fn read_required_string(
    params: &BTreeMap<String, ProtocolValue>,
    canonical: &str,
    aliases: &[&str],
    label: &str,
) -> Option<String> {
    read_string(params, canonical).or_else(|| {
        aliases.iter().find_map(|k| read_string(params, k)).or_else(|| {
            tracing::debug!(field = label, canonical, aliases = ?aliases, "missing required field");
            None
        })
    })
}

fn read_required_u32(
    params: &BTreeMap<String, ProtocolValue>,
    canonical: &str,
    aliases: &[&str],
    label: &str,
) -> Option<u32> {
    read_u32(params, canonical).or_else(|| {
        aliases.iter().find_map(|k| read_u32(params, k)).or_else(|| {
            tracing::debug!(field = label, canonical, aliases = ?aliases, "missing required field");
            None
        })
    })
}

fn read_required_u64(
    params: &BTreeMap<String, ProtocolValue>,
    canonical: &str,
    aliases: &[&str],
    label: &str,
) -> Option<u64> {
    read_u64(params, canonical).or_else(|| {
        aliases.iter().find_map(|k| read_u64(params, k)).or_else(|| {
            tracing::debug!(field = label, canonical, aliases = ?aliases, "missing required field");
            None
        })
    })
}

fn read_string(params: &BTreeMap<String, ProtocolValue>, key: &str) -> Option<String> {
    match params.get(key)? {
        ProtocolValue::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn read_u32(params: &BTreeMap<String, ProtocolValue>, key: &str) -> Option<u32> {
    as_u64(params.get(key)?)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
}

fn read_u64(params: &BTreeMap<String, ProtocolValue>, key: &str) -> Option<u64> {
    as_u64(params.get(key)?).filter(|v| *v > 0)
}

fn as_u64(v: &ProtocolValue) -> Option<u64> {
    match v {
        ProtocolValue::UnsignedByte(x) | ProtocolValue::Byte(x) => Some(u64::from(*x)),
        ProtocolValue::UnsignedShort(x) => Some(u64::from(*x)),
        ProtocolValue::Short(x) => u64::try_from(*x).ok(),
        ProtocolValue::UnsignedInt(x) => Some(u64::from(*x)),
        ProtocolValue::Int(x) => u64::try_from(*x).ok(),
        ProtocolValue::UnsignedLong(x) => Some(*x),
        ProtocolValue::Long(x) => u64::try_from(*x).ok(),
        ProtocolValue::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}
