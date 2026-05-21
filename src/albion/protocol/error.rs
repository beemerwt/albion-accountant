use thiserror::Error;

pub type DecodeResult<T> = Result<T, DecodeError>;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("transport framing error at offset {offset}: {reason}")]
    Transport { offset: usize, reason: String },
    #[error("command envelope error at offset {offset}: {reason}")]
    Command { offset: usize, reason: String },
    #[error("protocol16 decode error at offset {offset}: {reason}")]
    Protocol16 { offset: usize, reason: String },
}
