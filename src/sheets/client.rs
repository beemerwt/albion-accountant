use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use google_sheets4::Sheets;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use tokio::time::sleep;
use yup_oauth2::ServiceAccountAuthenticator;

use crate::{albion::transaction::MarketTransaction, sheets::row};

pub struct SheetsClient {
    hub: Sheets<hyper_rustls::HttpsConnector<HttpConnector>>,
    pub spreadsheet_id: String,
    pub sheet_name: String,
}

impl SheetsClient {
    pub async fn new(
        credentials: PathBuf,
        spreadsheet_id: String,
        sheet_name: String,
    ) -> Result<Self> {
        let key = yup_oauth2::read_service_account_key(credentials).await?;
        let auth = ServiceAccountAuthenticator::builder(key).build().await?;
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(https);
        let hub = Sheets::new(client, auth);
        Ok(Self {
            hub,
            spreadsheet_id,
            sheet_name,
        })
    }

    pub async fn ensure_header(&self) -> Result<()> {
        let range = format!("{}!A1:E1", self.sheet_name);
        let (_, current) = self
            .hub
            .spreadsheets()
            .values_get(&self.spreadsheet_id, &range)
            .doit()
            .await?;
        let needs_header = current
            .values
            .as_ref()
            .map(|v| v.is_empty())
            .unwrap_or(true);
        if needs_header {
            self.hub
                .spreadsheets()
                .values_append(row::header_row(), &self.spreadsheet_id, &range)
                .value_input_option("RAW")
                .insert_data_option("INSERT_ROWS")
                .doit()
                .await?;
        }
        Ok(())
    }

    pub async fn append_transaction_with_retry(&self, txn: &MarketTransaction) -> Result<()> {
        let range = format!("{}!A:E", self.sheet_name);
        let mut delay = Duration::from_secs(1);
        for _ in 0..3 {
            let out = self
                .hub
                .spreadsheets()
                .values_append(row::transaction_row(txn), &self.spreadsheet_id, &range)
                .value_input_option("RAW")
                .insert_data_option("INSERT_ROWS")
                .doit()
                .await;
            match out {
                Ok(_) => return Ok(()),
                Err(_) => sleep(delay).await,
            }
            delay *= 2;
        }
        self.hub
            .spreadsheets()
            .values_append(row::transaction_row(txn), &self.spreadsheet_id, &range)
            .value_input_option("RAW")
            .insert_data_option("INSERT_ROWS")
            .doit()
            .await?;
        Ok(())
    }
}
