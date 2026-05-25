use std::collections::BTreeMap;

use super::{
    ids,
    operation_codes::OperationCodes,
    protocol::{
        commands::{AlbionCommandType, PhotonMessage},
        operations::decode_operation_payload,
        protocol16::ProtocolValue,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketRequestKind {
    AuctionBuyOffer,
    AuctionSellSpecificItemRequest,
    QuickSellAuctionSellAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedMarketRequest {
    pub kind: MarketRequestKind,
    pub order_id: u64,
    pub amount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketResponseKind {
    AuctionGetOffers,
    AuctionGetRequests,
    AuctionBuyOffer,
    AuctionSellSpecificItemRequest,
    QuickSellAuctionSellAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedMarketResponse {
    pub kind: MarketResponseKind,
    pub return_code: i16,
    pub orders: Vec<NormalizedMarketOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedMarketOrder {
    pub order_id: u64,
    pub item_type_id: String,
    pub location_id: String,
    pub unit_price_silver: u64,
}

pub fn decode_market_request(message: &PhotonMessage) -> Option<DecodedMarketRequest> {
    if AlbionCommandType::from_message_type(message.message_type)
        != AlbionCommandType::OperationRequest
    {
        return None;
    }
    let root = decode_operation_payload(&message.payload).ok()?;
    let op_code = as_u16(root.get(ids::KEY_OP_CODE)?)?;

    if op_code == OperationCodes::AuctionBuyOffer as u16 {
        return Some(DecodedMarketRequest {
            kind: MarketRequestKind::AuctionBuyOffer,
            amount: read_u32_any(&root, &["Amount", "amount", "qty", "Quantity", "1"])?,
            order_id: read_u64_any(&root, &["OrderId", "order_id", "orderId", "2"])?,
        });
    }

    if op_code == OperationCodes::AuctionSellSpecificItemRequest as u16 {
        return Some(DecodedMarketRequest {
            kind: MarketRequestKind::AuctionSellSpecificItemRequest,
            amount: read_u32_any(&root, &["Amount", "amount", "qty", "Quantity", "4"])?,
            order_id: read_u64_any(&root, &["OrderId", "order_id", "orderId", "1"])?,
        });
    }

    if op_code == OperationCodes::QuickSellAuctionSellAction as u16 {
        return Some(DecodedMarketRequest {
            kind: MarketRequestKind::QuickSellAuctionSellAction,
            amount: read_u32_any(&root, &["Amount", "amount", "qty", "Quantity", "4"])?,
            order_id: read_u64_any(&root, &["OrderId", "order_id", "orderId", "1"])?,
        });
    }

    None
}

pub fn decode_market_response(message: &PhotonMessage) -> Option<DecodedMarketResponse> {
    if AlbionCommandType::from_message_type(message.message_type)
        != AlbionCommandType::OperationResponse
    {
        return None;
    }
    let root = decode_operation_payload(&message.payload).ok()?;
    let op_code = as_u16(root.get(ids::KEY_OP_CODE)?)?;
    let return_code = as_i16(root.get(ids::KEY_RETURN_CODE)?)?;
    let params = as_map(root.get(ids::KEY_PARAMS)?)?;

    let kind = if op_code == OperationCodes::AuctionGetOffers as u16 {
        MarketResponseKind::AuctionGetOffers
    } else if op_code == OperationCodes::AuctionGetRequests as u16 {
        MarketResponseKind::AuctionGetRequests
    } else if op_code == OperationCodes::AuctionBuyOffer as u16 {
        MarketResponseKind::AuctionBuyOffer
    } else if op_code == OperationCodes::AuctionSellSpecificItemRequest as u16 {
        MarketResponseKind::AuctionSellSpecificItemRequest
    } else if op_code == OperationCodes::QuickSellAuctionSellAction as u16 {
        MarketResponseKind::QuickSellAuctionSellAction
    } else {
        return None;
    };

    let orders = match kind {
        MarketResponseKind::AuctionGetOffers | MarketResponseKind::AuctionGetRequests => {
            extract_orders_from_params(params)
        }
        _ => Vec::new(),
    };

    Some(DecodedMarketResponse {
        kind,
        return_code,
        orders,
    })
}

fn extract_orders_from_params(
    params: &BTreeMap<String, ProtocolValue>,
) -> Vec<NormalizedMarketOrder> {
    let mut out = Vec::new();
    for value in params.values() {
        let Some(arr) = as_array(value) else { continue };
        for elem in arr {
            let Some(map) = as_map(elem) else { continue };
            let Some(order_id) = read_u64_any(map, &["Id", "OrderId", "id", "orderId"]) else {
                continue;
            };
            let Some(item_type_id) = read_string_any(map, &["ItemTypeId", "item", "ItemType"])
            else {
                continue;
            };
            let Some(location_id) = read_string_any(map, &["LocationId", "location", "Location"])
            else {
                continue;
            };
            let Some(unit_price_silver) =
                read_u64_any(map, &["UnitPriceSilver", "price", "UnitPrice"])
            else {
                continue;
            };
            out.push(NormalizedMarketOrder {
                order_id,
                item_type_id,
                location_id,
                unit_price_silver,
            });
        }
    }
    out
}

fn as_map(v: &ProtocolValue) -> Option<&BTreeMap<String, ProtocolValue>> {
    match v {
        ProtocolValue::Dictionary(v) | ProtocolValue::Hashtable(v) => Some(v),
        _ => None,
    }
}

fn as_array(v: &ProtocolValue) -> Option<&Vec<ProtocolValue>> {
    match v {
        ProtocolValue::Array(v) => Some(v),
        _ => None,
    }
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

fn as_u16(v: &ProtocolValue) -> Option<u16> {
    as_u64(v).and_then(|x| u16::try_from(x).ok())
}

fn as_i16(v: &ProtocolValue) -> Option<i16> {
    match v {
        ProtocolValue::Short(v) => Some(*v),
        ProtocolValue::Int(v) => i16::try_from(*v).ok(),
        _ => None,
    }
}

fn read_u64_any(map: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|k| map.get(*k).and_then(as_u64))
        .filter(|v| *v > 0)
}

fn read_u32_any(map: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<u32> {
    read_u64_any(map, keys).and_then(|v| u32::try_from(v).ok())
}

fn read_string_any(map: &BTreeMap<String, ProtocolValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match map.get(*k) {
        Some(ProtocolValue::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_normalized_orders_from_response_params() {
        let order = BTreeMap::from([
            ("Id".to_string(), ProtocolValue::UnsignedLong(99)),
            (
                "ItemTypeId".to_string(),
                ProtocolValue::String("T4_BAG".to_string()),
            ),
            (
                "LocationId".to_string(),
                ProtocolValue::String("Bridgewatch".to_string()),
            ),
            (
                "UnitPriceSilver".to_string(),
                ProtocolValue::UnsignedLong(1234),
            ),
        ]);

        let params = BTreeMap::from([(
            "0".to_string(),
            ProtocolValue::Array(vec![ProtocolValue::Dictionary(order)]),
        )]);

        let got = extract_orders_from_params(&params);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].order_id, 99);
        assert_eq!(got[0].item_type_id, "T4_BAG");
        assert_eq!(got[0].location_id, "Bridgewatch");
        assert_eq!(got[0].unit_price_silver, 1234);
    }
}
