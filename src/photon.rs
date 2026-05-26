use crate::{
    error::Result,
    event_codes::EventCode,
    names,
    operation_codes::OperationCode,
    packet::DecodedPacket,
    protocol18::Protocol18Deserializer,
    requests::{
        auction_buy_offer::AuctionBuyOffer, auction_get_offers::AuctionGetOffers,
        auction_get_requests::AuctionGetRequests,
        auction_sell_specific_item::AuctionSellSpecificItem as AuctionSellSpecificItemRequest,
    },
    responses::{
        auction_get_offers::AuctionGetOffersResult,
        auction_get_requests::AuctionGetRequestsResult,
        auction_trade::{AuctionTrade, AuctionTradeResponse},
    },
    util::{params_to_json, read_i32_be, to_signed_short, value_i64},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

const COMMAND_DISCONNECT: u8 = 4;
const COMMAND_SEND_RELIABLE: u8 = 6;
const COMMAND_SEND_UNRELIABLE: u8 = 7;
const COMMAND_SEND_FRAGMENT: u8 = 8;

const MESSAGE_OPERATION_REQUEST: u8 = 2;
const MESSAGE_OPERATION_RESPONSE: u8 = 3;
const MESSAGE_EVENT: u8 = 4;

struct PendingSegment {
    payload: Vec<u8>,
    written: usize,
    total_length: usize,
}

pub struct PhotonParser {
    file_name: String,
    debug: bool,
    deserializer: Protocol18Deserializer,
    pending_segments: HashMap<i32, PendingSegment>,
    decoded_packets: Vec<DecodedPacket>,
    market_orders_by_id: HashMap<i64, Value>,
    unconfirmed_trade: Option<AuctionTrade>,
}

impl PhotonParser {
    pub fn new(file_name: String, debug: bool) -> Self {
        Self {
            file_name,
            debug,
            deserializer: Protocol18Deserializer,
            pending_segments: HashMap::new(),
            decoded_packets: Vec::new(),
            market_orders_by_id: HashMap::new(),
            unconfirmed_trade: None,
        }
    }

    pub fn decoded_packets(&self) -> &[DecodedPacket] {
        &self.decoded_packets
    }

    pub fn market_order_count(&self) -> usize {
        self.market_orders_by_id.len()
    }

    pub fn into_decoded_packets(self) -> Vec<DecodedPacket> {
        self.decoded_packets
    }

    pub fn receive_packet(
        &mut self,
        payload: &[u8],
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<&'static str> {
        if payload.len() < 12 {
            return Ok("InvalidHeader");
        }
        let flags = payload[2];
        let command_count = payload[3];
        if flags == 1 {
            return Ok("Encrypted");
        }
        let mut offset = 12;
        let mut status = "Undefined";
        for command_index in 0..command_count {
            if payload.len().saturating_sub(offset) < 12 {
                return Ok("InvalidHeader");
            }
            let result = self.handle_command(
                payload,
                offset,
                packet_number,
                command_index,
                source,
                destination,
            )?;
            status = result.0;
            offset = result.1;
            if status == "InvalidHeader" {
                return Ok(status);
            }
        }
        Ok(status)
    }

    fn handle_command(
        &mut self,
        data: &[u8],
        mut offset: usize,
        packet_number: usize,
        command_index: u8,
        source: &str,
        destination: &str,
    ) -> Result<(&'static str, usize)> {
        let command_type = data[offset];
        let command_length = read_i32_be(data, offset + 4)? - 12;
        let sequence_number = read_i32_be(data, offset + 8)?;
        offset += 12;
        if self.debug {
            eprintln!(
                "DEBUG:albion:packet={packet_number} command={command_index} type={command_type} sequence={sequence_number} payload_length={command_length}"
            );
        }
        if command_length < 0 || data.len().saturating_sub(offset) < command_length as usize {
            return Ok(("InvalidHeader", offset));
        }
        let command_length = command_length as usize;
        match command_type {
            COMMAND_DISCONNECT => Ok(("DisconnectCommand", offset + command_length)),
            COMMAND_SEND_UNRELIABLE => {
                if command_length < 4 {
                    return Ok(("InvalidHeader", offset));
                }
                self.handle_send_reliable(
                    data,
                    offset + 4,
                    command_length - 4,
                    packet_number,
                    source,
                    destination,
                )
            }
            COMMAND_SEND_RELIABLE => self.handle_send_reliable(
                data,
                offset,
                command_length,
                packet_number,
                source,
                destination,
            ),
            COMMAND_SEND_FRAGMENT => self.handle_send_fragment(
                data,
                offset,
                command_length,
                packet_number,
                source,
                destination,
            ),
            _ => Ok(("Undefined", offset + command_length)),
        }
    }

    fn handle_send_reliable(
        &mut self,
        data: &[u8],
        offset: usize,
        command_length: usize,
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<(&'static str, usize)> {
        if command_length < 2 || data.len().saturating_sub(offset) < command_length {
            return Ok(("InvalidHeader", offset));
        }
        let message_type = data[offset + 1];
        let operation_payload = &data[offset + 2..offset + command_length];
        if message_type == 131 {
            return Ok(("Encrypted", offset + command_length));
        }
        match message_type {
            MESSAGE_OPERATION_REQUEST => {
                let (_, params) = self
                    .deserializer
                    .deserialize_operation_request(operation_payload)?;
                self.record_operation(
                    "request",
                    params,
                    None,
                    "",
                    packet_number,
                    source,
                    destination,
                )?;
            }
            MESSAGE_OPERATION_RESPONSE => {
                let (_, return_code, debug_message, params) = self
                    .deserializer
                    .deserialize_operation_response(operation_payload)?;
                self.record_operation(
                    "response",
                    params,
                    Some(return_code),
                    &debug_message,
                    packet_number,
                    source,
                    destination,
                )?;
            }
            MESSAGE_EVENT => {
                let (event_code, params) = self
                    .deserializer
                    .deserialize_event_data(operation_payload)?;
                self.record_event(event_code, params, packet_number, source, destination)?;
            }
            _ => {}
        }
        Ok(("Success", offset + command_length))
    }

    fn handle_send_fragment(
        &mut self,
        data: &[u8],
        mut offset: usize,
        command_length: usize,
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<(&'static str, usize)> {
        if command_length < 20 || data.len().saturating_sub(offset) < command_length {
            return Ok(("InvalidHeader", offset));
        }
        let start_sequence_number = read_i32_be(data, offset)?;
        let total_length = read_i32_be(data, offset + 12)? as usize;
        let fragment_offset = read_i32_be(data, offset + 16)? as usize;
        offset += 20;
        let fragment_length = command_length - 20;
        let fragment = &data[offset..offset + fragment_length];

        let pending = self
            .pending_segments
            .entry(start_sequence_number)
            .or_insert_with(|| PendingSegment {
                payload: vec![0; total_length],
                written: 0,
                total_length,
            });
        pending.payload[fragment_offset..fragment_offset + fragment_length]
            .copy_from_slice(fragment);
        pending.written += fragment_length;

        if pending.written >= pending.total_length {
            let total_payload = self
                .pending_segments
                .remove(&start_sequence_number)
                .unwrap()
                .payload;
            let (status, _) = self.handle_send_reliable(
                &total_payload,
                0,
                total_payload.len(),
                packet_number,
                source,
                destination,
            )?;
            return Ok((status, offset + fragment_length));
        }

        Ok(("Success", offset + fragment_length))
    }

    fn record_operation(
        &mut self,
        packet_kind: &str,
        parameters: BTreeMap<u8, Value>,
        return_code: Option<i16>,
        debug_message: &str,
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<()> {
        let operation_code = parse_operation_code(&parameters)?;
        let operation_name = operation_code.name();
        let extracted =
            self.extract_operation(packet_kind, operation_code, &parameters, return_code);
        self.decoded_packets.push(DecodedPacket {
            file: self.file_name.clone(),
            packet_number,
            direction: direction(source, destination).to_string(),
            source: source.to_string(),
            destination: destination.to_string(),
            message_type: format!("operation_{packet_kind}"),
            code: operation_code as i32,
            name: operation_name.to_string(),
            return_code,
            debug_message: debug_message.to_string(),
            parameters: params_to_json(&parameters),
            extracted,
        });
        Ok(())
    }

    fn record_event(
        &mut self,
        photon_event_code: u8,
        mut parameters: BTreeMap<u8, Value>,
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<()> {
        if photon_event_code == EventCode::Move as u8 {
            parameters.insert(252, json!(EventCode::Move as i32));
        }
        let event_code = parse_event_code(&parameters)?;
        let event_name = names::event(event_code)
            .map(|event| event.name())
            .unwrap_or("Unknown");
        let extracted = self.extract_event(event_code, &parameters);
        self.decoded_packets.push(DecodedPacket {
            file: self.file_name.clone(),
            packet_number,
            direction: direction(source, destination).to_string(),
            source: source.to_string(),
            destination: destination.to_string(),
            message_type: "event".to_string(),
            code: event_code,
            name: event_name.to_string(),
            return_code: None,
            debug_message: String::new(),
            parameters: params_to_json(&parameters),
            extracted,
        });
        Ok(())
    }

    fn extract_operation(
        &mut self,
        packet_kind: &str,
        operation_code: OperationCode,
        parameters: &BTreeMap<u8, Value>,
        return_code: Option<i16>,
    ) -> Option<Value> {
        match (operation_code, packet_kind) {
            (OperationCode::AuctionGetOffers, "request") => {
                let orders = extract_market_orders(parameters);
                return Some(to_json_value(AuctionGetOffers {
                    market_order_count: orders.len(),
                    market_orders: orders,
                }));
            }
            (OperationCode::AuctionGetRequests, "request") => {
                let orders = extract_market_orders(parameters);
                return Some(to_json_value(AuctionGetRequests {
                    market_order_count: orders.len(),
                    market_orders: orders,
                }));
            }
            (OperationCode::AuctionGetOffers, "response") => {
                let orders = extract_market_orders(parameters);
                for order in &orders {
                    if let Some(id) = order.get("Id").and_then(value_i64) {
                        self.market_orders_by_id.insert(id, order.clone());
                    }
                }
                return Some(to_json_value(AuctionGetOffersResult {
                    market_order_count: orders.len(),
                    market_orders: orders,
                }));
            }
            (OperationCode::AuctionGetRequests, "response") => {
                let orders = extract_market_orders(parameters);
                for order in &orders {
                    if let Some(id) = order.get("Id").and_then(value_i64) {
                        self.market_orders_by_id.insert(id, order.clone());
                    }
                }
                return Some(to_json_value(AuctionGetRequestsResult {
                    market_order_count: orders.len(),
                    market_orders: orders,
                }));
            }
            (OperationCode::AuctionBuyOffer, "request") => {
                let amount = parameters.get(&1).and_then(value_i64);
                let order_id = parameters.get(&2).and_then(value_i64);
                let cached_order =
                    order_id.and_then(|id| self.market_orders_by_id.get(&id).cloned());
                let cached_order = cached_order.unwrap_or(Value::Null);
                let request = AuctionBuyOffer {
                    amount,
                    cached_order: cached_order.clone(),
                    order_id,
                };
                self.unconfirmed_trade = Some(AuctionTrade {
                    amount,
                    operation: "buy",
                    order: cached_order,
                    order_id,
                });
                return Some(to_json_value(request));
            }
            (OperationCode::AuctionSellSpecificItem, "request") => {
                let amount = parameters.get(&4).and_then(value_i64);
                let order_id = parameters.get(&1).and_then(value_i64);
                let cached_order =
                    order_id.and_then(|id| self.market_orders_by_id.get(&id).cloned());
                let cached_order = cached_order.unwrap_or(Value::Null);
                let request = AuctionSellSpecificItemRequest {
                    amount,
                    cached_order: cached_order.clone(),
                    order_id,
                };
                self.unconfirmed_trade = Some(AuctionTrade {
                    amount,
                    operation: "sell",
                    order: cached_order,
                    order_id,
                });
                return Some(to_json_value(request));
            }
            (
                OperationCode::AuctionBuyOffer | OperationCode::AuctionSellSpecificItem,
                "response",
            ) => {
                let success = return_code == Some(0);
                let response = AuctionTradeResponse {
                    confirmed_trade: success.then(|| self.unconfirmed_trade.clone()).flatten(),
                    success,
                };
                self.unconfirmed_trade = None;
                return Some(to_json_value(response));
            }
            _ => {}
        }

        None
    }

    fn extract_event(&self, event_code: i32, parameters: &BTreeMap<u8, Value>) -> Option<Value> {
        if event_code == EventCode::MarketPlaceNotification as i32 {
            return Some(
                json!({"notification": parameters.get(&0).cloned().unwrap_or(Value::Null)}),
            );
        }
        None
    }
}

fn to_json_value(value: impl Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

fn parse_operation_code(params: &BTreeMap<u8, Value>) -> Result<OperationCode> {
    let value = params
        .get(&253)
        .and_then(value_i64)
        .ok_or("Operation code parameter 253 is missing")?;
    let code = to_signed_short(value);
    names::operation(code)
        .ok_or_else(|| format!("Unknown operation code in parameter 253: {code}").into())
}

fn parse_event_code(params: &BTreeMap<u8, Value>) -> Result<i32> {
    let value = params
        .get(&252)
        .and_then(value_i64)
        .ok_or("Event code parameter 252 is missing")?;
    let code = to_signed_short(value);
    if names::event(code).is_some() {
        return Ok(code);
    }
    let unsigned_value = (code as i64 & 0xffff) as i32;
    let shifted = unsigned_value >> 4;
    if (unsigned_value & 0x0f) == 0x01 && names::event(shifted).is_some() {
        return Ok(shifted);
    }
    Err(format!("Unknown event code in parameter 252: {code}").into())
}

fn extract_market_orders(params: &BTreeMap<u8, Value>) -> Vec<Value> {
    let Some(raw_orders) = params.get(&0) else {
        return Vec::new();
    };
    let values: Vec<Value> = match raw_orders {
        Value::Array(items) => items.clone(),
        item => vec![item.clone()],
    };
    values
        .into_iter()
        .filter_map(|value| match value {
            Value::String(text) => serde_json::from_str(&text).ok(),
            Value::Object(_) => Some(value),
            _ => None,
        })
        .collect()
}

fn direction(source: &str, destination: &str) -> &'static str {
    if source.ends_with(":5056") {
        "server_to_client"
    } else if destination.ends_with(":5056") {
        "client_to_server"
    } else {
        "unknown"
    }
}
