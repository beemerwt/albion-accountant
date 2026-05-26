use crate::error::Result;
use albion_network_lib::{
    DecodedPacket, HostFilter, PhotonParser, extract_udp_payload, iter_pcapng_packets,
};
use std::path::Path;

pub fn process_capture(path: &Path, debug: bool) -> Result<Vec<DecodedPacket>> {
    eprintln!("INFO:albion:processing {}", path.display());
    let host_filter = HostFilter::from_file(Path::new("hosts.txt"))?;
    eprintln!(
        "INFO:albion:loaded {} allowed host ranges from hosts.txt",
        host_filter.len()
    );
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let mut parser = PhotonParser::new(file_name, debug);
    let mut udp_packets = 0usize;
    let mut photon_packets = 0usize;
    for (packet_number, link_type, frame) in iter_pcapng_packets(path)? {
        let Some(packet) = extract_udp_payload(&frame, link_type) else {
            continue;
        };
        udp_packets += 1;
        if !(packet.source.is_albion_port() || packet.destination.is_albion_port()) {
            continue;
        }
        if !(host_filter.contains(packet.source.ip) || host_filter.contains(packet.destination.ip))
        {
            continue;
        }
        photon_packets += 1;
        parser.receive_packet(
            packet.payload,
            packet_number,
            &packet.source.to_string(),
            &packet.destination.to_string(),
        )?;
    }
    eprintln!(
        "INFO:albion:{} complete: udp_packets={} photon_packets={} decoded_messages={} cached_market_orders={}",
        path.display(),
        udp_packets,
        photon_packets,
        parser.decoded_packets().len(),
        parser.market_order_count()
    );
    Ok(parser.into_decoded_packets())
}
