use std::{
    collections::{BTreeMap, HashMap},
    net::IpAddr,
    time::{Duration, Instant},
};

use tracing::warn;

use super::protocol::{
    commands::{PhotonMessage, decode_command_envelope},
    transport::{FrameParseError, parse_udp_payload_incremental},
};

const MAX_FRAGMENT_BYTES: usize = 64 * 1024;
const MAX_PENDING_PER_CHANNEL: usize = 64;
const MAX_SEQUENCE_GAP: u16 = 512;
const MAX_INACTIVE_PENDING_AGE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub src_ip: IpAddr,
    pub src_port: u16,
    pub dst_ip: IpAddr,
    pub dst_port: u16,
    pub protocol: u8,
}

#[derive(Debug, Clone)]
pub struct SessionMetadata {
    pub src_ip: IpAddr,
    pub src_port: u16,
    pub dst_ip: IpAddr,
    pub dst_port: u16,
}

#[derive(Debug, Default)]
pub struct ChannelState {
    expected_seq: Option<u16>,
    pending: BTreeMap<u16, PhotonMessage>,
    last_progress: Option<Instant>,
}

#[derive(Debug)]
pub struct SessionState {
    channels: HashMap<u8, ChannelState>,
    fragment_buffer: Vec<u8>,
    peer: SessionMetadata,
    last_seen: Instant,
}

#[derive(Debug)]
pub struct PacketProcessor {
    sessions: HashMap<SessionKey, SessionState>,
    session_ttl: Duration,
}

impl PacketProcessor {
    pub fn new(session_ttl: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            session_ttl,
        }
    }

    pub fn ingest_packet(
        &mut self,
        session_key: SessionKey,
        packet_bytes: &[u8],
    ) -> Vec<PhotonMessage> {
        let now = Instant::now();
        let state = self
            .sessions
            .entry(session_key.clone())
            .or_insert_with(|| SessionState {
                channels: HashMap::new(),
                fragment_buffer: Vec::new(),
                peer: SessionMetadata {
                    src_ip: session_key.src_ip,
                    src_port: session_key.src_port,
                    dst_ip: session_key.dst_ip,
                    dst_port: session_key.dst_port,
                },
                last_seen: now,
            });
        state.last_seen = now;

        cleanup_stale_channel_pending(&session_key, now, &mut state.channels);

        if state.fragment_buffer.len() + packet_bytes.len() > MAX_FRAGMENT_BYTES {
            warn!(session_key = ?session_key, buffered = state.fragment_buffer.len(), incoming = packet_bytes.len(), "dropping fragment buffer due to size limit");
            state.fragment_buffer.clear();
        }

        let mut merged = Vec::with_capacity(state.fragment_buffer.len() + packet_bytes.len());
        merged.extend_from_slice(&state.fragment_buffer);
        merged.extend_from_slice(packet_bytes);
        state.fragment_buffer.clear();

        let frames = match parse_udp_payload_incremental(&merged) {
            Ok(frames) => frames,
            Err(FrameParseError::Incomplete { .. }) => {
                state.fragment_buffer = merged;
                state.last_seen = now;
                return Vec::new();
            }
            Err(FrameParseError::Invalid(err)) => {
                warn!(session_key = ?session_key, error = %err, "invalid framed payload; dropping packet bytes");
                return Vec::new();
            }
        };

        let mut emitted = Vec::new();
        for frame in frames {
            let msg = match decode_command_envelope(&frame.body) {
                Ok(msg) => msg,
                Err(err) => {
                    warn!(session_key = ?session_key, error = %err, "decode warning");
                    continue;
                }
            };

            let chan = state.channels.entry(msg.channel).or_default();
            if chan.pending.contains_key(&msg.reliable_sequence) {
                warn!(session_key = ?session_key, channel_id = msg.channel, seq = msg.reliable_sequence, "duplicate sequence suppressed");
                continue;
            }
            if chan.pending.len() >= MAX_PENDING_PER_CHANNEL {
                warn!(session_key = ?session_key, channel_id = msg.channel, seq = msg.reliable_sequence, "pending queue full; dropping out-of-order packet");
                continue;
            }
            let channel_id = msg.channel;
            chan.pending.insert(msg.reliable_sequence, msg);
            flush_channel(&session_key, channel_id, chan, &mut emitted);
        }

        emitted
    }

    pub fn cleanup_stale_sessions(&mut self) {
        let now = Instant::now();
        self.sessions.retain(|key, state| {
            let keep = now.duration_since(state.last_seen) <= self.session_ttl;
            if !keep {
                warn!(session_key = ?key, src_ip = %state.peer.src_ip, src_port = state.peer.src_port, dst_ip = %state.peer.dst_ip, dst_port = state.peer.dst_port, "dropping stale session state");
            }
            keep
        });
    }
}

