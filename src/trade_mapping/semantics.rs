use std::{collections::HashMap, net::SocketAddr, sync::OnceLock, time::Instant};

use serde_json::Value;

use crate::{
    albion::{
        correlator::{MarketOrderCacheEntry, TradeCorrelator},
        event_codes::EventCodes,
        ids,
        operation_codes::OperationCodes,
        transaction::MarketTransaction,
    },
    decode_engine::types::DecodedPacket,
};

const ALBION_SERVER_PORTS: &[u16] = &[5056, 5057];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    ServerToClient,
    ClientToServer,
    Unknown,
}

impl PacketDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServerToClient => "server_to_client",
            Self::ClientToServer => "client_to_server",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticPacket {
    pub direction: &'static str,
    pub code_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeMappingOutput {
    pub semantic: SemanticPacket,
    pub transitions: Vec<TradeTransition>,
    pub finalized_rows: Vec<MarketTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeTransition {
    CachedOrder {
        order_id: u64,
        location: String,
        item: String,
        unit_price_silver: u64,
    },
    StagedBuy {
        order_id: u64,
        amount: u32,
    },
    StagedSell {
        order_id: u64,
        amount: u32,
    },
    ConfirmedBuy,
    ConfirmedSell,
    ClearedBuy,
    ClearedSell,
    EventRow {
        code_name: String,
    },
}

#[derive(Default)]
pub struct TradeSemanticMapper {
    correlator: TradeCorrelator,
}

impl TradeSemanticMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map_packet(&mut self, packet: &DecodedPacket) -> TradeMappingOutput {
        let direction = normalize_direction(&packet.direction, &packet.source, &packet.destination);
        let code_name = code_name(packet);
        let mut finalized_rows = Vec::new();
        let mut transitions = Vec::new();

        match packet_kind(packet) {
            PacketKind::OperationRequest
                if direction == PacketDirection::ClientToServer
                    || direction == PacketDirection::Unknown =>
            {
                transitions.extend(self.observe_request(packet));
            }
            PacketKind::OperationResponse
                if direction == PacketDirection::ServerToClient
                    || direction == PacketDirection::Unknown =>
            {
                let observed = self.observe_response(packet);
                transitions.extend(observed.transitions);
                finalized_rows.extend(observed.finalized_rows);
            }
            PacketKind::Event
                if direction == PacketDirection::ServerToClient
                    || direction == PacketDirection::Unknown =>
            {
                let observed = self.observe_event(packet, &code_name);
                transitions.extend(observed.transitions);
                finalized_rows.extend(observed.finalized_rows);
            }
            _ => {}
        }

        self.correlator.expire_old_state();

        TradeMappingOutput {
            semantic: SemanticPacket {
                direction: direction.as_str(),
                code_name,
            },
            transitions,
            finalized_rows,
        }
    }

    pub fn map_stream<'a>(
        &mut self,
        packets: impl IntoIterator<Item = &'a DecodedPacket>,
    ) -> Vec<MarketTransaction> {
        packets
            .into_iter()
            .flat_map(|packet| self.map_packet(packet).finalized_rows)
            .collect()
    }

    fn observe_request(&mut self, packet: &DecodedPacket) -> Vec<TradeTransition> {
        if packet.code == OperationCodes::AuctionBuyOffer as i32 {
            if let Some((order_id, amount)) = buy_request(packet) {
                self.correlator.observe_buy_request(order_id, amount);
                return vec![TradeTransition::StagedBuy { order_id, amount }];
            }
        } else if packet.code == OperationCodes::AuctionSellSpecificItemRequest as i32
            || packet.code == OperationCodes::QuickSellAuctionSellAction as i32
        {
            if let Some((order_id, amount)) = sell_request(packet) {
                self.correlator.observe_sell_request(order_id, amount);
                return vec![TradeTransition::StagedSell { order_id, amount }];
            }
        }
        Vec::new()
    }

    fn observe_response(&mut self, packet: &DecodedPacket) -> ObservedSemantics {
        match packet.code {
            x if x == OperationCodes::AuctionGetOffers as i32
                || x == OperationCodes::AuctionGetRequests as i32 =>
            {
                let orders = extract_orders(packet);
                let transitions = orders
                    .iter()
                    .map(|order| TradeTransition::CachedOrder {
                        order_id: order.order_id,
                        location: order.location.clone(),
                        item: order.item_type_id.clone(),
                        unit_price_silver: order.unit_price_silver,
                    })
                    .collect();
                self.correlator.observe_market_orders(orders);
                ObservedSemantics {
                    transitions,
                    finalized_rows: Vec::new(),
                }
            }
            x if x == OperationCodes::AuctionBuyOffer as i32 => {
                let success = is_success(packet);
                let tx = self.correlator.observe_buy_response(success);
                ObservedSemantics {
                    transitions: vec![if success {
                        TradeTransition::ConfirmedBuy
                    } else {
                        TradeTransition::ClearedBuy
                    }],
                    finalized_rows: tx.into_iter().collect(),
                }
            }
            x if x == OperationCodes::AuctionSellSpecificItemRequest as i32
                || x == OperationCodes::QuickSellAuctionSellAction as i32 =>
            {
                let success = is_success(packet);
                let tx = self.correlator.observe_sell_response(success);
                ObservedSemantics {
                    transitions: vec![if success {
                        TradeTransition::ConfirmedSell
                    } else {
                        TradeTransition::ClearedSell
                    }],
                    finalized_rows: tx.into_iter().collect(),
                }
            }
            _ => ObservedSemantics::default(),
        }
    }

    fn observe_event(&mut self, packet: &DecodedPacket, code_name: &str) -> ObservedSemantics {
        if packet.code == EventCodes::MarketPlaceNotification as i32
            || packet.code == EventCodes::MarketPlaceBuildingInfo as i32
        {
            let finalized_rows = direct_transaction(packet).into_iter().collect::<Vec<_>>();
            ObservedSemantics {
                transitions: if finalized_rows.is_empty() {
                    Vec::new()
                } else {
                    vec![TradeTransition::EventRow {
                        code_name: code_name.to_string(),
                    }]
                },
                finalized_rows,
            }
        } else {
            ObservedSemantics::default()
        }
    }
}

