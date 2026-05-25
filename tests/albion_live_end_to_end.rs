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
    use std::collections::BTreeMap;

    let mut decoded_events = 0usize;
    let mut decoded_operations = 0usize;
    let mut all_messages = Vec::new();
    let mut valid_envelopes = 0usize;
    let mut payload_candidates = 0usize;
    let mut replay_debug_summaries: Vec<(String, usize, usize, usize, BTreeMap<u8, usize>)> = Vec::new();
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
        let mut unsupported_message_type_count = 0usize;
        let mut event_decode_failed_count = 0usize;
        let mut operation_decode_failed_count = 0usize;
        let mut message_type_histogram: BTreeMap<u8, usize> = BTreeMap::new();

        // Stage A: pcap transport/envelope accounting.
        for message in &messages {
            *message_type_histogram.entry(message.message_type).or_insert(0) += 1;

            if !matches!(message.message_type, 0x02 | 0x03 | 0x04) {
                unsupported_message_type_count += 1;
                continue;
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
                DecodeProbe::UnsupportedCommandType { .. } => {
                    unsupported_message_type_count += 1;
                }
                DecodeProbe::EventDecodeFailed => {
                    event_decode_failed_count += 1;
                    if message.message_type == 0x04 {
                        if let Ok(event_map) = decode_event_payload(&message.payload) {
                            decoded_events += 1;
                            eprintln!(
                                "[decoded-event-direct] capture={}, channel={}, command_type={}, message_type=0x{:02x}, reliable_sequence={}, payload_length={}, event_top_level_keys={}",
                                capture,
                                message.channel,
                                u16::from(message.command_type),
                                message.message_type,
                                message.reliable_sequence,
                                message.payload_length,
                                event_map.len()
                            );
                        }
                    } else if let Ok(op_map) = decode_operation_payload(&message.payload) {
                        operation_decode_failed_count += 1;
                        decoded_operations += 1;
                        eprintln!(
                            "[decoded-operation-direct] capture={}, channel={}, command_type={}, message_type=0x{:02x}, reliable_sequence={}, payload_length={}, operation_top_level_keys={}",
                            capture,
                            message.channel,
                            u16::from(message.command_type),
                            message.message_type,
                            message.reliable_sequence,
                            message.payload_length,
                            op_map.len()
                        );
                    } else {
                        operation_decode_failed_count += 1;
                    }
                }
                DecodeProbe::OperationDecodeFailed => {
                    operation_decode_failed_count += 1;
                }
            }
        }

        replay_debug_summaries.push((
            capture.to_string(),
            unsupported_message_type_count,
            event_decode_failed_count,
            operation_decode_failed_count,
            message_type_histogram,
        ));
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

    for (
        capture,
        unsupported_message_type_count,
        event_decode_failed_count,
        operation_decode_failed_count,
        message_type_histogram,
    ) in &replay_debug_summaries
    {
        eprintln!(
            "[replay-decode-debug] capture={}, unsupported_message_type_count={}, event_decode_failed_count={}, operation_decode_failed_count={}, message_type_histogram={:?}",
            capture,
            unsupported_message_type_count,
            event_decode_failed_count,
            operation_decode_failed_count,
            message_type_histogram
        );
    }

    assert!(
        payload_candidates > 0,
        "expected replay captures to include payload-candidate messages (types 0x02/0x03/0x04)"
    );

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
