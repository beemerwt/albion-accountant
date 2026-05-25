#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngressPacket {
    pub packet_number: usize,
    pub source_endpoint: String,
    pub destination_endpoint: String,
    pub udp_payload: Vec<u8>,
}
