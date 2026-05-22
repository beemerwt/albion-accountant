mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, extract_udp_payload},
    protocol::{commands::decode_command_envelope, transport::parse_udp_payload},
};
use support::load_pcapng_packets;

#[test]
fn replay_pcapng_packets_across_decode_stages() {
    let packets = load_pcapng_packets("../../quick_buy_and_sell.pcapng");
    assert!(!packets.is_empty());

    let mut udp_payload_count = 0usize;
    let mut frame_count = 0usize;
    let mut envelope_count = 0usize;

    for packet in &packets {
        let Ok(tuple) = extract_udp_payload(CapturePacket {
            link_type: 1,
            packet,
        }) else {
            continue;
        };
        udp_payload_count += 1;
        if let Ok(frames) = parse_udp_payload(tuple.payload) {
            frame_count += frames.len();
            for frame in frames {
                if decode_command_envelope(&frame.body).is_ok() {
                    envelope_count += 1;
                }
            }
        }
    }

    eprintln!(
        "[debug] replay stats: packets={}, udp_payloads={}, transport_frames={}, decoded_envelopes={}",
        packets.len(),
        udp_payload_count,
        frame_count,
        envelope_count
    );

    assert!(frame_count > 0, "expected parsed transport frames");
    assert!(envelope_count > 0, "expected decoded command envelopes");
}
