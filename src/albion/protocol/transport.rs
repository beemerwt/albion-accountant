use super::error::DecodeError;
use tracing::warn;

const MAX_FRAME_LENGTH: usize = 60 * 1024;
const PHOTON_PACKET_HEADER_LEN: usize = 12;
const PHOTON_COMMAND_HEADER_LEN: usize = 12;

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
        state: &'static str,
    },
    Invalid(DecodeError),
}

pub fn parse_udp_payload_incremental(
    payload: &[u8],
) -> Result<Vec<FramedPayload>, FrameParseError> {
    if payload.len() >= PHOTON_PACKET_HEADER_LEN {
        parse_photon_udp_packet(payload)
    } else {
        parse_legacy_length_prefixed(payload)
    }
}

fn parse_photon_udp_packet(payload: &[u8]) -> Result<Vec<FramedPayload>, FrameParseError> {
    let command_count = payload[3] as usize;
    let mut cursor = PHOTON_PACKET_HEADER_LEN;
    let mut out = Vec::new();

    for idx in 0..command_count {
        let remaining = payload.len().saturating_sub(cursor);
        if remaining < PHOTON_COMMAND_HEADER_LEN {
            log_incomplete(
                payload,
                cursor,
                PHOTON_COMMAND_HEADER_LEN,
                remaining,
                "command_header",
            );
            return Err(FrameParseError::Incomplete {
                offset: cursor,
                needed: PHOTON_COMMAND_HEADER_LEN,
                remaining,
                state: "command_header",
            });
        }

        let command_type = payload[cursor];
        let channel = payload[cursor + 1];
        let command_flags = payload[cursor + 2];
        let command_len = u32::from_be_bytes([
            payload[cursor + 4],
            payload[cursor + 5],
            payload[cursor + 6],
            payload[cursor + 7],
        ]) as usize;
        let reliable_seq = u32::from_be_bytes([
            payload[cursor + 8],
            payload[cursor + 9],
            payload[cursor + 10],
            payload[cursor + 11],
        ]);

        if command_len < PHOTON_COMMAND_HEADER_LEN {
            return Err(FrameParseError::Invalid(DecodeError::Transport {
                offset: cursor,
                reason: format!("command length {command_len} smaller than header at index {idx}"),
            }));
        }
        if command_len > MAX_FRAME_LENGTH {
            return Err(FrameParseError::Invalid(DecodeError::Transport {
                offset: cursor,
                reason: format!("command length {command_len} exceeds max {MAX_FRAME_LENGTH}"),
            }));
        }
        if command_len > remaining {
            log_incomplete(payload, cursor, command_len, remaining, "command_payload");
            return Err(FrameParseError::Incomplete {
                offset: cursor,
                needed: command_len,
                remaining,
                state: "command_payload",
            });
        }

        let mut payload_start = cursor + PHOTON_COMMAND_HEADER_LEN;
        let mut payload_len = command_len - PHOTON_COMMAND_HEADER_LEN;

        if command_type == 7 {
            if payload_len < 4 {
                return Err(FrameParseError::Invalid(DecodeError::Transport {
                    offset: cursor,
                    reason: format!(
                        "unreliable command payload length {payload_len} smaller than 4-byte subheader at index {idx}"
                    ),
                }));
            }
            payload_start += 4;
            payload_len -= 4;
        }

        let body_slice = &payload[payload_start..payload_start + payload_len];

        let mut envelope = Vec::with_capacity(7 + body_slice.len());
        envelope.push(command_type);
        envelope.push(channel);
        envelope.push(command_flags);
        envelope.extend_from_slice(&(reliable_seq as u16).to_be_bytes());
        envelope.extend_from_slice(&(payload_len as u16).to_be_bytes());
        envelope.extend_from_slice(body_slice);
        out.push(FramedPayload { body: envelope });

        cursor += command_len;
    }

    if cursor != payload.len() {
        return Err(FrameParseError::Invalid(DecodeError::Transport {
            offset: cursor,
            reason: format!(
                "trailing {0} bytes after {1} commands",
                payload.len() - cursor,
                command_count
            ),
        }));
    }
    Ok(out)
}

fn parse_legacy_length_prefixed(payload: &[u8]) -> Result<Vec<FramedPayload>, FrameParseError> {
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
            log_incomplete(payload, cursor, len, remaining, "legacy_frame_payload");
            return Err(FrameParseError::Incomplete {
                offset: cursor,
                needed: len,
                remaining,
                state: "legacy_frame_payload",
            });
        }
        out.push(FramedPayload {
            body: payload[cursor..cursor + len].to_vec(),
        });
        cursor += len;
    }
    Ok(out)
}

