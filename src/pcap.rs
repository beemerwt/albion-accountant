use crate::{
    error::Result,
    util::{read_u16, read_u32},
};
use std::{
    collections::HashMap,
    fmt, fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
};

pub struct Endpoint {
    pub ip: IpAddr,
    pub port: u16,
}

impl Endpoint {
    pub fn is_albion_port(&self) -> bool {
        self.port == 5056
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.ip, self.port)
    }
}

pub struct UdpPacket<'a> {
    pub source: Endpoint,
    pub destination: Endpoint,
    pub payload: &'a [u8],
}

pub fn iter_pcapng_packets(path: &Path) -> Result<Vec<(usize, Option<u16>, Vec<u8>)>> {
    let data = fs::read(path)?;
    let mut offset = 0;
    let mut little = true;
    let mut interfaces = HashMap::new();
    let mut packet_index = 0usize;
    let mut packets = Vec::new();

    while offset + 12 <= data.len() {
        let mut block_type = read_u32(&data, offset, little)?;
        let mut block_total_length = read_u32(&data, offset + 4, little)? as usize;
        if block_total_length < 12 || offset + block_total_length > data.len() {
            little = !little;
            block_type = read_u32(&data, offset, little)?;
            block_total_length = read_u32(&data, offset + 4, little)? as usize;
            if block_total_length < 12 || offset + block_total_length > data.len() {
                return Err(format!("Invalid pcapng block at offset {offset}").into());
            }
        }
        let body = &data[offset + 8..offset + block_total_length - 4];
        match block_type {
            0x0A0D0D0A => {
                if body.starts_with(&[0x4d, 0x3c, 0x2b, 0x1a]) {
                    little = true;
                } else if body.starts_with(&[0x1a, 0x2b, 0x3c, 0x4d]) {
                    little = false;
                }
            }
            1 => {
                let link_type = read_u16(body, 0, little)?;
                interfaces.insert(interfaces.len() as u32, link_type);
            }
            6 => {
                let interface_id = read_u32(body, 0, little)?;
                let captured_length = read_u32(body, 12, little)? as usize;
                packet_index += 1;
                packets.push((
                    packet_index,
                    interfaces.get(&interface_id).copied(),
                    body[20..20 + captured_length].to_vec(),
                ));
            }
            3 => {
                let interface_id = read_u16(body, 0, little)? as u32;
                let captured_length = read_u32(body, 4, little)? as usize;
                packet_index += 1;
                packets.push((
                    packet_index,
                    interfaces.get(&interface_id).copied(),
                    body[12..12 + captured_length].to_vec(),
                ));
            }
            _ => {}
        }
        offset += block_total_length;
    }
    Ok(packets)
}

pub fn extract_udp_payload(frame: &[u8], link_type: Option<u16>) -> Option<UdpPacket<'_>> {
    if link_type != Some(1) || frame.len() < 14 {
        return None;
    }
    let mut eth_type = u16::from_be_bytes(frame[12..14].try_into().ok()?);
    let mut offset = 14;
    while matches!(eth_type, 0x8100 | 0x88A8) && frame.len() >= offset + 4 {
        eth_type = u16::from_be_bytes(frame[offset + 2..offset + 4].try_into().ok()?);
        offset += 4;
    }
    match eth_type {
        0x0800 => extract_ipv4_udp(frame, offset),
        0x86DD => extract_ipv6_udp(frame, offset),
        _ => None,
    }
}

fn extract_ipv4_udp(frame: &[u8], offset: usize) -> Option<UdpPacket<'_>> {
    if frame.len() < offset + 20 || frame[offset] >> 4 != 4 {
        return None;
    }
    let ihl = ((frame[offset] & 0x0f) * 4) as usize;
    let total_length = u16::from_be_bytes(frame[offset + 2..offset + 4].try_into().ok()?) as usize;
    if frame[offset + 9] != 17 || frame.len() < offset + total_length {
        return None;
    }
    let source_ip = Ipv4Addr::new(
        frame[offset + 12],
        frame[offset + 13],
        frame[offset + 14],
        frame[offset + 15],
    );
    let destination_ip = Ipv4Addr::new(
        frame[offset + 16],
        frame[offset + 17],
        frame[offset + 18],
        frame[offset + 19],
    );
    let udp_offset = offset + ihl;
    if frame.len() < udp_offset + 8 {
        return None;
    }
    let source_port = u16::from_be_bytes(frame[udp_offset..udp_offset + 2].try_into().ok()?);
    let destination_port =
        u16::from_be_bytes(frame[udp_offset + 2..udp_offset + 4].try_into().ok()?);
    let udp_length =
        u16::from_be_bytes(frame[udp_offset + 4..udp_offset + 6].try_into().ok()?) as usize;
    if frame.len() < udp_offset + udp_length {
        return None;
    }
    Some(UdpPacket {
        source: Endpoint {
            ip: IpAddr::V4(source_ip),
            port: source_port,
        },
        destination: Endpoint {
            ip: IpAddr::V4(destination_ip),
            port: destination_port,
        },
        payload: &frame[udp_offset + 8..udp_offset + udp_length],
    })
}

fn extract_ipv6_udp(frame: &[u8], offset: usize) -> Option<UdpPacket<'_>> {
    if frame.len() < offset + 40 || frame[offset + 6] != 17 {
        return None;
    }
    let source_ip = Ipv6Addr::from(<[u8; 16]>::try_from(&frame[offset + 8..offset + 24]).ok()?);
    let destination_ip =
        Ipv6Addr::from(<[u8; 16]>::try_from(&frame[offset + 24..offset + 40]).ok()?);
    let udp_offset = offset + 40;
    if frame.len() < udp_offset + 8 {
        return None;
    }
    let source_port = u16::from_be_bytes(frame[udp_offset..udp_offset + 2].try_into().ok()?);
    let destination_port =
        u16::from_be_bytes(frame[udp_offset + 2..udp_offset + 4].try_into().ok()?);
    let udp_length =
        u16::from_be_bytes(frame[udp_offset + 4..udp_offset + 6].try_into().ok()?) as usize;
    if frame.len() < udp_offset + udp_length {
        return None;
    }
    Some(UdpPacket {
        source: Endpoint {
            ip: IpAddr::V6(source_ip),
            port: source_port,
        },
        destination: Endpoint {
            ip: IpAddr::V6(destination_ip),
            port: destination_port,
        },
        payload: &frame[udp_offset + 8..udp_offset + udp_length],
    })
}
