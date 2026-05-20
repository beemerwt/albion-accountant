use std::{
    collections::{BTreeMap, HashMap},
    net::IpAddr,
    time::{Duration, Instant},
};

use tracing::warn;

use super::protocol::{
    commands::{PhotonMessage, decode_command_envelope},
    transport::parse_udp_payload,
};

const MAX_FRAGMENT_BYTES: usize = 64 * 1024;
const MAX_PENDING_PER_CHANNEL: usize = 64;

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

        if state.fragment_buffer.len() + packet_bytes.len() > MAX_FRAGMENT_BYTES {
            warn!(session_key = ?session_key, buffered = state.fragment_buffer.len(), incoming = packet_bytes.len(), "dropping fragment buffer due to size limit");
            state.fragment_buffer.clear();
        }

        let mut merged = Vec::with_capacity(state.fragment_buffer.len() + packet_bytes.len());
        merged.extend_from_slice(&state.fragment_buffer);
        merged.extend_from_slice(packet_bytes);
        state.fragment_buffer.clear();

        let frames = match parse_udp_payload(&merged) {
            Ok(frames) => frames,
            Err(_) => {
                state.fragment_buffer = merged;
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
    if chan.expected_seq.is_none() {
        if let Some((&seq, _)) = chan.pending.iter().next() {
            chan.expected_seq = Some(seq);
        }
    }

    while let Some(expected) = chan.expected_seq {
        if let Some(msg) = chan.pending.remove(&expected) {
            out.push(msg);
            chan.expected_seq = Some(expected.wrapping_add(1));
            continue;
        }

        if let Some((&smallest, _)) = chan.pending.iter().next() {
            if smallest > expected {
                warn!(session_key = ?session_key, channel_id = channel_id, seq = expected, next_seq = smallest, "missing sequence; dropping gap and advancing expected seq");
                chan.expected_seq = Some(smallest);
                continue;
            }
        }
        break;
    }
}
