mod albion;
mod capture;
mod config;
mod sheets;

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    albion::{ids, transaction::MarketTransaction},
    config::Config,
};

#[derive(Default)]
struct PipelineCounters {
    packets_seen: usize,
    non_ipv4_drops: usize,
    non_udp_drops: usize,
    malformed_header_drops: usize,
    udp_payloads_accepted: usize,
    frame_parse_incomplete: usize,
    frame_parse_invalid: usize,
    command_envelope_decode_errors: usize,
    unsupported_command_types: usize,
    event_decode_failures: usize,
    operation_decode_failures: usize,
    successful_decodes: usize,
    mapped_transactions_emitted: usize,
}

impl PipelineCounters {
    fn pct(&self, value: usize) -> f64 {
        if self.packets_seen == 0 {
            0.0
        } else {
            (value as f64 * 100.0) / self.packets_seen as f64
        }
    }

    fn should_sample(value: usize) -> bool {
        value <= 8 || value.is_multiple_of(128)
    }
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> Result<()> {
    install_rustls_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "albion_accountant=info".into()),
        )
        .init();

    let config = Config::load()?;

    if config.list_interfaces {
        for name in capture::pcap_capture::list_interfaces()? {
            println!("{name}");
        }
        return Ok(());
    }

    let interfaces = if config.all_interfaces {
        capture::pcap_capture::list_non_loopback_interfaces()?
    } else if config.interfaces.is_empty() {
        vec![capture::pcap_capture::pick_interface(vec![])?]
    } else {
        config.interfaces.clone()
    };
    info!(interfaces = ?interfaces, "selected capture interfaces");

    let (tx, mut rx) = mpsc::channel::<MarketTransaction>(256);
    let mut active_interfaces = 0usize;
    for interface in interfaces {
        let filter_expr = capture::pcap_capture::build_filter_expression(
            config.filter_mode,
            config.bpf.as_deref(),
            config.albion_hosts_file.as_deref(),
            config.albion_port_expr.as_deref(),
        );
        match capture::pcap_capture::open_capture_handle(&interface, &filter_expr) {
            Ok(mut cap) => {
                active_interfaces = active_interfaces.wrapping_add(1);
                let capture_tx = tx.clone();
                std::thread::spawn(move || {
                    let mut processor =
                        albion::session::PacketProcessor::new(Duration::from_secs(90));
                    let mut counters = PipelineCounters::default();
                    loop {
                        let packet = match cap.next_packet() {
                            Ok(packet) => packet,
                            Err(err) => {
                                error!(interface = %interface, error = %err, "capture loop failed");
                                return;
                            }
                        };
                        if packet.data.is_empty() {
                            continue;
                        }
                        let packet = packet.data;
                        counters.packets_seen = counters.packets_seen.wrapping_add(1);
                        if counters.packets_seen % 512 == 0 {
                            processor.cleanup_stale_sessions();
                        }
                        let Some((udp_payload, src_ip, src_port, dst_ip, dst_port, protocol)) =
                            albion::decoder::extract_udp_payload_ipv4(packet)
                        else {
                            if packet.len() < 14 {
                                counters.malformed_header_drops =
                                    counters.malformed_header_drops.wrapping_add(1);
                                if PipelineCounters::should_sample(counters.malformed_header_drops)
                                {
                                    debug!(drop_reason = "malformed_eth", interface = %interface, packet_len = packet.len(), "packet rejected");
                                }
                            } else {
                                let ether_type = u16::from_be_bytes([packet[12], packet[13]]);
                                if ether_type != 0x0800 {
                                    counters.non_ipv4_drops =
                                        counters.non_ipv4_drops.wrapping_add(1);
                                    if PipelineCounters::should_sample(counters.non_ipv4_drops) {
                                        debug!(drop_reason = "non_ipv4", interface = %interface, packet_len = packet.len(), ether_type, "packet rejected");
                                    }
                                } else if packet.len() < 34 {
                                    counters.malformed_header_drops =
                                        counters.malformed_header_drops.wrapping_add(1);
                                } else {
                                    let proto = packet[23];
                                    if proto != 17 {
                                        counters.non_udp_drops =
                                            counters.non_udp_drops.wrapping_add(1);
                                        if PipelineCounters::should_sample(counters.non_udp_drops) {
                                            debug!(drop_reason = "non_udp", interface = %interface, packet_len = packet.len(), ether_type, proto, "packet rejected");
                                        }
                                    } else {
                                        counters.malformed_header_drops =
                                            counters.malformed_header_drops.wrapping_add(1);
                                    }
                                }
                            }
                            continue;
                        };
                        counters.udp_payloads_accepted =
                            counters.udp_payloads_accepted.wrapping_add(1);
                        let session_key = albion::session::SessionKey {
                            src_ip,
                            src_port,
                            dst_ip,
                            dst_port,
                            protocol,
                        };
                        let messages = processor.ingest_packet(session_key, udp_payload);
                        for message in &messages {
                            match albion::decoder::probe_message(message) {
                                albion::decoder::DecodeProbe::EventDecoded { code, key_count } => {
                                    if ids::MARKET_EVENT_CODES.contains(&code) {
                                        counters.successful_decodes =
                                            counters.successful_decodes.wrapping_add(1);
                                        debug!(interface = %interface, code, key_count, "market event code observed in decoded message");
                                    }
                                }
                                albion::decoder::DecodeProbe::OperationDecoded {
                                    op_code,
                                    return_code,
                                    key_count,
                                } => {
                                    if ids::MARKET_OPERATION_CODES.contains(&op_code) {
                                        counters.successful_decodes =
                                            counters.successful_decodes.wrapping_add(1);
                                        debug!(interface = %interface, op_code, return_code, key_count, "market operation code observed in decoded message");
                                    }
                                }
                                albion::decoder::DecodeProbe::UnsupportedCommandType {
                                    command_type,
                                } => {
                                    counters.unsupported_command_types =
                                        counters.unsupported_command_types.wrapping_add(1);
                                    if PipelineCounters::should_sample(
                                        counters.unsupported_command_types,
                                    ) {
                                        debug!(drop_reason = "unsupported_command_type", interface = %interface, command_type, "unsupported command type observed");
                                    }
                                }
                                albion::decoder::DecodeProbe::EventDecodeFailed => {
                                    counters.event_decode_failures =
                                        counters.event_decode_failures.wrapping_add(1);
                                }
                                albion::decoder::DecodeProbe::OperationDecodeFailed => {
                                    counters.operation_decode_failures =
                                        counters.operation_decode_failures.wrapping_add(1);
                                }
                            }
                        }
                        for txn in albion::decoder::extract_market_transactions(&messages) {
                            counters.mapped_transactions_emitted =
                                counters.mapped_transactions_emitted.wrapping_add(1);
                            debug!(interface = %interface, ?txn, "decoded transaction event");
                            let _ = capture_tx.blocking_send(txn);
                        }
                        if counters.packets_seen % 2048 == 0 {
                            info!(interface = %interface, packets_seen = counters.packets_seen, non_ipv4_drops = counters.non_ipv4_drops, non_udp_drops = counters.non_udp_drops, malformed_header_drops = counters.malformed_header_drops, udp_payloads_accepted = counters.udp_payloads_accepted, frame_parse_incomplete = counters.frame_parse_incomplete, frame_parse_invalid = counters.frame_parse_invalid, command_envelope_decode_errors = counters.command_envelope_decode_errors, unsupported_command_types = counters.unsupported_command_types, event_decode_failures = counters.event_decode_failures, operation_decode_failures = counters.operation_decode_failures, successful_decodes = counters.successful_decodes, mapped_transactions_emitted = counters.mapped_transactions_emitted, non_ipv4_drop_pct = counters.pct(counters.non_ipv4_drops), non_udp_drop_pct = counters.pct(counters.non_udp_drops), malformed_drop_pct = counters.pct(counters.malformed_header_drops), accepted_udp_pct = counters.pct(counters.udp_payloads_accepted), decode_success_pct = counters.pct(counters.successful_decodes), mapped_txn_pct = counters.pct(counters.mapped_transactions_emitted), "decoder pipeline summary");
                        }
                    }
                });
            }
            Err(err) => {
                warn!(interface = %interface, error = %err, "failed to open capture interface; continuing")
            }
        }
    }
    if active_interfaces == 0 {
        anyhow::bail!("no capture interfaces could be activated");
    }

    if config.dry_run {
        while let Some(txn) = rx.recv().await {
            println!(
                "{} | {} | {} | {} | {}",
                txn.location, txn.item, txn.quantity, txn.per_item_cost, txn.total_cost
            );
        }
        return Ok(());
    }

    config.validate_google_config()?;

    let sheets_client = sheets::client::SheetsClient::new(
        config.google_client_secret.clone().expect("validated"),
        config.google_token_cache.clone(),
        config.spreadsheet_id.clone().expect("validated"),
        config.sheet_name.clone(),
    )
    .await?;

    info!(interface = "n/a", spreadsheet = %sheets_client.spreadsheet_id, sheet = %sheets_client.sheet_name, "Google Sheet target");

    let sheets_client = Arc::new(sheets_client);
    sheets_client.ensure_header().await?;

    let mut dedupe = HashSet::new();
    while let Some(txn) = rx.recv().await {
        let key = txn.dedupe_key();
        if !dedupe.insert(key) {
            debug!(interface = "n/a", "duplicate transaction skipped");
            continue;
        }
        if let Err(err) = sheets_client.append_transaction_with_retry(&txn).await {
            warn!(interface = "n/a", error = %err, "Google append failed");
        }
    }

    Ok(())
}
