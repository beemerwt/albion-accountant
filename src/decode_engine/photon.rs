use crate::{
    albion::{
        ids,
        protocol::{
            commands::{AlbionCommandType, PhotonMessage},
            events::decode_event_payload,
            operations::decode_operation_payload,
            protocol16::ProtocolValue,
        },
        session::{IngestOutcome, PacketProcessor, SessionKey},
        transaction::MarketTransaction,
    },
    decode_engine::types::DecodedPacket,
    ingress::IngressPacket,
    trade_mapping::semantics::TradeSemanticMapper,
};
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

pub struct DecodeEngine {
    processor: PacketProcessor,
    semantic_mapper: TradeSemanticMapper,
}

#[derive(Debug)]
pub struct DecodeEngineResult {
    pub outcome: IngestOutcome,
    pub decoded_packets: Vec<DecodedPacket>,
    pub transactions: Vec<MarketTransaction>,
    pub stats: DecodeEngineStats,
    pub session_key: SessionKey,
}

#[derive(Debug, Default, Clone)]
pub struct DecodeEngineStats {
    pub successful_decodes: usize,
    pub unsupported_command_types: usize,
    pub event_decode_failures: usize,
    pub operation_decode_failures: usize,
    pub unknown_message_types: usize,
    pub encrypted_like_payloads_seen: usize,
    pub stateful_transactions_emitted: usize,
}

impl DecodeEngine {
    pub fn new() -> Self {
        Self {
            processor: PacketProcessor::new(Duration::from_secs(90)),
            semantic_mapper: TradeSemanticMapper::new(),
        }
    }

    pub fn ingest_packet(&mut self, packet: &IngressPacket) -> Result<DecodeEngineResult> {
        if packet.packet_number.is_multiple_of(512) {
            self.processor.cleanup_stale_sessions();
        }
        let session_key = session_key_from_ingress(packet)?;
        let outcome = self
            .processor
            .ingest_packet(session_key.clone(), &packet.udp_payload);
        let mut stats = DecodeEngineStats::default();

        let decoded_packets = outcome
            .messages
            .iter()
            .filter_map(|message| decoded_packet_from_message(packet, message))
            .collect::<Vec<_>>();
        stats.observe_messages(&outcome.messages);
        stats.observe_decoded_packets(&decoded_packets);

        let transactions = self.semantic_mapper.map_stream(decoded_packets.iter());
        stats.stateful_transactions_emitted = transactions.len();

        Ok(DecodeEngineResult {
            outcome,
            decoded_packets,
            transactions,
            stats,
            session_key,
        })
    }
}

impl DecodeEngineStats {
    fn observe_messages(&mut self, messages: &[PhotonMessage]) {
        for message in messages {
            if payload_looks_encrypted(&message.payload) {
                self.encrypted_like_payloads_seen =
                    self.encrypted_like_payloads_seen.wrapping_add(1);
            }
            match AlbionCommandType::from_message_type(message.message_type) {
                AlbionCommandType::Event => {
                    if decode_event_payload(&message.payload).is_err() {
                        self.event_decode_failures = self.event_decode_failures.wrapping_add(1);
                    }
                }
                AlbionCommandType::OperationRequest | AlbionCommandType::OperationResponse => {
                    if decode_operation_payload(&message.payload).is_err() {
                        self.operation_decode_failures =
                            self.operation_decode_failures.wrapping_add(1);
                    }
                }
                AlbionCommandType::Unsupported(_) => {
                    self.unsupported_command_types = self.unsupported_command_types.wrapping_add(1);
                    self.unknown_message_types = self.unknown_message_types.wrapping_add(1);
                }
                _ => {
                    self.unsupported_command_types = self.unsupported_command_types.wrapping_add(1);
                }
            }
        }
    }

    fn observe_decoded_packets(&mut self, packets: &[DecodedPacket]) {
        for packet in packets {
            let is_market = match packet.message_type.as_str() {
                "event" => ids::MARKET_EVENT_CODES.contains(&(packet.code as u16)),
                "operation_request" | "operation_response" => {
                    ids::MARKET_OPERATION_CODES.contains(&(packet.code as u16))
                }
                _ => false,
            };
            if is_market {
                self.successful_decodes = self.successful_decodes.wrapping_add(1);
            }
        }
    }
}

fn payload_looks_encrypted(payload: &[u8]) -> bool {
    payload
        .first()
        .map(|b| matches!(*b, 0xF3 | 0xFD | 0x7E))
        .unwrap_or(false)
}

