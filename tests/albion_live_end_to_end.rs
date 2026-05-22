mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, extract_market_transactions, extract_udp_payload},
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