#[derive(Debug, Default)]
struct ObservedSemantics {
    transitions: Vec<TradeTransition>,
    finalized_rows: Vec<MarketTransaction>,
}

pub fn normalize_direction(direction: &str, source: &str, destination: &str) -> PacketDirection {
    match direction.trim().to_ascii_lowercase().as_str() {
        "server_to_client" | "server-to-client" | "s2c" => return PacketDirection::ServerToClient,
        "client_to_server" | "client-to-server" | "c2s" => return PacketDirection::ClientToServer,
        "unknown" => return PacketDirection::Unknown,
        _ => {}
    }

    let source_port = endpoint_port(source);
    let destination_port = endpoint_port(destination);
    match (
        source_port.is_some_and(|p| ALBION_SERVER_PORTS.contains(&p)),
        destination_port.is_some_and(|p| ALBION_SERVER_PORTS.contains(&p)),
    ) {
        (true, false) => PacketDirection::ServerToClient,
        (false, true) => PacketDirection::ClientToServer,
        _ => PacketDirection::Unknown,
    }
}

pub fn operation_name(code: i32) -> Option<&'static str> {
    operation_names().get(&code).copied()
}

pub fn event_name(code: i32) -> Option<&'static str> {
    event_names().get(&code).copied()
}

fn code_name(packet: &DecodedPacket) -> String {
    if !packet.name.is_empty() {
        return packet.name.clone();
    }
    match packet_kind(packet) {
        PacketKind::Event => event_name(packet.code),
        PacketKind::OperationRequest | PacketKind::OperationResponse => operation_name(packet.code),
        PacketKind::Unknown => None,
    }
    .map(ToString::to_string)
    .unwrap_or_else(|| format!("unknown_{}", packet.code))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketKind {
    OperationRequest,
    OperationResponse,
    Event,
    Unknown,
}

fn packet_kind(packet: &DecodedPacket) -> PacketKind {
    match packet.message_type.trim().to_ascii_lowercase().as_str() {
        "operation_request" | "operation request" | "request" | "2" => PacketKind::OperationRequest,
        "operation_response" | "operation response" | "response" | "3" => {
            PacketKind::OperationResponse
        }
        "event" | "event_data" | "event data" | "4" => PacketKind::Event,
        _ => {
            if packet.return_code.is_some() {
                PacketKind::OperationResponse
            } else {
                PacketKind::Unknown
            }
        }
    }
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint
        .parse::<SocketAddr>()
        .map(|addr| addr.port())
        .ok()
        .or_else(|| endpoint.rsplit_once(':')?.1.parse().ok())
}

fn buy_request(packet: &DecodedPacket) -> Option<(u64, u32)> {
    Some((
        read_u64_any(packet, &["OrderId", "order_id", "orderId", "2"], &[2])?,
        read_u32_any(packet, &["Amount", "amount", "qty", "Quantity", "1"], &[1])?,
    ))
}

fn sell_request(packet: &DecodedPacket) -> Option<(u64, u32)> {
    Some((
        read_u64_any(packet, &["OrderId", "order_id", "orderId", "1"], &[1])?,
        read_u32_any(packet, &["Amount", "amount", "qty", "Quantity", "4"], &[4])?,
    ))
}

fn is_success(packet: &DecodedPacket) -> bool {
    packet
        .return_code
        .is_some_and(|code| ids::SUCCESS_RETURN_CODES.contains(&code))
}

fn extract_orders(packet: &DecodedPacket) -> Vec<MarketOrderCacheEntry> {
    packet
        .parameters
        .values()
        .flat_map(values_from_possible_array)
        .filter_map(order_from_value)
        .collect()
}

fn order_from_value(value: &Value) -> Option<MarketOrderCacheEntry> {
    let object = value.as_object()?;
    Some(MarketOrderCacheEntry {
        order_id: read_object_u64_any(object, &["Id", "OrderId", "id", "orderId"])?,
        location: read_object_string_any(object, &["LocationId", "location", "Location"])?,
        item_type_id: read_object_string_any(object, &["ItemTypeId", "item", "ItemType"])?,
        unit_price_silver: read_object_u64_any(object, &["UnitPriceSilver", "price", "UnitPrice"])?,
        observed_at: Instant::now(),
    })
}

fn direct_transaction(packet: &DecodedPacket) -> Option<MarketTransaction> {
    let location = read_string_any(packet, &["LocationId", "location", "Location"], &[])?;
    let item = read_string_any(packet, &["ItemTypeId", "item", "ItemType"], &[])?;
    let quantity = read_u32_any(packet, &["Amount", "amount", "qty", "Quantity"], &[])?;
    let unit_price = read_u64_any(packet, &["UnitPriceSilver", "price", "UnitPrice"], &[])?;
    MarketTransaction::new(location, item, quantity, unit_price, None).ok()
}

fn values_from_possible_array(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    }
}

