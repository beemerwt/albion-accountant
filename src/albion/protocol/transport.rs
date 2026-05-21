use super::error::DecodeError;

const MAX_FRAME_LENGTH: usize = 60 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedPayload {
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum FrameParseError {
    Incomplete {
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    Invalid(DecodeError),
}

pub fn parse_udp_payload_incremental(
    payload: &[u8],
) -> Result<Vec<FramedPayload>, FrameParseError> {
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while cursor < payload.len() {
        let header_remaining = payload.len() - cursor;
        if header_remaining < 2 {
            return Err(FrameParseError::Invalid(DecodeError::Transport {
                offset: cursor,
                reason: "trailing byte noise after frame parsing".into(),
            }));
        }

        let len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
        cursor += 2;
        if len == 0 {
            return Err(FrameParseError::Invalid(DecodeError::Transport {
                offset: cursor - 2,
                reason: "zero-length frame is not allowed".into(),
            }));
        }
        if len > MAX_FRAME_LENGTH {
            return Err(FrameParseError::Invalid(DecodeError::Transport {
                offset: cursor - 2,
                reason: format!("frame length {len} exceeds max {MAX_FRAME_LENGTH}"),
            }));
        }
        let remaining = payload.len() - cursor;
        if len > remaining {
            return Err(FrameParseError::Incomplete {
                offset: cursor,
                needed: len,
                remaining,
            });
        }

        out.push(FramedPayload {
            body: payload[cursor..cursor + len].to_vec(),
        });
        cursor += len;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_length_frame() {
        let err = parse_udp_payload_incremental(&[0, 0]).unwrap_err();
        assert!(format!("{err:?}").contains("zero-length"));
    }

    #[test]
    fn rejects_oversized_frame_length() {
        let err = parse_udp_payload_incremental(&[0xFF, 0xFF]).unwrap_err();
        assert!(format!("{err:?}").contains("exceeds max"));
    }

    #[test]
    fn rejects_trailing_noise_after_frames() {
        let payload = [0, 1, 42, 0xFF];
        let err = parse_udp_payload_incremental(&payload).unwrap_err();
        assert!(format!("{err:?}").contains("trailing byte noise"));
    }
}
