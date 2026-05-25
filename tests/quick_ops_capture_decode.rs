mod support;

use albion_accountant::albion::{
    correlator::{MarketOrderCacheEntry, TradeCorrelator, TradeSide},
    decoder::{CapturePacket, extract_udp_payload},
    market_decode::{decode_market_request, decode_market_response, MarketRequestKind, MarketResponseKind},
    protocol::{commands::decode_command_envelope, transport::parse_udp_payload},
};
use support::load_pcapng_packets;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use albion_accountant::albion::{ids, protocol::{events::decode_event_payload, operations::decode_operation_payload, protocol16::ProtocolValue}};

#[derive(Debug)]
struct Diagnostic {
    pcapng_filename: String,
    packet_count_read: usize,
    udp_packet_count: usize,
    albion_candidate_packet_count: usize,
    decoded_message_count: usize,
    observed_requests: Vec<String>,
    observed_responses: Vec<String>,
    observed_operation_codes: Vec<String>,
    observed_event_codes: Vec<String>,
    correlator_input_records: Vec<String>,
    correlator_rejected_records: Vec<String>,
    final_stage: String,
}


fn parse_enum_code_map(path: &str) -> BTreeMap<u16, String> {
    let mut out = BTreeMap::new();
    let Ok(content) = fs::read_to_string(path) else { return out };
    for raw in content.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() || !line.contains('=') {
            continue;
        }
        let mut parts = line.split('=');
        let lhs = parts.next().unwrap_or("").trim().trim_end_matches(',');
        let rhs = parts.next().unwrap_or("").trim().trim_end_matches(',');
        if lhs.is_empty() { continue; }
        if let Ok(code) = rhs.parse::<u16>() {
            out.insert(code, lhs.to_string());
        }
    }
    out
}

fn as_u16(value: &ProtocolValue) -> Option<u16> {
    match value {
        ProtocolValue::Byte(v) | ProtocolValue::UnsignedByte(v) => Some(u16::from(*v)),
        ProtocolValue::Short(v) => u16::try_from(*v).ok(),
        ProtocolValue::UnsignedShort(v) => Some(*v),
        ProtocolValue::Int(v) => u16::try_from(*v).ok(),
        ProtocolValue::UnsignedInt(v) => u16::try_from(*v).ok(),
        ProtocolValue::Long(v) => u16::try_from(*v).ok(),
        ProtocolValue::UnsignedLong(v) => u16::try_from(*v).ok(),
        _ => None,
    }
}