fn decoded_packet_from_message(
    ingress: &IngressPacket,
    message: &PhotonMessage,
) -> Option<DecodedPacket> {
    match AlbionCommandType::from_message_type(message.message_type) {
        AlbionCommandType::OperationRequest | AlbionCommandType::OperationResponse => {
            let root = decode_operation_payload(&message.payload).ok()?;
            let code = protocol_u64(root.get(ids::KEY_OP_CODE)?)? as i32;
            let return_code = root.get(ids::KEY_RETURN_CODE).and_then(protocol_i16);
            let parameters = root
                .get(ids::KEY_PARAMS)
                .and_then(protocol_map)
                .map(numbered_json_parameters)
                .unwrap_or_default();
            let extracted = Some(protocol_map_to_json_object(&root));
            Some(DecodedPacket {
                file: String::new(),
                packet_number: ingress.packet_number,
                direction: String::new(),
                source: ingress.source_endpoint.clone(),
                destination: ingress.destination_endpoint.clone(),
                message_type: AlbionCommandType::from_message_type(message.message_type)
                    .as_str()
                    .to_string(),
                code,
                name: String::new(),
                parameters,
                return_code,
                debug_message: String::new(),
                extracted,
            })
        }
        AlbionCommandType::Event => {
            let root = decode_event_payload(&message.payload).ok()?;
            let code = protocol_u64(root.get(ids::KEY_EVENT_CODE)?)? as i32;
            let parameters = root
                .get(ids::KEY_PARAMS)
                .and_then(protocol_map)
                .map(numbered_json_parameters)
                .unwrap_or_default();
            let extracted = Some(protocol_map_to_json_object(&root));
            Some(DecodedPacket {
                file: String::new(),
                packet_number: ingress.packet_number,
                direction: String::new(),
                source: ingress.source_endpoint.clone(),
                destination: ingress.destination_endpoint.clone(),
                message_type: AlbionCommandType::Event.as_str().to_string(),
                code,
                name: String::new(),
                parameters,
                return_code: None,
                debug_message: String::new(),
                extracted,
            })
        }
        _ => None,
    }
}

fn numbered_json_parameters(map: &BTreeMap<String, ProtocolValue>) -> BTreeMap<i32, Value> {
    map.iter()
        .filter_map(|(key, value)| Some((key.parse::<i32>().ok()?, protocol_value_to_json(value))))
        .collect()
}

fn protocol_map_to_json_object(map: &BTreeMap<String, ProtocolValue>) -> Value {
    Value::Object(
        map.iter()
            .map(|(key, value)| (key.clone(), protocol_value_to_json(value)))
            .collect::<Map<_, _>>(),
    )
}

fn protocol_value_to_json(value: &ProtocolValue) -> Value {
    match value {
        ProtocolValue::UnsignedByte(v) | ProtocolValue::Byte(v) => Value::from(*v),
        ProtocolValue::UnsignedShort(v) => Value::from(*v),
        ProtocolValue::Short(v) => Value::from(*v),
        ProtocolValue::UnsignedInt(v) => Value::from(*v),
        ProtocolValue::Int(v) => Value::from(*v),
        ProtocolValue::UnsignedLong(v) => Value::from(*v),
        ProtocolValue::Long(v) => Value::from(*v),
        ProtocolValue::Float(v) => Value::from(*v as f64),
        ProtocolValue::Double(v) => Value::from(*v),
        ProtocolValue::String(v) => Value::from(v.clone()),
        ProtocolValue::Bool(v) => Value::from(*v),
        ProtocolValue::ByteArray(v) => Value::Array(v.iter().map(|b| Value::from(*b)).collect()),
        ProtocolValue::Custom(type_code, wrapped) => {
            let mut object = Map::new();
            object.insert("type_code".to_string(), Value::from(*type_code));
            object.insert("value".to_string(), protocol_value_to_json(wrapped));
            Value::Object(object)
        }
        ProtocolValue::Object(wrapped) => protocol_value_to_json(wrapped),
        ProtocolValue::Array(values) => {
            Value::Array(values.iter().map(protocol_value_to_json).collect())
        }
        ProtocolValue::Dictionary(map) | ProtocolValue::Hashtable(map) => {
            protocol_map_to_json_object(map)
        }
    }
}

fn protocol_map(value: &ProtocolValue) -> Option<&BTreeMap<String, ProtocolValue>> {
    match value {
        ProtocolValue::Dictionary(map) | ProtocolValue::Hashtable(map) => Some(map),
        _ => None,
    }
}

fn protocol_u64(value: &ProtocolValue) -> Option<u64> {
    match value {
        ProtocolValue::UnsignedByte(v) | ProtocolValue::Byte(v) => Some(u64::from(*v)),
        ProtocolValue::UnsignedShort(v) => Some(u64::from(*v)),
        ProtocolValue::Short(v) => u64::try_from(*v).ok(),
        ProtocolValue::UnsignedInt(v) => Some(u64::from(*v)),
        ProtocolValue::Int(v) => u64::try_from(*v).ok(),
        ProtocolValue::UnsignedLong(v) => Some(*v),
        ProtocolValue::Long(v) => u64::try_from(*v).ok(),
        ProtocolValue::String(v) => v.parse().ok(),
        _ => None,
    }
}

fn protocol_i16(value: &ProtocolValue) -> Option<i16> {
    match value {
        ProtocolValue::Short(v) => Some(*v),
        ProtocolValue::Int(v) => i16::try_from(*v).ok(),
        ProtocolValue::String(v) => v.parse().ok(),
        _ => None,
    }
}

fn session_key_from_ingress(packet: &IngressPacket) -> Result<SessionKey> {
    let source = packet
        .source_endpoint
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid source endpoint {}", packet.source_endpoint))?;
    let destination = packet
        .destination_endpoint
        .parse::<SocketAddr>()
        .with_context(|| {
            format!(
                "invalid destination endpoint {}",
                packet.destination_endpoint
            )
        })?;
    Ok(SessionKey {
        src_ip: source.ip(),
        src_port: source.port(),
        dst_ip: destination.ip(),
        dst_port: destination.port(),
        protocol: 17,
    })
}

impl Default for DecodeEngine {
    fn default() -> Self {
        Self::new()
    }
}
