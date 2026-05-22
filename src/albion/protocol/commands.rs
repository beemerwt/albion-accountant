use super::error::{DecodeError, DecodeResult};

pub const MESSAGE_TYPE_OPERATION_REQUEST: u8 = 2;
pub const MESSAGE_TYPE_OPERATION_RESPONSE: u8 = 3;
pub const MESSAGE_TYPE_EVENT: u8 = 4;
pub const COMMAND_TYPE_DISCONNECT: u16 = 4;
pub const COMMAND_TYPE_UNRELIABLE: u16 = 7;
pub const COMMAND_TYPE_RELIABLE: u16 = 6;
pub const COMMAND_TYPE_FRAGMENT: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbionCommandType {
    Reliable,
    Unreliable,
    Fragment,
    Disconnect,
    OperationRequest,
    Event,
    OperationResponse,
    Unsupported(u16),
}

impl From<u16> for AlbionCommandType {
    fn from(value: u16) -> Self {
        match value {
            COMMAND_TYPE_RELIABLE => Self::Reliable,
            COMMAND_TYPE_UNRELIABLE => Self::Unreliable,
            COMMAND_TYPE_FRAGMENT => Self::Fragment,
            COMMAND_TYPE_DISCONNECT => Self::Disconnect,
            _ => Self::Unsupported(value),
        }
    }
}

impl AlbionCommandType {
    pub fn from_message_type(value: u8) -> Self {
        match value {
            MESSAGE_TYPE_OPERATION_REQUEST => Self::OperationRequest,
            MESSAGE_TYPE_OPERATION_RESPONSE => Self::OperationResponse,
            MESSAGE_TYPE_EVENT => Self::Event,
            other => Self::Unsupported(u16::from(other)),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AlbionCommandType::Reliable => "reliable",
            AlbionCommandType::Unreliable => "unreliable",
            AlbionCommandType::Fragment => "fragment",
            AlbionCommandType::Disconnect => "disconnect",
            AlbionCommandType::OperationRequest => "operation_request",
            AlbionCommandType::Event => "event",
            AlbionCommandType::OperationResponse => "operation_response",
            AlbionCommandType::Unsupported(_) => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotonMessage {
    pub command_type: u16,
    pub channel: u8,
    pub command_flags: u8,
    pub reliable_sequence: u16,
    pub signal_byte: u8,
    pub message_type: u8,
    pub payload_length: u16,
    pub payload: Vec<u8>,
}

pub fn decode_command_envelope(body: &[u8]) -> DecodeResult<PhotonMessage> {
    if body.len() < 8 {
        return Err(DecodeError::Command {
            offset: 0,
            reason: "body too short for command envelope".into(),
        });
    }

    let command_type = u16::from(body[0]);
    let channel = body[1];
    let command_flags = body[2];
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

    if payload_length < 2 {
        return Err(DecodeError::Command {
            offset: 5,
            reason: "payload too short for signal/message type".into(),
        });
    }
    let signal_byte = body[7];
    let message_type = body[8];

    Ok(PhotonMessage {
        command_type,
        channel,
        command_flags,
        reliable_sequence,
        signal_byte,
        message_type,
        payload_length,
        payload: body[9..expected].to_vec(),
    })
}
