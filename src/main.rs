mod albion;
mod capture;
mod config;
mod sheets;

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{albion::{ids, transaction::MarketTransaction}, config::Config};

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
    std::thread::spawn(move || {
        let mut processor = albion::session::PacketProcessor::new(Duration::from_secs(90));
        let mut packet_count = 0usize;
        let mut udp_payload_count = 0usize;
        let mut decoded_message_count = 0usize;
        let mut event_code_hits = 0usize;
        let mut operation_code_hits = 0usize;
        let mut decode_failures = 0usize;
        let mut transaction_count = 0usize;
        if let Err(err) = capture::pcap_capture::capture_loop(&interface, move |packet| {
            packet_count = packet_count.wrapping_add(1);
            if packet_count % 512 == 0 {
                processor.cleanup_stale_sessions();
            }
            let Some((udp_payload, src_ip, src_port, dst_ip, dst_port, protocol)) =
                albion::decoder::extract_udp_payload_ipv4(packet)
            else {
                return;
            };
            udp_payload_count = udp_payload_count.wrapping_add(1);
            let session_key = albion::session::SessionKey {
                src_ip,
                src_port,
                dst_ip,
                dst_port,
                protocol,
            };

            let messages = processor.ingest_packet(session_key, udp_payload);
            decoded_message_count = decoded_message_count.wrapping_add(messages.len());
            for message in &messages {
                match albion::decoder::probe_message(message) {
                    albion::decoder::DecodeProbe::EventDecoded { code, key_count } => {
                        if ids::MARKET_EVENT_CODES.contains(&code) {
                            event_code_hits = event_code_hits.wrapping_add(1);
                            debug!(code, key_count, "market event code observed in decoded message");
                        }
                    }
                    albion::decoder::DecodeProbe::OperationDecoded {
                        op_code,
                        return_code,
                        key_count,
                    } => {
                        if ids::MARKET_OPERATION_CODES.contains(&op_code) {
                            operation_code_hits = operation_code_hits.wrapping_add(1);
                            debug!(op_code, return_code, key_count, "market operation code observed in decoded message");
                        }
                    }
                    albion::decoder::DecodeProbe::UnsupportedCommandType { command_type } => {
                        debug!(command_type, "unsupported command type observed");
                    }
                    albion::decoder::DecodeProbe::EventDecodeFailed
                    | albion::decoder::DecodeProbe::OperationDecodeFailed => {
                        decode_failures = decode_failures.wrapping_add(1);
                    }
                }
            }
            for txn in albion::decoder::extract_market_transactions(&messages) {
                transaction_count = transaction_count.wrapping_add(1);
                debug!(?txn, "decoded transaction event");
                let _ = capture_tx.blocking_send(txn);
            }
            if packet_count % 2048 == 0 {
                info!(
                    packet_count,
                    udp_payload_count,
                    decoded_message_count,
                    event_code_hits,
                    operation_code_hits,
                    decode_failures,
                    transaction_count,
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
