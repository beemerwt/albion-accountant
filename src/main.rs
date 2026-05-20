mod albion;
mod capture;
mod config;
mod sheets;

use std::{collections::HashSet, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{albion::transaction::MarketTransaction, config::Config};

#[tokio::main]
async fn main() -> Result<()> {
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
        if let Err(err) = capture::pcap_capture::capture_loop(&interface, move |packet| {
            if let Some(txn) = albion::decoder::decode_transaction(packet) {
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

    let sheets_client = sheets::client::SheetsClient::new(
        config
            .google_credentials
            .clone()
            .context("missing --google-credentials or ALBION_ACCOUNTANT_GOOGLE_CREDENTIALS")?,
        config
            .spreadsheet_id
            .clone()
            .context("missing --spreadsheet-id or ALBION_ACCOUNTANT_SPREADSHEET_ID")?,
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
