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

    for capture in [
        "../../quick_buy_and_sell.pcapng",
        "../../full_market_quick_buy.pcapng",
        "../../full_market_quick_sell.pcapng",
    ] {
        let packets = load_pcapng_packets(capture);
        assert!(!packets.is_empty(), "pcapng must contain packets: {capture}");

        let mut messages = Vec::new();
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
                    valid_envelopes += 1;
                    eprintln!(
                        "[validated-envelope] capture={}, command_type={}, channel={}, reliable_sequence={}",
                        capture,
                        u16::from(msg.command_type),
                        msg.channel,
                        msg.reliable_sequence
                    );
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

        for message in &messages {
            *message_type_histogram.entry(message.message_type).or_insert(0) += 1;

            if !matches!(message.message_type, 0x02 | 0x03 | 0x04) {
                unsupported_message_type_count += 1;
                continue;
            }
            payload_candidates += 1;

            match probe_message(message) {
                DecodeProbe::EventDecoded {
                    code,
                    key_count,
                    encrypted_like,
                    ..
                } => {
                    decoded_events += 1;
                    eprintln!(
                        "[decoded-event] capture={}, channel={}, command_type={}, message_type=0x{:02x}, reliable_sequence={}, payload_length={}, event_code={}, param_keys={}, encrypted_like={}",
                        capture,
                        message.channel,
                        u16::from(message.command_type),
                        message.message_type,
                        message.reliable_sequence,
                        message.payload_length,
                        code,
                        key_count,
                        encrypted_like
                    );
                }
                DecodeProbe::OperationDecoded {
                    op_code,
                    return_code,
                    key_count,
                    encrypted_like,
                    ..
                } => {
                    decoded_operations += 1;
                    eprintln!(
                        "[decoded-operation] capture={}, channel={}, command_type={}, message_type=0x{:02x}, reliable_sequence={}, payload_length={}, op_code={}, return_code={}, param_keys={}, encrypted_like={}",
                        capture,
                        message.channel,
                        u16::from(message.command_type),
                        message.message_type,
                        message.reliable_sequence,
                        message.payload_length,
                        op_code,
                        return_code,
                        key_count,
                        encrypted_like
                    );
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
    }

    assert!(
        valid_envelopes > 0,
        "expected replay captures to produce at least one valid command envelope"
    );

    let fixture = load_hex_fixture("market_packet_valid.hex");
    if let Ok(event_map) = decode_event_payload(&fixture) {
        decoded_events += 1;
        eprintln!(
            "[fixture-decoded-event] fixture=market_packet_valid.hex, event_top_level_keys={}",
            event_map.len()
        );
    }
    if let Ok(op_map) = decode_operation_payload(&fixture) {
        decoded_operations += 1;
        eprintln!(
            "[fixture-decoded-operation] fixture=market_packet_valid.hex, operation_top_level_keys={}",
            op_map.len()
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
        "[payload-validation-summary] candidates={}, decoded_events={}, decoded_operations={}",
        payload_candidates, decoded_events, decoded_operations
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