fn read_u64_any(packet: &DecodedPacket, names: &[&str], keys: &[i32]) -> Option<u64> {
    keys.iter()
        .find_map(|key| packet.parameters.get(key).and_then(as_u64))
        .or_else(|| {
            names
                .iter()
                .find_map(|name| packet.extracted.as_ref().and_then(|v| lookup_path(v, name)))
                .and_then(as_u64)
        })
}

fn read_u32_any(packet: &DecodedPacket, names: &[&str], keys: &[i32]) -> Option<u32> {
    read_u64_any(packet, names, keys).and_then(|v| u32::try_from(v).ok())
}

fn read_string_any(packet: &DecodedPacket, names: &[&str], keys: &[i32]) -> Option<String> {
    keys.iter()
        .find_map(|key| packet.parameters.get(key).and_then(as_string))
        .or_else(|| {
            names
                .iter()
                .find_map(|name| packet.extracted.as_ref().and_then(|v| lookup_path(v, name)))
                .and_then(as_string)
        })
}

fn read_object_u64_any(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(as_u64))
}

fn read_object_string_any(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Option<String> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(as_string))
}

fn lookup_path<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value
        .get(key)
        .or_else(|| value.get("params")?.get(key))
        .or_else(|| value.get("parameters")?.get(key))
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|v| u64::try_from(v).ok()))
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| *v > 0)
}

fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn operation_names() -> &'static HashMap<i32, &'static str> {
    static NAMES: OnceLock<HashMap<i32, &'static str>> = OnceLock::new();
    NAMES.get_or_init(|| enum_name_map(include_str!("../albion/operation_codes.rs")))
}

fn event_names() -> &'static HashMap<i32, &'static str> {
    static NAMES: OnceLock<HashMap<i32, &'static str>> = OnceLock::new();
    NAMES.get_or_init(|| enum_name_map(include_str!("../albion/event_codes.rs")))
}

