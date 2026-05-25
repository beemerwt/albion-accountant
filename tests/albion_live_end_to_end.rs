mod support;

use albion_accountant::albion::{
    correlator::TradeCorrelator,
    decoder::{
        CapturePacket, DecodeProbe, extract_market_transactions, extract_market_transactions_stateful,
        extract_udp_payload, probe_message,
    },
    protocol::{
        commands::decode_command_envelope, events::decode_event_payload,
        operations::decode_operation_payload, transport::parse_udp_payload,
    },
};
use support::{load_hex_fixture, load_pcapng_packets};

#[test]
fn pcapng_covers_full_market_pipeline() {
    let packets = load_pcapng_packets("../../quick_buy_and_sell.pcapng");
    assert!(!packets.is_empty(), "pcapng must contain packets");

    let mut messages = Vec::new();
    let mut udp_payloads = 0usize;
    let mut transport_frames = 0usize;
    for packet in &packets {
        let Ok(tuple) = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet,
        }) else {
            continue;
        };
        udp_payloads += 1;

        let Ok(frames) = parse_udp_payload(tuple.payload) else {
            continue;
        };
        transport_frames += frames.len();
        for frame in frames {
            if let Ok(msg) = decode_command_envelope(&frame.body) {
                messages.push(msg);
            }
        }
    }

    eprintln!(
        "[debug] pipeline stats: packets={}, udp_payloads={}, transport_frames={}, decoded_messages={}",
        packets.len(),
        udp_payloads,
        transport_frames,
        messages.len()
    );

    assert!(!messages.is_empty(), "expected decodable photon messages");
    let _txs = extract_market_transactions(&messages);
    assert!(
        messages.iter().any(|m| m.payload_length > 0),
        "expected replay to produce non-empty decoded messages"
    );
}

#[test]
fn pcapng_replay_emits_completed_market_transactions() {
    #[derive(Default)]
    struct CaptureCounts {
        envelopes: usize,
        type2: usize,
        type3: usize,
        type4: usize,
        event_decoded: usize,
        op_decoded: usize,
    }

    let mut all_messages = Vec::new();
    let mut total_counts = CaptureCounts::default();
    let mut replay_decoded_events = 0usize;
    let mut replay_decoded_operations = 0usize;

    for capture in [
        "../../quick_buy_and_sell.pcapng",
        "../../full_market_quick_buy.pcapng",
        "../../full_market_quick_sell.pcapng",
    ] {
        let packets = load_pcapng_packets(capture);
        assert!(!packets.is_empty(), "pcapng must contain packets: {capture}");

        let mut messages = Vec::new();
        let mut counts = CaptureCounts::default();
        for packet in &packets {
            let Ok(tuple) = extract_udp_payload(CapturePacket {
                link_type: 1,
                packet,
            }) else {
                continue;
            };

            let Ok(frames) = parse_udp_payload(tuple.payload) else {
                continue;
            };

            for frame in frames {
                if let Ok(msg) = decode_command_envelope(&frame.body) {
                    counts.envelopes += 1;
                    messages.push(msg);
                }
            }
        }

        assert!(
            !messages.is_empty(),
            "expected at least one fully decoded command envelope in {capture}"
        );
        all_messages.extend(messages.clone());

        // Stage A: pcap transport/envelope accounting.
        for message in &messages {
            match message.message_type {
                0x02 => counts.type2 += 1,
                0x03 => counts.type3 += 1,
                0x04 => counts.type4 += 1,
                _ => {}
            }
        }

        let capture_candidates = counts.type2 + counts.type3 + counts.type4;
        assert!(
            capture_candidates > 0,
            "Stage A failed for {capture}: expected at least one protocol message candidate (types 0x02/0x03/0x04), got envelopes={}, type2={}, type3={}, type4={}",
            counts.envelopes,
            counts.type2,
            counts.type3,
            counts.type4
        );

        // Stage B: protocol payload decode from replay candidate messages only.
        for message in messages
            .iter()
            .filter(|m| matches!(m.message_type, 0x02 | 0x03 | 0x04))
        {
            match probe_message(message) {
                DecodeProbe::EventDecoded {
                    ..
                } => {
                    counts.event_decoded += 1;
                    replay_decoded_events += 1;
                }
                DecodeProbe::OperationDecoded {
                    ..
                } => {
                    counts.op_decoded += 1;
                    replay_decoded_operations += 1;
                }
                DecodeProbe::UnsupportedCommandType { .. }
                | DecodeProbe::EventDecodeFailed
                | DecodeProbe::OperationDecodeFailed => {}
            }
        }

        total_counts.envelopes += counts.envelopes;
        total_counts.type2 += counts.type2;
        total_counts.type3 += counts.type3;
        total_counts.type4 += counts.type4;
        total_counts.event_decoded += counts.event_decoded;
        total_counts.op_decoded += counts.op_decoded;

        eprintln!(
            "[capture-summary] capture={}, envelopes={}, type2={}, type3={}, type4={}, event_decoded={}, op_decoded={}",
            capture,
            counts.envelopes,
            counts.type2,
            counts.type3,
            counts.type4,
            counts.event_decoded,
            counts.op_decoded
        );
    }

    assert!(
        total_counts.envelopes > 0,
        "Stage A failed: expected replay captures to produce at least one valid command envelope"
    );
    assert!(
        replay_decoded_events + replay_decoded_operations > 0,
        "Stage B failed: no EventDecoded/OperationDecoded from replay candidates. totals: envelopes={}, type2={}, type3={}, type4={}, event_decoded={}, op_decoded={}",
        total_counts.envelopes,
        total_counts.type2,
        total_counts.type3,
        total_counts.type4,
        total_counts.event_decoded,
        total_counts.op_decoded
    );

    // Stage C: fixture-only parser validation (kept separate from replay counters).
    let fixture = load_hex_fixture("market_packet_valid.hex");
    let fixture_event_decoded = decode_event_payload(&fixture).is_ok();
    let fixture_op_decoded = decode_operation_payload(&fixture).is_ok();
    if fixture_event_decoded {
        eprintln!(
            "[fixture-summary] fixture=market_packet_valid.hex, event_decoded=true, op_decoded={}",
            fixture_op_decoded
        );
    }
    if !fixture_event_decoded {
        eprintln!(
            "[fixture-summary] fixture=market_packet_valid.hex, event_decoded=false, op_decoded={}",
            fixture_op_decoded
        );
    }

    eprintln!(
        "[replay-summary] envelopes={}, type2={}, type3={}, type4={}, event_decoded={}, op_decoded={}",
        total_counts.envelopes,
        total_counts.type2,
        total_counts.type3,
        total_counts.type4,
        total_counts.event_decoded,
        total_counts.op_decoded
    );

    let mut correlator = TradeCorrelator::default();
    let stateful = extract_market_transactions_stateful(&mut correlator, &all_messages);
    let stateless = extract_market_transactions(&all_messages);
    for tx in stateful.iter().chain(stateless.iter()) {
        eprintln!(
            "[decoded-transaction] location={}, item={}, quantity={}, per_item_cost={}, total_cost={}",
            tx.location, tx.item, tx.quantity, tx.per_item_cost, tx.total_cost
        );
    }
}
