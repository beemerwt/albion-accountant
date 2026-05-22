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

    assert!(!messages.is_empty(), "expected decodable photon messages");
    let _txs = extract_market_transactions(&messages);
    assert!(
        messages.iter().any(|m| m.payload_length > 0),
        "expected replay to produce non-empty decoded messages"
    );
}
