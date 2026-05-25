use anyhow::{Context, Result, bail};

use crate::{ingress::IngressPacket, live_adapter};

const SECTION_HEADER_BLOCK: u32 = 0x0A0D0D0A;
const INTERFACE_DESCRIPTION_BLOCK: u32 = 0x00000001;
const ENHANCED_PACKET_BLOCK: u32 = 0x00000006;
const SIMPLE_PACKET_BLOCK: u32 = 0x00000003;
const BYTE_ORDER_MAGIC: u32 = 0x1A2B3C4D;
const BYTE_ORDER_MAGIC_SWAPPED: u32 = 0x4D3C2B1A;

#[derive(Debug, Clone, Copy)]
enum Endian {
    Little,
    Big,
}

pub fn parse_pcapng(bytes: &[u8]) -> Result<Vec<IngressPacket>> {
    let mut parser = Parser {
        bytes,
        endian: Endian::Little,
        interfaces: Vec::new(),
        packet_number: 0,
        ingress_packets: Vec::new(),
    };
    parser.parse()?;
    Ok(parser.ingress_packets)
}

struct Parser<'a> {
    bytes: &'a [u8],
    endian: Endian,
    interfaces: Vec<i32>,
    packet_number: usize,
    ingress_packets: Vec<IngressPacket>,
}

impl Parser<'_> {
    fn parse(&mut self) -> Result<()> {
        let mut offset = 0usize;
        while offset + 12 <= self.bytes.len() {
            let block_type = self.read_u32_at(offset)?;
            if block_type == SECTION_HEADER_BLOCK {
                self.set_section_endian(offset)?;
            }
            let block_len = self.read_u32_at(offset + 4)? as usize;
            if block_len < 12 {
                bail!("pcapng block at offset {offset} has invalid length {block_len}");
            }
            let block_end = offset
                .checked_add(block_len)
                .context("pcapng block length overflow")?;
            if block_end > self.bytes.len() {
                bail!("pcapng block at offset {offset} extends past end of file");
            }
            let trailing_len = self.read_u32_at(block_end - 4)? as usize;
            if trailing_len != block_len {
                bail!("pcapng block at offset {offset} has mismatched trailing length");
            }

            match block_type {
                INTERFACE_DESCRIPTION_BLOCK => {
                    self.parse_interface_description(offset, block_len)?
                }
                ENHANCED_PACKET_BLOCK => self.parse_enhanced_packet(offset, block_len)?,
                SIMPLE_PACKET_BLOCK => self.parse_simple_packet(offset, block_len)?,
                _ => {}
            }

            offset = block_end;
        }
        Ok(())
    }

    fn set_section_endian(&mut self, offset: usize) -> Result<()> {
        let magic = self
            .bytes
            .get(offset + 8..offset + 12)
            .context("truncated pcapng section header")?;
        let little = u32::from_le_bytes(magic.try_into().expect("checked len"));
        let big = u32::from_be_bytes(magic.try_into().expect("checked len"));
        self.endian = if little == BYTE_ORDER_MAGIC {
            Endian::Little
        } else if big == BYTE_ORDER_MAGIC {
            Endian::Big
        } else if little == BYTE_ORDER_MAGIC_SWAPPED {
            Endian::Big
        } else {
            bail!("pcapng section header has invalid byte-order magic");
        };
        self.interfaces.clear();
        Ok(())
    }

    fn parse_interface_description(&mut self, offset: usize, block_len: usize) -> Result<()> {
        if block_len < 20 {
            bail!("pcapng interface block at offset {offset} is truncated");
        }
        let link_type = self.read_u16_at(offset + 8)? as i32;
        self.interfaces.push(link_type);
        Ok(())
    }

    fn parse_enhanced_packet(&mut self, offset: usize, block_len: usize) -> Result<()> {
        if block_len < 32 {
            bail!("pcapng enhanced packet block at offset {offset} is truncated");
        }
        let interface_id = self.read_u32_at(offset + 8)? as usize;
        let captured_len = self.read_u32_at(offset + 20)? as usize;
        let data_start = offset + 28;
        self.parse_packet_data(offset, block_len, interface_id, captured_len, data_start)
    }

    fn parse_simple_packet(&mut self, offset: usize, block_len: usize) -> Result<()> {
        if block_len < 16 {
            bail!("pcapng simple packet block at offset {offset} is truncated");
        }
        let packet_len = self.read_u32_at(offset + 8)? as usize;
        let captured_len = packet_len.min(block_len.saturating_sub(16));
        self.parse_packet_data(offset, block_len, 0, captured_len, offset + 12)
    }

    fn parse_packet_data(
        &mut self,
        offset: usize,
        block_len: usize,
        interface_id: usize,
        captured_len: usize,
        data_start: usize,
    ) -> Result<()> {
        let data_end = data_start
            .checked_add(captured_len)
            .context("pcapng packet length overflow")?;
        let block_payload_end = offset + block_len - 4;
        if data_end > block_payload_end {
            bail!("pcapng packet block at offset {offset} has truncated packet data");
        }
        self.packet_number = self.packet_number.wrapping_add(1);
        let link_type = self.interfaces.get(interface_id).copied().unwrap_or(1);
        if let Ok(packet) = live_adapter::adapt_packet(
            self.packet_number,
            link_type,
            &self.bytes[data_start..data_end],
        ) {
            self.ingress_packets.push(packet);
        }
        Ok(())
    }

    fn read_u16_at(&self, offset: usize) -> Result<u16> {
        let raw = self
            .bytes
            .get(offset..offset + 2)
            .context("truncated pcapng u16")?;
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(raw.try_into().expect("checked len")),
            Endian::Big => u16::from_be_bytes(raw.try_into().expect("checked len")),
        })
    }

    fn read_u32_at(&self, offset: usize) -> Result<u32> {
        let raw = self
            .bytes
            .get(offset..offset + 4)
            .context("truncated pcapng u32")?;
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(raw.try_into().expect("checked len")),
            Endian::Big => u32::from_be_bytes(raw.try_into().expect("checked len")),
        })
    }
}
