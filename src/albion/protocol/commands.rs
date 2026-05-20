use super::error::{DecodeError, DecodeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotonMessage {
    pub command_type: u8,
    pub channel: u8,
    pub reliable_sequence: u16,
    pub payload_length: u16,
    pub payload: Vec<u8>,
}

pub fn decode_command_envelope(body: &[u8]) -> DecodeResult<PhotonMessage> {
    if body.len() < 6 {
        return Err(DecodeError::Command {
            offset: 0,
            reason: "body too short for command envelope".into(),
        });
    }

    let command_type = body[0];
    let channel = body[1];
    let reliable_sequence = u16::from_be_bytes([body[2], body[3]]);
    let payload_length = u16::from_be_bytes([body[4], body[5]]);

    let expected = 6usize + payload_length as usize;
    if body.len() < expected {
        return Err(DecodeError::Command {
            offset: 4,
            reason: format!(
                "declared payload length {payload_length} exceeds available {}",
                body.len() - 6
            ),
        });
    }

    Ok(PhotonMessage {
        command_type,
        channel,
        reliable_sequence,
        payload_length,
        payload: body[6..expected].to_vec(),
    })
}
