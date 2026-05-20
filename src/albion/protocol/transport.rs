use super::error::{DecodeError, DecodeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedPayload {
    pub body: Vec<u8>,
}

pub fn parse_udp_payload(payload: &[u8]) -> DecodeResult<Vec<FramedPayload>> {
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while cursor + 2 <= payload.len() {
        let len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
        cursor += 2;
        if cursor + len > payload.len() {
            return Err(DecodeError::Transport {
                offset: cursor,
                reason: format!(
                    "frame length {len} exceeds remaining {}",
                    payload.len() - cursor
                ),
            });
        }
        out.push(FramedPayload {
            body: payload[cursor..cursor + len].to_vec(),
        });
        cursor += len;
    }

    if cursor != payload.len() {
        return Err(DecodeError::Transport {
            offset: cursor,
            reason: "trailing bytes after frame parsing".into(),
        });
    }

    Ok(out)
}
