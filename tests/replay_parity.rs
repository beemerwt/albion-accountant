use std::{collections::BTreeMap, fs, path::PathBuf};

use albion_accountant::{
    albion::{
        event_codes::EventCodes, operation_codes::OperationCodes, transaction::MarketTransaction,
    },
    decode_engine::{photon::DecodeEngine, types::DecodedPacket},
    pcapng_adapter,
    trade_mapping::semantics::{TradeSemanticMapper, TradeTransition, event_name, operation_name},
};
use serde_json::{Value, json};

#[test]
fn quick_buy_and_sell_decoded_stream_matches_golden_summary() {
    let actual = decoded_stream_summary("../../quick_buy_and_sell.pcapng");
    assert_json_fixture("quick_buy_and_sell.decoded_summary.expected.json", actual);
}

#[test]
fn semantic_trade_flow_matches_golden_transitions_and_upload_rows() {
    let actual = semantic_trade_flow_summary();
    assert_json_fixture("semantic_trade_flow.expected.json", actual);
}

fn decoded_stream_summary(capture: &str) -> Value {
    let bytes = fs::read(fixture_path(capture)).expect("pcapng fixture readable");
    let ingress = pcapng_adapter::parse_pcapng(&bytes).expect("pcapng parses through adapter");
    let mut engine = DecodeEngine::new();

    let mut packet_statuses: BTreeMap<String, usize> = BTreeMap::new();
    let mut message_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut command_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut operation_codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut event_codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut decoded_packet_count = 0usize;
    let mut rows = Vec::new();

    for packet in &ingress {
        let result = engine.ingest_packet(packet).expect("ingress decodes");
        let status = if !result.outcome.failures.is_empty() {
            "decode_failure"
        } else if result.outcome.messages.is_empty() {
            "no_messages"
        } else {
            "decoded_messages"
        };
        *packet_statuses.entry(status.to_string()).or_default() += 1;

        for message in &result.outcome.messages {
            *message_types
                .entry(message.message_type.to_string())
                .or_default() += 1;
        }
        for diagnostic in &result.outcome.diagnostics {
            *command_kinds
                .entry(diagnostic.command_kind.to_string())
                .or_default() += 1;
        }
        for decoded in &result.decoded_packets {
            decoded_packet_count += 1;
            match decoded.message_type.as_str() {
                "operation_request" | "operation_response" => {
                    let key = format!(
                        "{}:{}",
                        decoded.code,
                        operation_name(decoded.code).unwrap_or("UnknownOperation")
                    );
                    *operation_codes.entry(key).or_default() += 1;
                }
                "event" => {
                    let key = format!(
                        "{}:{}",
                        decoded.code,
                        event_name(decoded.code).unwrap_or("UnknownEvent")
                    );
                    *event_codes.entry(key).or_default() += 1;
                }
                _ => {}
            }
        }
        rows.extend(result.transactions.iter().map(transaction_json));
    }

    json!({
        "capture": capture.trim_start_matches("../../"),
        "ingress_packets": ingress.len(),
        "packet_statuses": packet_statuses,
        "command_kinds": command_kinds,
        "message_types": message_types,
        "decoded_packets": decoded_packet_count,
        "operation_codes": operation_codes,
        "event_codes": event_codes,
        "final_transactions": rows,
    })
}

fn semantic_trade_flow_summary() -> Value {
    let mut mapper = TradeSemanticMapper::new();
    let packets = semantic_trade_packets();
    let mut semantics = Vec::new();
    let mut transitions = Vec::new();
    let mut rows = Vec::new();

    for packet in &packets {
        let output = mapper.map_packet(packet);
        semantics.push(json!({
            "direction": output.semantic.direction,
            "code_name": output.semantic.code_name,
        }));
        transitions.extend(output.transitions.iter().map(transition_json));
        rows.extend(output.finalized_rows.iter().map(transaction_json));
    }

    json!({
        "packet_semantics": semantics,
        "trade_state_transitions": transitions,
        "final_transactions": rows,
    })
}

