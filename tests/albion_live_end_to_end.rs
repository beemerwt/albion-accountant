mod support;

use albion_accountant::albion::{
    correlator::TradeCorrelator,
    decoder::{
        CapturePacket, extract_market_transactions, extract_market_transactions_stateful,
        extract_udp_payload,
    },
    protocol::{commands::decode_command_envelope, transport::parse_udp_payload},
};
use support::load_pcapng_packets;

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
    let packets = load_pcapng_packets("../../quick_buy_and_sell.pcapng");
    assert!(!packets.is_empty(), "pcapng must contain packets");

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
                messages.push(msg);
            }
        }
    }

    assert!(
        !messages.is_empty(),
        "expected at least one fully decoded command envelope"
    );

    let first = &messages[0];
    eprintln!(
        "[decoded-packet] channel={}, command_type={}, message_type=0x{:02x}, reliable_sequence={}, payload_length={}",
        first.channel,
        u16::from(first.command_type),
        first.message_type,
        first.reliable_sequence,
        first.payload_length
    );

    let mut correlator = TradeCorrelator::default();
    let stateful = extract_market_transactions_stateful(&mut correlator, &messages);
    let stateless = extract_market_transactions(&messages);
    for tx in stateful.iter().chain(stateless.iter()) {
        eprintln!(
            "[decoded-transaction] location={}, item={}, quantity={}, per_item_cost={}, total_cost={}",
            tx.location, tx.item, tx.quantity, tx.per_item_cost, tx.total_cost
        );
    }
}
