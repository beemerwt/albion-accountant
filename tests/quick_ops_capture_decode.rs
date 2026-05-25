mod support;

use albion_accountant::albion::{
    correlator::{MarketOrderCacheEntry, TradeCorrelator, TradeSide},
    decoder::{CapturePacket, extract_udp_payload},
    market_decode::{decode_market_request, decode_market_response, MarketRequestKind, MarketResponseKind},
    protocol::{commands::decode_command_envelope, transport::parse_udp_payload},
};
use support::load_pcapng_packets;

#[derive(Debug)]
struct Diagnostic {
    pcapng_filename: String,
    packet_count_read: usize,
    udp_packet_count: usize,
    albion_candidate_packet_count: usize,
    decoded_message_count: usize,
    observed_requests: Vec<String>,
    observed_responses: Vec<String>,
    correlator_input_records: Vec<String>,
    correlator_rejected_records: Vec<String>,
    final_stage: String,
}

fn run_capture(path: &str) -> (Diagnostic, Vec<(TradeSide, albion_accountant::albion::transaction::MarketTransaction)>) {
    let packets = load_pcapng_packets(path);
    let mut diag = Diagnostic {
        pcapng_filename: path.to_string(),
        packet_count_read: packets.len(),
        udp_packet_count: 0,
        albion_candidate_packet_count: 0,
        decoded_message_count: 0,
        observed_requests: vec![],
        observed_responses: vec![],
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
