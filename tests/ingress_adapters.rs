use std::{fs, path::PathBuf};

use albion_accountant::{decode_engine::photon::DecodeEngine, pcapng_adapter};

#[test]
fn pcapng_adapter_emits_source_neutral_ingress_packets() {
    let bytes =
        fs::read(fixture_path("../../quick_buy_and_sell.pcapng")).expect("fixture readable");
    let ingress = pcapng_adapter::parse_pcapng(&bytes).expect("pcapng parses");

    assert!(!ingress.is_empty(), "expected UDP ingress packets");
    assert!(ingress[0].packet_number > 0);
    assert!(ingress[0].source_endpoint.contains(':'));
    assert!(ingress[0].destination_endpoint.contains(':'));
    assert!(!ingress[0].udp_payload.is_empty());
}

#[test]
fn decode_engine_accepts_ingress_packet_from_replay_adapter() {
    let bytes =
        fs::read(fixture_path("../../quick_buy_and_sell.pcapng")).expect("fixture readable");
    let ingress = pcapng_adapter::parse_pcapng(&bytes).expect("pcapng parses");
    let mut engine = DecodeEngine::new();

    let decoded = ingress
        .iter()
        .take(32)
        .map(|packet| engine.ingest_packet(packet).expect("ingress decodes"))
        .count();

    assert_eq!(decoded, 32);
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
