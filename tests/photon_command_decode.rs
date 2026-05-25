mod support;

use albion_accountant::albion::{
    decoder::{CapturePacket, extract_udp_payload},
    protocol::{commands::decode_command_envelope, transport::parse_udp_payload},
};
use support::load_pcapng_packets;

#[test]
fn parses_transport_frame_and_command_envelope_from_pcapng() {
    let packets = load_pcapng_packets("../../quick_buy_and_sell.pcapng");
    assert!(!packets.is_empty(), "pcapng must contain packets");

    let decoded = packets
        .iter()
        .filter_map(|packet| {
            let tuple = extract_udp_payload(CapturePacket {
                link_type: 1,
                packet,
            })
            .ok()?;
            let frames = parse_udp_payload(tuple.payload).ok()?;
            let message = decode_command_envelope(&frames.first()?.body).ok()?;
            Some(message)
        })
        .collect::<Vec<_>>();

    assert!(
        !decoded.is_empty(),
        "expected at least one decodable message"
    );
}

#[test]
fn malformed_transport_bytes_fail_deterministically() {
    let err = parse_udp_payload(&[0x01, 0x02, 0x03]).expect_err("must fail");
    assert!(!err.to_string().is_empty());
}