fn flush_channel(
    session_key: &SessionKey,
    channel_id: u8,
    chan: &mut ChannelState,
    out: &mut Vec<PhotonMessage>,
) {
    // Policy: for a brand-new channel, buffer the first seen sequence and set
    // `expected_seq` to `first_seen - 1` (wrapping). This intentionally waits for
    // a potentially missing lower sequence before releasing buffered messages.
    // If that lower sequence does not arrive, small-gap advance logic eventually
    // moves `expected_seq` forward and emits in-order from the earliest pending
    // sequence; large gaps still trigger resynchronization.
    if chan.expected_seq.is_none() {
        if let Some((&seq, _)) = chan.pending.iter().next() {
            chan.expected_seq = Some(seq.wrapping_sub(1));
        }
    }

    while let Some(expected) = chan.expected_seq {
        if let Some(msg) = chan.pending.remove(&expected) {
            out.push(msg);
            chan.expected_seq = Some(expected.wrapping_add(1));
            chan.last_progress = Some(Instant::now());
            continue;
        }

        if let Some((&smallest, _)) = chan.pending.iter().next() {
            let gap = smallest.wrapping_sub(expected);
            if gap > 0 && gap <= MAX_SEQUENCE_GAP {
                warn!(session_key = ?session_key, channel_id = channel_id, seq = expected, next_seq = smallest, gap = gap, "missing sequence in small gap; advancing expected seq");
                chan.expected_seq = Some(smallest);
                continue;
            }
            if gap > MAX_SEQUENCE_GAP {
                warn!(session_key = ?session_key, channel_id = channel_id, seq = expected, next_seq = smallest, gap = gap, "large sequence gap detected; resyncing channel state");
                chan.pending.clear();
                chan.expected_seq = Some(smallest);
                break;
            }
        }
        break;
    }
}

fn cleanup_stale_channel_pending(
    session_key: &SessionKey,
    now: Instant,
    channels: &mut HashMap<u8, ChannelState>,
) {
    for (channel_id, chan) in channels.iter_mut() {
        if chan.pending.is_empty() {
            continue;
        }
        if let Some(last_progress) = chan.last_progress
            && now.duration_since(last_progress) > MAX_INACTIVE_PENDING_AGE
        {
            warn!(session_key = ?session_key, channel_id = channel_id, pending = chan.pending.len(), "stale pending queue dropped for channel");
            chan.pending.clear();
            chan.expected_seq = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::{IpAddr, Ipv4Addr}, thread, time::Duration};
    use super::*;

    fn key() -> SessionKey {
        SessionKey { src_ip: IpAddr::V4(Ipv4Addr::new(1,1,1,1)), src_port: 1000, dst_ip: IpAddr::V4(Ipv4Addr::new(2,2,2,2)), dst_port: 2000, protocol: 17 }
    }

    fn frame(channel: u8, seq: u16, payload: &[u8]) -> Vec<u8> {
        let mut body = vec![7, channel];
        body.extend_from_slice(&seq.to_be_bytes());
        body.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        body.extend_from_slice(payload);
        let mut out = (body.len() as u16).to_be_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn split_frame_across_packets_buffers_and_reassembles() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        let pkt = frame(1, 10, b"abc");
        assert!(p.ingest_packet(key(), &pkt[..3]).is_empty());
        let out = p.ingest_packet(key(), &pkt[3..]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reliable_sequence, 10);
    }

    #[test]
    fn reordered_reliable_messages_emit_in_sequence() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        assert!(p.ingest_packet(key(), &frame(1, 12, b"twelve")).is_empty());
        let out = p.ingest_packet(key(), &frame(1, 11, b"eleven"));
        assert_eq!(
            out.iter().map(|m| m.reliable_sequence).collect::<Vec<_>>(),
            vec![11, 12]
        );
    }

    #[test]
    fn first_packet_high_seq_is_buffered_until_gap_logic_advances() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        let first = p.ingest_packet(key(), &frame(1, 900, b"high"));
        assert_eq!(
            first
                .iter()
                .map(|m| m.reliable_sequence)
                .collect::<Vec<_>>(),
            vec![900]
        );
    }

    #[test]
    fn in_order_messages_emit_normally() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        let first = p.ingest_packet(key(), &frame(1, 10, b"ten"));
        assert!(first.is_empty());
        let second = p.ingest_packet(key(), &frame(1, 11, b"eleven"));
        assert_eq!(
            second
                .iter()
                .map(|m| m.reliable_sequence)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
    }

    #[test]
    fn duplicate_suppression_keeps_only_first_emission() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        let first = p.ingest_packet(key(), &frame(1, 40, b"a"));
        assert_eq!(
            first
                .iter()
                .map(|m| m.reliable_sequence)
                .collect::<Vec<_>>(),
            vec![40]
        );

        let duplicate = p.ingest_packet(key(), &frame(1, 40, b"a"));
        assert!(duplicate.is_empty());
    }

    #[test]
    fn gap_recovery_advances_and_emits_from_smallest_pending() {
        let mut p = PacketProcessor::new(Duration::from_secs(60));
        assert!(p.ingest_packet(key(), &frame(1, 10, b"ten")).is_empty());

        let out = p.ingest_packet(key(), &frame(1, 12, b"twelve"));
        assert_eq!(
            out.iter().map(|m| m.reliable_sequence).collect::<Vec<_>>(),
            vec![10]
        );

        let out = p.ingest_packet(key(), &frame(1, 11, b"eleven"));
        assert_eq!(
            out.iter().map(|m| m.reliable_sequence).collect::<Vec<_>>(),
            vec![11, 12]
        );
    }

    #[test]
    fn stale_session_cleanup_keeps_recent_fragmented_sessions() {
        let mut p = PacketProcessor::new(Duration::from_millis(100));
        let pkt = frame(1, 1, b"xyz");
        let _ = p.ingest_packet(key(), &pkt[..3]);
        thread::sleep(Duration::from_millis(70));
        let _ = p.ingest_packet(key(), &[]);
        p.cleanup_stale_sessions();
        assert_eq!(p.sessions.len(), 1);
    }
}
