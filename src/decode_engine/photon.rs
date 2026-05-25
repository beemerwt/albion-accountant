use crate::decode_engine::{
    protocol18::Protocol18Deserializer,
    types::{CommandStatus, DecodedPacket},
};
use anyhow::Result;
use std::collections::HashMap;
pub struct DecodeEngine {
    deserializer: Protocol18Deserializer,
    pub pending_segments: HashMap<i32, Vec<u8>>,
    pub decoded_packets: Vec<DecodedPacket>,
}
impl DecodeEngine {
    pub fn new() -> Self {
        Self {
            deserializer: Protocol18Deserializer::new(),
            pending_segments: HashMap::new(),
            decoded_packets: vec![],
        }
    }
    pub fn ingest_udp_payload(
        &mut self,
        payload: &[u8],
        packet_number: usize,
        source: &str,
        destination: &str,
    ) -> Result<CommandStatus> {
        if payload.len() < 12 {
            return Ok(CommandStatus::InvalidHeader);
        };
        let flags = payload[2];
        let cc = payload[3] as usize;
        if flags == 1 {
            return Ok(CommandStatus::Encrypted);
        };
        let mut off = 12usize;
        let mut status = CommandStatus::Undefined;
        for _ in 0..cc {
            if payload.len() < off + 12 {
                return Ok(CommandStatus::InvalidHeader);
            };
            let ct = payload[off];
            let clen = i32::from_be_bytes(payload[off + 4..off + 8].try_into().unwrap()) - 12;
            off += 12;
            if clen < 0 || payload.len() < off + clen as usize {
                return Ok(CommandStatus::InvalidHeader);
            };
            status = match ct {
                4 => CommandStatus::DisconnectCommand,
                6 | 7 => {
                    let (mut o, mut l) = (off, clen as usize);
                    if ct == 7 {
                        o += 4;
                        l = l.saturating_sub(4)
                    };
                    self.handle_send_reliable(
                        &payload[o..o + l],
                        packet_number,
                        source,
                        destination,
                    )?;
                    CommandStatus::Success
                }
                8 => {
                    self.pending_segments
                        .insert(0, payload[off..off + clen as usize].to_vec());
                    CommandStatus::Success
                }
                _ => CommandStatus::Undefined,
            };
            off += clen as usize;
        }
        Ok(status)
    }
    fn handle_send_reliable(
        &mut self,
        payload: &[u8],
        _n: usize,
        _s: &str,
        _d: &str,
    ) -> Result<()> {
        if payload.len() < 2 {
            return Ok(());
        }
        let mt = payload[1];
        let op = &payload[2..];
        match mt {
            2 => {
                let _ = self.deserializer.deserialize_operation_request(op)?;
            }
            3 => {
                let _ = self.deserializer.deserialize_operation_response(op)?;
            }
            4 => {
                let _ = self.deserializer.deserialize_event_data(op)?;
            }
            131 => {}
            _ => {}
        };
        Ok(())
    }
}
