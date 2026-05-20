mod albion;
mod capture;
mod config;
mod sheets;

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{albion::transaction::MarketTransaction, config::Config};

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
            let session_key = albion::session::SessionKey {
                src_ip,
                src_port,
                dst_ip,
                dst_port,
                protocol,
            };

            let messages = processor.ingest_packet(session_key, udp_payload);
            for txn in albion::decoder::extract_market_transactions(&messages) {
                debug!(?txn, "decoded transaction event");
                let _ = capture_tx.blocking_send(txn);
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