fn run_capture(path: &str) -> (Diagnostic, Vec<(TradeSide, albion_accountant::albion::transaction::MarketTransaction)>) {
    let packets = load_pcapng_packets(path);
    let op_map = parse_enum_code_map("src/albion/operation_codes.rs");
    let event_map = parse_enum_code_map("src/albion/event_codes.rs");
    let mut seen_ops = BTreeSet::new();
    let mut seen_events = BTreeSet::new();
    let mut diag = Diagnostic {
        pcapng_filename: path.to_string(),
        packet_count_read: packets.len(),
        udp_packet_count: 0,
        albion_candidate_packet_count: 0,
        decoded_message_count: 0,
        observed_requests: vec![],
        observed_responses: vec![],
        observed_operation_codes: vec![],
        observed_event_codes: vec![],
        correlator_input_records: vec![],
        correlator_rejected_records: vec![],
        final_stage: "pcap read".to_string(),
    };
    let mut correlator = TradeCorrelator::default();
    let mut out = Vec::new();

    for packet in &packets {
        let Ok(tuple) = extract_udp_payload(CapturePacket { link_type: 1, packet }) else { continue; };
        diag.udp_packet_count += 1;
        diag.final_stage = "UDP extraction".to_string();

        let Ok(frames) = parse_udp_payload(tuple.payload) else { continue; };
        diag.albion_candidate_packet_count += 1;
        diag.final_stage = "Photon framing".to_string();

        for frame in frames {
            let Ok(msg) = decode_command_envelope(&frame.body) else { continue; };
            diag.decoded_message_count += 1;
            diag.final_stage = "message decoding".to_string();

            if let Ok(root) = decode_operation_payload(&msg.payload) {
                if let Some(code) = root.get(ids::KEY_OP_CODE).and_then(as_u16) {
                    let name = op_map.get(&code).cloned().unwrap_or_else(|| format!("UnknownOperationCode({code})"));
                    let key = format!("{code}:{name}");
                    if seen_ops.insert(key.clone()) {
                        diag.observed_operation_codes.push(key);
                    }
                }
            }
            if let Ok(root) = decode_event_payload(&msg.payload) {
                if let Some(code) = root.get(ids::KEY_EVENT_CODE).and_then(as_u16) {
                    let name = event_map.get(&code).cloned().unwrap_or_else(|| format!("UnknownEventCode({code})"));
                    let key = format!("{code}:{name}");
                    if seen_events.insert(key.clone()) {
                        diag.observed_event_codes.push(key);
                    }
                }
            }

            if let Some(resp) = decode_market_response(&msg) {
                diag.observed_responses.push(format!("{:?}/rc={}", resp.kind, resp.return_code));
                match resp.kind {
                    MarketResponseKind::AuctionGetOffers | MarketResponseKind::AuctionGetRequests => {
                        for o in resp.orders {
                            correlator.observe_market_orders([MarketOrderCacheEntry {
                                order_id: o.order_id,
                                location: o.location_id,
                                item_type_id: o.item_type_id,
                                unit_price_silver: o.unit_price_silver,
                                observed_at: std::time::Instant::now(),
                            }]);
                        }
                    }
                    MarketResponseKind::AuctionBuyOffer => {
                        if let Some(tx) = correlator.observe_buy_response(resp.return_code == 0) { out.push((TradeSide::Buy, tx)); }
                    }
                    MarketResponseKind::AuctionSellSpecificItemRequest | MarketResponseKind::QuickSellAuctionSellAction => {
                        if let Some(tx) = correlator.observe_sell_response(resp.return_code == 0) { out.push((TradeSide::Sell, tx)); }
                    }
                }
            }

            if let Some(req) = decode_market_request(&msg) {
                diag.observed_requests.push(format!("{:?}/order={}/amount={}", req.kind, req.order_id, req.amount));
                match req.kind {
                    MarketRequestKind::AuctionBuyOffer => {
                        if correlator.has_cached_order(req.order_id) {
                            correlator.observe_buy_request(req.order_id, req.amount);
                            diag.correlator_input_records.push(format!("buy pending {} {}", req.order_id, req.amount));
                        } else {
                            diag.correlator_rejected_records.push(format!("buy missing cache {}", req.order_id));
                        }
                    }
                    MarketRequestKind::AuctionSellSpecificItemRequest | MarketRequestKind::QuickSellAuctionSellAction => {
                        if correlator.has_cached_order(req.order_id) {
                            correlator.observe_sell_request(req.order_id, req.amount);
                            diag.correlator_input_records.push(format!("sell pending {} {}", req.order_id, req.amount));
                        } else {
                            diag.correlator_rejected_records.push(format!("sell missing cache {}", req.order_id));
                        }
                    }
                }
            }
        }
    }

    diag.final_stage = if out.is_empty() { "correlation".into() } else { "final operation construction".into() };
    (diag, out)
}

#[test]
fn decodes_full_market_quick_buy_capture_as_complete_quick_buy_operation() {
    let (diag, out) = run_capture("../../full_market_quick_buy.pcapng");
    let has_buy = out.iter().any(|(side, _)| *side == TradeSide::Buy);
    assert!(has_buy, "{}", format!("{:#?}", diag));
}

#[test]
fn decodes_full_market_quick_sell_capture_as_complete_quick_sell_operation() {
    let (diag, out) = run_capture("../../full_market_quick_sell.pcapng");
    let has_sell = out.iter().any(|(side, _)| *side == TradeSide::Sell);
    assert!(has_sell, "{}", format!("{:#?}", diag));
}
