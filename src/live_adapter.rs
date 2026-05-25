use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::ingress::IngressPacket;

pub fn adapt_packet(
    packet_number: usize,
    link_type: i32,
    packet: &[u8],
) -> Result<IngressPacket, UdpExtractDropReason> {
    let tuple = extract_udp_payload(link_type, packet)?;
    Ok(IngressPacket {
        packet_number,
        source_endpoint: SocketAddr::new(tuple.src_ip, tuple.src_port).to_string(),
        destination_endpoint: SocketAddr::new(tuple.dst_ip, tuple.dst_port).to_string(),
        udp_payload: tuple.payload.to_vec(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpTuple<'a> {
    pub payload: &'a [u8],
    pub src_ip: IpAddr,
    pub src_port: u16,
    pub dst_ip: IpAddr,
    pub dst_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdpExtractDropReason {
    UnsupportedLinkType,
    TruncatedL2,
    UnsupportedEtherType,
    TruncatedIpv4,
    TruncatedIpv6,
    NonUdp,
    TruncatedUdp,
}

fn extract_udp_payload(
    link_type: i32,
    packet: &[u8],
) -> Result<UdpTuple<'_>, UdpExtractDropReason> {
    let (l3_start, ether_type) = match link_type {
        1 => {
            if packet.len() < 14 {
                return Err(UdpExtractDropReason::TruncatedL2);
            }
            (14usize, u16::from_be_bytes([packet[12], packet[13]]))
        }
        113 => {
            if packet.len() < 16 {
                return Err(UdpExtractDropReason::TruncatedL2);
            }
            (16usize, u16::from_be_bytes([packet[14], packet[15]]))
        }
        _ => return Err(UdpExtractDropReason::UnsupportedLinkType),
    };

    match ether_type {
        0x0800 => extract_udp_ipv4(packet, l3_start),
        0x86DD => extract_udp_ipv6(packet, l3_start),
        _ => Err(UdpExtractDropReason::UnsupportedEtherType),
    }
}

fn extract_udp_ipv4(packet: &[u8], ip_start: usize) -> Result<UdpTuple<'_>, UdpExtractDropReason> {
    if packet.len() < ip_start + 20 {
        return Err(UdpExtractDropReason::TruncatedIpv4);
    }
    let ihl = (packet[ip_start] & 0x0f) as usize * 4;
    if ihl < 20 || packet.len() < ip_start + ihl {
        return Err(UdpExtractDropReason::TruncatedIpv4);
    }
    let proto = packet[ip_start + 9];
    if proto != 17 {
        return Err(UdpExtractDropReason::NonUdp);
    }
    let src_ip = IpAddr::V4(Ipv4Addr::new(
        packet[ip_start + 12],
        packet[ip_start + 13],
        packet[ip_start + 14],
        packet[ip_start + 15],
    ));
    let dst_ip = IpAddr::V4(Ipv4Addr::new(
        packet[ip_start + 16],
        packet[ip_start + 17],
        packet[ip_start + 18],
        packet[ip_start + 19],
    ));
    extract_udp_common(packet, ip_start + ihl, src_ip, dst_ip)
}

fn extract_udp_ipv6(packet: &[u8], ip_start: usize) -> Result<UdpTuple<'_>, UdpExtractDropReason> {
    if packet.len() < ip_start + 40 {
        return Err(UdpExtractDropReason::TruncatedIpv6);
    }
    if packet[ip_start + 6] != 17 {
        return Err(UdpExtractDropReason::NonUdp);
    }
    let src_ip = IpAddr::V6(Ipv6Addr::from(
        <[u8; 16]>::try_from(&packet[ip_start + 8..ip_start + 24]).expect("checked len"),
    ));
    let dst_ip = IpAddr::V6(Ipv6Addr::from(
        <[u8; 16]>::try_from(&packet[ip_start + 24..ip_start + 40]).expect("checked len"),
    ));
    extract_udp_common(packet, ip_start + 40, src_ip, dst_ip)
}

fn extract_udp_common(
    packet: &[u8],
    udp_start: usize,
    src_ip: IpAddr,
    dst_ip: IpAddr,
) -> Result<UdpTuple<'_>, UdpExtractDropReason> {
    if packet.len() < udp_start + 8 {
        return Err(UdpExtractDropReason::TruncatedUdp);
    }
    let src_port = u16::from_be_bytes([packet[udp_start], packet[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([packet[udp_start + 2], packet[udp_start + 3]]);
    let udp_len = u16::from_be_bytes([packet[udp_start + 4], packet[udp_start + 5]]) as usize;
    if udp_len < 8 || packet.len() < udp_start + udp_len {
        return Err(UdpExtractDropReason::TruncatedUdp);
    }
    Ok(UdpTuple {
        payload: &packet[udp_start + 8..udp_start + udp_len],
        src_ip,
        src_port,
        dst_ip,
        dst_port,
    })
}