fn enum_name_map(source: &'static str) -> HashMap<i32, &'static str> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.split("//").next()?.trim();
            let (name, rest) = line.split_once('=')?;
            let value = rest.trim().trim_end_matches(',').parse::<i32>().ok()?;
            Some((value, name.trim()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn packet(message_type: &str, code: i32) -> DecodedPacket {
        DecodedPacket {
            file: "fixture.pcapng".into(),
            packet_number: 1,
            direction: String::new(),
            source: "10.0.0.1:60000".into(),
            destination: "10.0.0.2:5056".into(),
            message_type: message_type.into(),
            code,
            name: String::new(),
            parameters: BTreeMap::new(),
            return_code: None,
            debug_message: String::new(),
            extracted: None,
        }
    }

    #[test]
    fn normalizes_direction_from_albion_ports() {
        assert_eq!(
            normalize_direction("", "10.0.0.2:5056", "10.0.0.1:60000").as_str(),
            "server_to_client"
        );
        assert_eq!(
            normalize_direction("", "10.0.0.1:60000", "10.0.0.2:5057").as_str(),
            "client_to_server"
        );
        assert_eq!(normalize_direction("", "a", "b").as_str(), "unknown");
    }

    #[test]
    fn enum_name_maps_are_generated_from_rust_sources() {
        assert_eq!(
            operation_name(OperationCodes::AuctionBuyOffer as i32),
            Some("AuctionBuyOffer")
        );
        assert_eq!(
            event_name(EventCodes::MarketPlaceNotification as i32),
            Some("MarketPlaceNotification")
        );
    }

    #[test]
    fn caches_listing_stages_request_and_confirms_successful_buy() {
        let mut mapper = TradeSemanticMapper::new();
        let mut listing = packet(
            "operation_response",
            OperationCodes::AuctionGetOffers as i32,
        );
        listing.direction = "server_to_client".into();
        listing.return_code = Some(0);
        listing.parameters.insert(
            0,
            json!([{
                "Id": 42,
                "ItemTypeId": "T4_BAG",
                "LocationId": "Bridgewatch",
                "UnitPriceSilver": 1200
            }]),
        );
        assert!(mapper.map_packet(&listing).finalized_rows.is_empty());

        let mut request = packet("operation_request", OperationCodes::AuctionBuyOffer as i32);
        request.parameters.insert(1, json!(3));
        request.parameters.insert(2, json!(42));
        assert!(mapper.map_packet(&request).finalized_rows.is_empty());

        let mut response = packet("operation_response", OperationCodes::AuctionBuyOffer as i32);
        response.direction = "server_to_client".into();
        response.return_code = Some(0);
        let rows = mapper.map_packet(&response).finalized_rows;

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].location, "Bridgewatch");
        assert_eq!(rows[0].item, "T4_BAG");
        assert_eq!(rows[0].quantity, 3);
        assert_eq!(rows[0].total_cost, 3600);
    }

    #[test]
    fn failed_response_clears_pending_trade() {
        let mut mapper = TradeSemanticMapper::new();
        let mut listing = packet(
            "operation_response",
            OperationCodes::AuctionGetOffers as i32,
        );
        listing.direction = "server_to_client".into();
        listing.parameters.insert(
            0,
            json!([{
                "Id": 42,
                "ItemTypeId": "T4_BAG",
                "LocationId": "Bridgewatch",
                "UnitPriceSilver": 1200
            }]),
        );
        mapper.map_packet(&listing);

        let mut request = packet("operation_request", OperationCodes::AuctionBuyOffer as i32);
        request.parameters.insert(1, json!(3));
        request.parameters.insert(2, json!(42));
        mapper.map_packet(&request);

        let mut failed = packet("operation_response", OperationCodes::AuctionBuyOffer as i32);
        failed.direction = "server_to_client".into();
        failed.return_code = Some(1);
        assert!(mapper.map_packet(&failed).finalized_rows.is_empty());

        let mut success = packet("operation_response", OperationCodes::AuctionBuyOffer as i32);
        success.direction = "server_to_client".into();
        success.return_code = Some(0);
        assert!(mapper.map_packet(&success).finalized_rows.is_empty());
    }

    #[test]
    fn market_notification_event_can_emit_direct_row() {
        let mut mapper = TradeSemanticMapper::new();
        let mut event = packet("event", EventCodes::MarketPlaceNotification as i32);
        event.direction = "server_to_client".into();
        event.extracted = Some(json!({
            "params": {
                "LocationId": "Martlock",
                "ItemTypeId": "T5_ORE",
                "Amount": 2,
                "UnitPriceSilver": 99
            }
        }));

        let rows = mapper.map_packet(&event).finalized_rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_cost, 198);
    }
}
