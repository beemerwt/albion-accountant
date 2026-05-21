mod support;

use albion_accountant::albion::protocol::{
    commands::decode_command_envelope, transport::parse_udp_payload,
};
use support::load_hex_fixture;

#[test]
fn parses_transport_frame_and_command_envelope() {
    let packet = load_hex_fixture("market_packet_valid.hex");
    let frames = parse_udp_payload(&packet).expect("frame should parse");
    assert_eq!(frames.len(), 1);

    let message = decode_command_envelope(&frames[0].body).expect("command envelope should parse");
    assert_eq!(message.command_type, 7);
    assert_eq!(message.channel, 0);
    assert_eq!(message.reliable_sequence, 1);
    assert_eq!(message.payload.len(), usize::from(message.payload_length));
}

#[test]
fn rejects_truncated_packet_deterministically() {
    let packet = load_hex_fixture("truncated_packet.hex");
    let err = parse_udp_payload(&packet).expect_err("must fail");
    let rendered = err.to_string();
    assert!(rendered.contains("offset 2"));
    assert!(rendered.contains("frame length 6 exceeds remaining 5"));
}
