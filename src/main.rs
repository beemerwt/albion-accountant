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
        if self.packets_seen == 0 { 0.0 } else { (value as f64 * 100.0) / self.packets_seen as f64 }
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

    let interface = capture::pcap_capture::pick_interface(config.interface.clone())?;
    info!(%interface, "selected capture interface");

    let (tx, mut rx) = mpsc::channel::<MarketTransaction>(256);
    let capture_tx = tx.clone();
    let interface_for_logs = interface.clone();
    std::thread::spawn(move || {
        let mut processor = albion::session::PacketProcessor::new(Duration::from_secs(90));
        let mut counters = PipelineCounters::default();
        if let Err(err) = capture::pcap_capture::capture_loop(&interface, move |packet| {
            counters.packets_seen = counters.packets_seen.wrapping_add(1);
            if counters.packets_seen % 512 == 0 {
                processor.cleanup_stale_sessions();
            }
            let Some((udp_payload, src_ip, src_port, dst_ip, dst_port, protocol)) =
                albion::decoder::extract_udp_payload_ipv4(packet)
            else {
                if packet.len() < 14 {
                    counters.malformed_header_drops = counters.malformed_header_drops.wrapping_add(1);
                    if PipelineCounters::should_sample(counters.malformed_header_drops) {
                        debug!(drop_reason = "malformed_eth", interface = %interface_for_logs, packet_len = packet.len(), "packet rejected");
                    }
                } else {
                    let ether_type = u16::from_be_bytes([packet[12], packet[13]]);
                    if ether_type != 0x0800 {
                        counters.non_ipv4_drops = counters.non_ipv4_drops.wrapping_add(1);
                        if PipelineCounters::should_sample(counters.non_ipv4_drops) {
                            debug!(drop_reason = "non_ipv4", interface = %interface_for_logs, packet_len = packet.len(), ether_type, "packet rejected");
                        }
                    } else if packet.len() < 34 {
                        counters.malformed_header_drops = counters.malformed_header_drops.wrapping_add(1);
                    } else {
                        let proto = packet[23];
                        if proto != 17 {
                            counters.non_udp_drops = counters.non_udp_drops.wrapping_add(1);
                            if PipelineCounters::should_sample(counters.non_udp_drops) {
                                debug!(drop_reason = "non_udp", interface = %interface_for_logs, packet_len = packet.len(), ether_type, proto, "packet rejected");
                            }
                        } else {
                            counters.malformed_header_drops = counters.malformed_header_drops.wrapping_add(1);
                        }
                    }
                }
                return;
            };
            counters.udp_payloads_accepted = counters.udp_payloads_accepted.wrapping_add(1);
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
                            counters.successful_decodes = counters.successful_decodes.wrapping_add(1);
                            debug!(
                                code,
                                key_count, "market event code observed in decoded message"
                            );
                        }
                    }
                    albion::decoder::DecodeProbe::OperationDecoded {
                        op_code,
                        return_code,
                        key_count,
                    } => {
                        if ids::MARKET_OPERATION_CODES.contains(&op_code) {
                            counters.successful_decodes = counters.successful_decodes.wrapping_add(1);
                            debug!(
                                op_code,
                                return_code,
                                key_count,
                                "market operation code observed in decoded message"
                            );
                        }
                    }
                    albion::decoder::DecodeProbe::UnsupportedCommandType { command_type } => {
                        counters.unsupported_command_types = counters.unsupported_command_types.wrapping_add(1);
                        if PipelineCounters::should_sample(counters.unsupported_command_types) {
                            debug!(drop_reason = "unsupported_command_type", command_type, "unsupported command type observed");
                        }
                    }
                    albion::decoder::DecodeProbe::EventDecodeFailed => {
                        counters.event_decode_failures = counters.event_decode_failures.wrapping_add(1);
                    }
                    albion::decoder::DecodeProbe::OperationDecodeFailed => {
                        counters.operation_decode_failures = counters.operation_decode_failures.wrapping_add(1);
                    }
                }
            }
            for txn in albion::decoder::extract_market_transactions(&messages) {
                counters.mapped_transactions_emitted = counters.mapped_transactions_emitted.wrapping_add(1);
                debug!(?txn, "decoded transaction event");
                let _ = capture_tx.blocking_send(txn);
            }
            if counters.packets_seen % 2048 == 0 {
                info!(
                    packets_seen = counters.packets_seen,
                    non_ipv4_drops = counters.non_ipv4_drops,
                    non_udp_drops = counters.non_udp_drops,
                    malformed_header_drops = counters.malformed_header_drops,
                    udp_payloads_accepted = counters.udp_payloads_accepted,
                    frame_parse_incomplete = counters.frame_parse_incomplete,
                    frame_parse_invalid = counters.frame_parse_invalid,
                    command_envelope_decode_errors = counters.command_envelope_decode_errors,
                    unsupported_command_types = counters.unsupported_command_types,
                    event_decode_failures = counters.event_decode_failures,
                    operation_decode_failures = counters.operation_decode_failures,
                    successful_decodes = counters.successful_decodes,
                    mapped_transactions_emitted = counters.mapped_transactions_emitted,
                    non_ipv4_drop_pct = counters.pct(counters.non_ipv4_drops),
                    non_udp_drop_pct = counters.pct(counters.non_udp_drops),
                    malformed_drop_pct = counters.pct(counters.malformed_header_drops),
                    accepted_udp_pct = counters.pct(counters.udp_payloads_accepted),
                    decode_success_pct = counters.pct(counters.successful_decodes),
                    mapped_txn_pct = counters.pct(counters.mapped_transactions_emitted),
                    "decoder pipeline summary"
                );
            }
        }) {
            error!(error = %err, "capture loop failed");
        }
    });

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

    info!(spreadsheet = %sheets_client.spreadsheet_id, sheet = %sheets_client.sheet_name, "Google Sheet target");

    let sheets_client = Arc::new(sheets_client);
    sheets_client.ensure_header().await?;

    let mut dedupe = HashSet::new();
    while let Some(txn) = rx.recv().await {
        let key = txn.dedupe_key();
        if !dedupe.insert(key) {
            debug!("duplicate transaction skipped");
            continue;
        }
        if let Err(err) = sheets_client.append_transaction_with_retry(&txn).await {
            warn!(error = %err, "Google append failed");
        }
    }

    Ok(())
}
