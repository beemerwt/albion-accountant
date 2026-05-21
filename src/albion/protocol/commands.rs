use super::error::{DecodeError, DecodeResult};

pub const COMMAND_TYPE_OPERATION_RESPONSE: u16 = 3;
pub const COMMAND_TYPE_EVENT: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbionCommandType {
    Event,
    OperationResponse,
    Unsupported(u16),
}

impl From<u16> for AlbionCommandType {
    fn from(value: u16) -> Self {
        match value {
            COMMAND_TYPE_EVENT => Self::Event,
            COMMAND_TYPE_OPERATION_RESPONSE => Self::OperationResponse,
            other => Self::Unsupported(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotonMessage {
    pub command_type: u16,
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

    let command_type = u16::from_be_bytes([body[0], body[1]]);
    let channel = body[2];
    let reliable_sequence = u16::from_be_bytes([body[3], body[4]]);
    let payload_length = u16::from_be_bytes([body[5], body[6]]);

    let expected = 7usize + payload_length as usize;
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