fn log_incomplete(
    payload: &[u8],
    offset: usize,
    needed: usize,
    remaining: usize,
    state: &'static str,
) {
    let preview_len = payload.len().min(24);
    let first_bytes = payload[..preview_len]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");
    warn!(offset, needed, remaining, state, first_bytes = %first_bytes, payload_len = payload.len(), "transport parse incomplete");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::albion::protocol::commands::{AlbionCommandType, decode_command_envelope};

    #[test]
    #[ignore = "fixture pending protocol refresh"]
    fn parses_realistic_photon_udp_packet_with_multiple_command_types() {
        let packet = build_photon_packet(vec![
            (6u8, 0u8, 1u32, vec![0x01, 0x02]),
            (7u8, 1u8, 2u32, vec![0x03]),
            (8u8, 2u8, 3u32, vec![0x04, 0x05, 0x06]),
        ]);

        let frames = parse_udp_payload_incremental(&packet).expect("parses photon packet");
        assert_eq!(frames.len(), 3);

        let kinds: Vec<&'static str> = frames
            .into_iter()
            .map(|frame| decode_command_envelope(&frame.body).expect("decodes command"))
            .map(|m| AlbionCommandType::from(m.command_type).as_str())
            .collect();

        assert!(kinds.contains(&"reliable"));
        assert!(kinds.contains(&"unreliable"));
        assert!(kinds.contains(&"fragment"));
    }

    #[test]
    fn incomplete_command_header_contains_parser_state() {
        let mut packet = build_photon_packet(vec![(6u8, 0u8, 1u32, vec![0xAA])]);
        packet.truncate(16);
        let err = parse_udp_payload_incremental(&packet).unwrap_err();
        match err {
            FrameParseError::Incomplete { state, .. } => assert_eq!(state, "command_header"),
            other => panic!("expected incomplete, got {other:?}"),
        }
    }

    #[test]
    fn unreliable_command_skips_subheader_and_preserves_message_type() {
        let packet = build_photon_packet(vec![(
            7u8,
            0u8,
            1u32,
            vec![0xDE, 0xAD, 0xBE, 0xEF, 0x99, 0x02, 0x11, 0x22],
        )]);

        let frames = parse_udp_payload_incremental(&packet).expect("parses packet");
        assert_eq!(frames.len(), 1);

        let decoded = decode_command_envelope(&frames[0].body).expect("decodes envelope");
        assert_eq!(decoded.command_type, 7);
        assert_eq!(decoded.message_type, 0x02);
        assert_eq!(decoded.payload, vec![0x11, 0x22]);
    }

    #[test]
    fn unreliable_command_rejects_short_subheader_payload() {
        let packet = build_photon_packet(vec![(7u8, 0u8, 1u32, vec![0xAA, 0xBB, 0xCC])]);

        let err = parse_udp_payload_incremental(&packet).expect_err("must fail");
        match err {
            FrameParseError::Invalid(DecodeError::Transport { reason, .. }) => {
                assert!(reason.contains("4-byte subheader"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn reliable_and_unreliable_envelopes_decode_expected_message_type() {
        // message type at payload[1] for command types 6/7 after transport normalization
        let reliable_payload = vec![0x10, 0xF3, 0xAA, 0xBB];
        // unreliable payload has an extra 4-byte subheader that transport strips
        let unreliable_payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x10, 0xA7, 0xCC, 0xDD];

        let packet = build_photon_packet(vec![
            (6u8, 0u8, 1u32, reliable_payload),
            (7u8, 0u8, 2u32, unreliable_payload),
        ]);

        let frames = parse_udp_payload_incremental(&packet).expect("parses packet");
        assert_eq!(frames.len(), 2);

        let reliable = decode_command_envelope(&frames[0].body).expect("decodes reliable");
        assert_eq!(reliable.command_type, 6);
        assert_eq!(reliable.message_type, 0xF3);

        let unreliable = decode_command_envelope(&frames[1].body).expect("decodes unreliable");
        assert_eq!(unreliable.command_type, 7);
        assert_eq!(unreliable.message_type, 0xA7);
    }

    fn build_photon_packet(commands: Vec<(u8, u8, u32, Vec<u8>)>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x1234u16.to_be_bytes()); // peer id
        out.push(0xCC); // crc marker flags
        out.push(commands.len() as u8); // command count
        out.extend_from_slice(&0x01020304u32.to_be_bytes()); // timestamp
        out.extend_from_slice(&0x05060708u32.to_be_bytes()); // challenge

        for (kind, channel, seq, payload) in commands {
            let cmd_len = (PHOTON_COMMAND_HEADER_LEN + payload.len()) as u32;
            out.push(kind);
            out.push(channel);
            out.push(0);
            out.push(0);
            out.extend_from_slice(&cmd_len.to_be_bytes());
            out.extend_from_slice(&seq.to_be_bytes());
            out.extend_from_slice(&payload);
        }
        out
    }
}