fn semantic_trade_packets() -> Vec<DecodedPacket> {
    let mut listing = decoded_packet(
        "operation_response",
        OperationCodes::AuctionGetOffers as i32,
    );
    listing.direction = "server_to_client".to_string();
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

    let mut buy_request =
        decoded_packet("operation_request", OperationCodes::AuctionBuyOffer as i32);
    buy_request.parameters.insert(1, json!(3));
    buy_request.parameters.insert(2, json!(42));

    let mut buy_response =
        decoded_packet("operation_response", OperationCodes::AuctionBuyOffer as i32);
    buy_response.direction = "server_to_client".to_string();
    buy_response.return_code = Some(0);

    let mut sell_request = decoded_packet(
        "operation_request",
        OperationCodes::AuctionSellSpecificItemRequest as i32,
    );
    sell_request.parameters.insert(1, json!(42));
    sell_request.parameters.insert(4, json!(1));

    let mut sell_response = decoded_packet(
        "operation_response",
        OperationCodes::AuctionSellSpecificItemRequest as i32,
    );
    sell_response.direction = "server_to_client".to_string();
    sell_response.return_code = Some(1);

    let mut event = decoded_packet("event", EventCodes::MarketPlaceNotification as i32);
    event.direction = "server_to_client".to_string();
    event.extracted = Some(json!({
        "params": {
            "LocationId": "Martlock",
            "ItemTypeId": "T5_ORE",
            "Amount": 2,
            "UnitPriceSilver": 99
        }
    }));

    vec![
        listing,
        buy_request,
        buy_response,
        sell_request,
        sell_response,
        event,
    ]
}

fn decoded_packet(message_type: &str, code: i32) -> DecodedPacket {
    DecodedPacket {
        file: "semantic_trade_flow".to_string(),
        packet_number: 1,
        direction: "client_to_server".to_string(),
        source: "10.0.0.1:60000".to_string(),
        destination: "10.0.0.2:5056".to_string(),
        message_type: message_type.to_string(),
        code,
        name: String::new(),
        parameters: BTreeMap::new(),
        return_code: None,
        debug_message: String::new(),
        extracted: None,
    }
}

fn transition_json(transition: &TradeTransition) -> Value {
    match transition {
        TradeTransition::CachedOrder {
            order_id,
            location,
            item,
            unit_price_silver,
        } => json!({
            "type": "cached_order",
            "order_id": order_id,
            "location": location,
            "item": item,
            "unit_price_silver": unit_price_silver,
        }),
        TradeTransition::StagedBuy { order_id, amount } => {
            json!({"type": "staged_buy", "order_id": order_id, "amount": amount})
        }
        TradeTransition::StagedSell { order_id, amount } => {
            json!({"type": "staged_sell", "order_id": order_id, "amount": amount})
        }
        TradeTransition::ConfirmedBuy => json!({"type": "confirmed_buy"}),
        TradeTransition::ConfirmedSell => json!({"type": "confirmed_sell"}),
        TradeTransition::ClearedBuy => json!({"type": "cleared_buy"}),
        TradeTransition::ClearedSell => json!({"type": "cleared_sell"}),
        TradeTransition::EventRow { code_name } => {
            json!({"type": "event_row", "code_name": code_name})
        }
    }
}

fn transaction_json(txn: &MarketTransaction) -> Value {
    json!({
        "location": txn.location,
        "item": txn.item,
        "quantity": txn.quantity,
        "per_item_cost": txn.per_item_cost,
        "total_cost": txn.total_cost,
    })
}

fn assert_json_fixture(name: &str, actual: Value) {
    let path = fixture_path(name);
    let expected: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden fixture {}\nactual:\n{}",
            path.display(),
            serde_json::to_string_pretty(&actual).expect("actual json formats")
        )
    }))
    .expect("golden fixture is valid json");
    assert_eq!(actual, expected, "golden mismatch for {}", path.display());
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
