use crate::{
    browser::open_url_in_browser,
    cli::Args,
    error::{DecodeError, Result},
};
use google_sheets4::{
    Sheets,
    api::{
        AddSheetRequest, BatchUpdateSpreadsheetRequest, ClearValuesRequest,
        DeleteDuplicatesRequest, DimensionRange, GridRange, Request, SheetProperties,
        SortRangeRequest, SortSpec, ValueRange,
    },
    hyper_rustls, hyper_util, yup_oauth2,
    yup_oauth2::authenticator_delegate::InstalledFlowDelegate,
};
use serde_json::Value;
use std::{future::Future, path::PathBuf, pin::Pin};

const TOKEN_CACHE_PATH: &str = ".albion-accountant-token.json";
const SHEET_HEADER: [&str; 7] = ["ID", "Date", "Time", "Location", "Item", "Debit", "Credit"];
const SHEET_COLUMN_COUNT: i32 = 7;
const ID_COLUMN_INDEX: i32 = 0;
const DATE_COLUMN_INDEX: i32 = 1;
const TIME_COLUMN_INDEX: i32 = 2;
const HEADER_ROW_COUNT: i32 = 1;
type HttpsConnector =
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>;

#[derive(Clone)]
pub struct GoogleSheetsConfig {
    client_secret: PathBuf,
    spreadsheet_id: String,
    sheet_name: String,
}

#[derive(Clone)]
pub struct GoogleSheetsClient {
    hub: Sheets<HttpsConnector>,
    spreadsheet_id: String,
    sheet_name: String,
    sheet_id: i32,
}

impl GoogleSheetsConfig {
    pub fn from_args(args: &Args) -> Result<Option<Self>> {
        match (
            args.client_secret.as_ref(),
            args.spreadsheet_id.as_ref(),
            args.sheet_name.as_ref(),
        ) {
            (None, None, None) => Ok(None),
            (Some(client_secret), Some(spreadsheet_id), Some(sheet_name)) => {
                if spreadsheet_id.trim().is_empty() {
                    return Err(DecodeError(
                        "--spreadsheet-id / ALBION_ACCOUNTANT_SPREADSHEET_ID cannot be empty"
                            .to_string(),
                    ));
                }
                if sheet_name.trim().is_empty() {
                    return Err(DecodeError(
                        "--sheet-name / ALBION_ACCOUNTANT_SHEET_NAME cannot be empty".to_string(),
                    ));
                }

                Ok(Some(Self {
                    client_secret: client_secret.clone(),
                    spreadsheet_id: spreadsheet_id.clone(),
                    sheet_name: sheet_name.clone(),
                }))
            }
            _ => Err(DecodeError(
                "Google Sheets setup requires --client-secret, --spreadsheet-id, and --sheet-name, or ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET, ALBION_ACCOUNTANT_SPREADSHEET_ID, and ALBION_ACCOUNTANT_SHEET_NAME".to_string(),
            )),
        }
    }
}

pub async fn prepare_google_sheet(config: &GoogleSheetsConfig) -> Result<GoogleSheetsClient> {
    eprintln!(
        "INFO:albion: preparing Google sheet '{}' in spreadsheet '{}'.",
        config.sheet_name, config.spreadsheet_id
    );

    let secret = yup_oauth2::read_application_secret(&config.client_secret)
        .await
        .map_err(|err| {
            DecodeError(format!(
                "failed to read Google OAuth client secret '{}': {err}",
                config.client_secret.display()
            ))
        })?;

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|err| DecodeError(format!("failed to load native TLS roots: {err}")))?
        .https_or_http()
        .enable_http2()
        .build();
    let auth_client =
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build::<_, String>(connector.clone());
    let sheets_client =
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(connector);
    let auth = yup_oauth2::InstalledFlowAuthenticator::with_client(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        yup_oauth2::CustomHyperClientBuilder::from(auth_client),
    )
    .flow_delegate(Box::new(BrowserOpeningInstalledFlowDelegate))
    .persist_tokens_to_disk(TOKEN_CACHE_PATH)
    .build()
    .await
    .map_err(|err| DecodeError(format!("failed to initialize Google OAuth flow: {err}")))?;
    let hub = Sheets::new(sheets_client, auth);

    ensure_sheet_exists(&hub, config).await?;
    let sheet_id = load_sheet_id(&hub, config).await?;
    ensure_sheet_header(&hub, config).await?;
    Ok(GoogleSheetsClient {
        hub,
        spreadsheet_id: config.spreadsheet_id.clone(),
        sheet_name: config.sheet_name.clone(),
        sheet_id,
    })
}

struct BrowserOpeningInstalledFlowDelegate;

impl InstalledFlowDelegate for BrowserOpeningInstalledFlowDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            eprintln!("INFO:albion: opening Google OAuth token page in your browser: {url}");
            if let Err(err) = open_url_in_browser(url) {
                eprintln!(
                    "WARN:albion: failed to open browser automatically: {err}. Open this URL manually: {url}"
                );
            }
            Ok(String::new())
        })
    }
}

impl GoogleSheetsClient {
    pub async fn upsert_values(&self, values: Vec<Vec<Value>>) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }

        for row in values {
            let Some(row_id) = row.first().map(cell_value_to_string) else {
                continue;
            };
            if row_id.trim().is_empty() {
                continue;
            }

            if let Some(row_number) = self.find_row_number_by_id(&row_id).await? {
                self.update_row(row_number, row).await?;
            } else {
                self.append_row(row).await?;
            }
        }

        self.deduplicate_ids().await?;
        self.sort_table().await?;
        Ok(())
    }

    async fn find_row_number_by_id(&self, id: &str) -> Result<Option<usize>> {
        let range = sheet_id_column_range(&self.sheet_name);
        let (_, values) = self
            .hub
            .spreadsheets()
            .values_get(&self.spreadsheet_id, &range)
            .doit()
            .await
            .map_err(|err| {
                DecodeError(format!(
                    "failed to read ID column from sheet '{}' in spreadsheet '{}': {err}",
                    self.sheet_name, self.spreadsheet_id
                ))
            })?;

        Ok(values
            .values
            .unwrap_or_default()
            .iter()
            .enumerate()
            .find_map(|(index, row)| {
                if index == 0 {
                    return None;
                }
                let value = row.first().map(cell_value_to_string)?;
                (value == id).then_some(index + 1)
            }))
    }

    async fn update_row(&self, row_number: usize, row: Vec<Value>) -> Result<()> {
        let range = sheet_row_range(&self.sheet_name, row_number);
        let request = values_request(&range, vec![row]);

        self.hub
            .spreadsheets()
            .values_update(request, &self.spreadsheet_id, &range)
            .value_input_option("USER_ENTERED")
            .doit()
            .await
            .map_err(|err| {
                DecodeError(format!(
                    "failed to update row {row_number} in sheet '{}' in spreadsheet '{}': {err}",
                    self.sheet_name, self.spreadsheet_id
                ))
            })?;

        Ok(())
    }

    async fn append_row(&self, row: Vec<Value>) -> Result<()> {
        let range = sheet_append_range(&self.sheet_name);
        let request = values_request(&range, vec![row]);

        self.hub
            .spreadsheets()
            .values_append(request, &self.spreadsheet_id, &range)
            .value_input_option("USER_ENTERED")
            .insert_data_option("INSERT_ROWS")
            .doit()
            .await
            .map_err(|err| {
                DecodeError(format!(
                    "failed to append values to sheet '{}' in spreadsheet '{}': {err}",
                    self.sheet_name, self.spreadsheet_id
                ))
            })?;

        Ok(())
    }

    async fn deduplicate_ids(&self) -> Result<()> {
        let request = BatchUpdateSpreadsheetRequest {
            requests: Some(vec![Request {
                delete_duplicates: Some(DeleteDuplicatesRequest {
                    comparison_columns: Some(vec![DimensionRange {
                        dimension: Some("COLUMNS".to_string()),
                        sheet_id: Some(self.sheet_id),
                        start_index: Some(ID_COLUMN_INDEX),
                        end_index: Some(ID_COLUMN_INDEX + 1),
                    }]),
                    range: Some(data_grid_range(self.sheet_id)),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };

        self.hub
            .spreadsheets()
            .batch_update(request, &self.spreadsheet_id)
            .doit()
            .await
            .map_err(|err| {
                DecodeError(format!(
                    "failed to remove duplicate IDs from sheet '{}' in spreadsheet '{}': {err}",
                    self.sheet_name, self.spreadsheet_id
                ))
            })?;

        Ok(())
    }

    async fn sort_table(&self) -> Result<()> {
        let request = BatchUpdateSpreadsheetRequest {
            requests: Some(vec![Request {
                sort_range: Some(SortRangeRequest {
                    range: Some(data_grid_range(self.sheet_id)),
                    sort_specs: Some(vec![
                        ascending_sort(DATE_COLUMN_INDEX),
                        ascending_sort(TIME_COLUMN_INDEX),
                    ]),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };

        self.hub
            .spreadsheets()
            .batch_update(request, &self.spreadsheet_id)
            .doit()
            .await
            .map_err(|err| {
                DecodeError(format!(
                    "failed to sort sheet '{}' in spreadsheet '{}': {err}",
                    self.sheet_name, self.spreadsheet_id
                ))
            })?;

        Ok(())
    }
}

async fn ensure_sheet_header<C>(hub: &Sheets<C>, config: &GoogleSheetsConfig) -> Result<()>
where
    C: google_sheets4::common::Connector,
{
    let range = sheet_header_range(&config.sheet_name);
    let (_, values) = hub
        .spreadsheets()
        .values_get(&config.spreadsheet_id, &range)
        .doit()
        .await
        .map_err(|err| {
            DecodeError(format!(
                "failed to read sheet header '{}' in spreadsheet '{}': {err}",
                config.sheet_name, config.spreadsheet_id
            ))
        })?;

    if header_matches(values.values.as_deref()) {
        return Ok(());
    }

    if !header_is_empty(values.values.as_deref()) {
        eprintln!(
            "Warning: Google sheet '{}' has unexpected headers; clearing values before writing the Albion Accountant header.",
            config.sheet_name
        );
        clear_sheet_values(hub, config).await?;
    }

    write_header(hub, config).await
}

async fn ensure_sheet_exists<C>(hub: &Sheets<C>, config: &GoogleSheetsConfig) -> Result<()>
where
    C: google_sheets4::common::Connector,
{
    let (_, spreadsheet) = hub
        .spreadsheets()
        .get(&config.spreadsheet_id)
        .include_grid_data(false)
        .param("fields", "sheets.properties.title")
        .doit()
        .await
        .map_err(|err| DecodeError(format!("failed to load spreadsheet metadata: {err}")))?;

    let sheet_exists = spreadsheet.sheets.unwrap_or_default().iter().any(|sheet| {
        sheet
            .properties
            .as_ref()
            .and_then(|properties| properties.title.as_ref())
            == Some(&config.sheet_name)
    });

    if sheet_exists {
        return Ok(());
    }

    let request = BatchUpdateSpreadsheetRequest {
        requests: Some(vec![Request {
            add_sheet: Some(AddSheetRequest {
                properties: Some(SheetProperties {
                    title: Some(config.sheet_name.clone()),
                    ..Default::default()
                }),
            }),
            ..Default::default()
        }]),
        ..Default::default()
    };

    hub.spreadsheets()
        .batch_update(request, &config.spreadsheet_id)
        .doit()
        .await
        .map_err(|err| {
            DecodeError(format!(
                "failed to create sheet '{}' in spreadsheet '{}': {err}",
                config.sheet_name, config.spreadsheet_id
            ))
        })?;

    Ok(())
}

async fn load_sheet_id<C>(hub: &Sheets<C>, config: &GoogleSheetsConfig) -> Result<i32>
where
    C: google_sheets4::common::Connector,
{
    let (_, spreadsheet) = hub
        .spreadsheets()
        .get(&config.spreadsheet_id)
        .include_grid_data(false)
        .param("fields", "sheets.properties(sheetId,title)")
        .doit()
        .await
        .map_err(|err| DecodeError(format!("failed to load spreadsheet metadata: {err}")))?;

    spreadsheet
        .sheets
        .unwrap_or_default()
        .into_iter()
        .filter_map(|sheet| sheet.properties)
        .find(|properties| properties.title.as_ref() == Some(&config.sheet_name))
        .and_then(|properties| properties.sheet_id)
        .ok_or_else(|| {
            DecodeError(format!(
                "failed to find sheet '{}' in spreadsheet '{}'",
                config.sheet_name, config.spreadsheet_id
            ))
        })
}

async fn clear_sheet_values<C>(hub: &Sheets<C>, config: &GoogleSheetsConfig) -> Result<()>
where
    C: google_sheets4::common::Connector,
{
    hub.spreadsheets()
        .values_clear(
            ClearValuesRequest::default(),
            &config.spreadsheet_id,
            &sheet_range(&config.sheet_name),
        )
        .doit()
        .await
        .map_err(|err| {
            DecodeError(format!(
                "failed to clear sheet '{}' in spreadsheet '{}': {err}",
                config.sheet_name, config.spreadsheet_id
            ))
        })?;

    Ok(())
}

async fn write_header<C>(hub: &Sheets<C>, config: &GoogleSheetsConfig) -> Result<()>
where
    C: google_sheets4::common::Connector,
{
    let range = sheet_header_range(&config.sheet_name);
    let values = vec![
        SHEET_HEADER
            .iter()
            .map(|header| Value::from(*header))
            .collect(),
    ];
    let request = values_request(&range, values);

    hub.spreadsheets()
        .values_update(request, &config.spreadsheet_id, &range)
        .value_input_option("USER_ENTERED")
        .doit()
        .await
        .map_err(|err| {
            DecodeError(format!(
                "failed to write sheet header '{}' in spreadsheet '{}': {err}",
                config.sheet_name, config.spreadsheet_id
            ))
        })?;

    Ok(())
}

fn sheet_range(sheet_name: &str) -> String {
    format!("'{}'", sheet_name.replace('\'', "''"))
}

fn sheet_header_range(sheet_name: &str) -> String {
    format!("{}!A1:G1", sheet_range(sheet_name))
}

fn sheet_append_range(sheet_name: &str) -> String {
    format!("{}!A:G", sheet_range(sheet_name))
}

fn sheet_id_column_range(sheet_name: &str) -> String {
    format!("{}!A:A", sheet_range(sheet_name))
}

fn sheet_row_range(sheet_name: &str, row_number: usize) -> String {
    format!("{}!A{row_number}:G{row_number}", sheet_range(sheet_name))
}

fn values_request(range: &str, values: Vec<Vec<Value>>) -> ValueRange {
    ValueRange {
        major_dimension: Some("ROWS".to_string()),
        range: Some(range.to_string()),
        values: Some(values),
    }
}

fn header_matches(values: Option<&[Vec<Value>]>) -> bool {
    let Some(first_row) = values.and_then(|rows| rows.first()) else {
        return false;
    };

    first_row.len() == SHEET_HEADER.len()
        && first_row
            .iter()
            .zip(SHEET_HEADER)
            .all(|(actual, expected)| actual.as_str() == Some(expected))
}

fn header_is_empty(values: Option<&[Vec<Value>]>) -> bool {
    values.is_none_or(|rows| rows.first().is_none_or(Vec::is_empty))
}

fn cell_value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn data_grid_range(sheet_id: i32) -> GridRange {
    GridRange {
        sheet_id: Some(sheet_id),
        start_row_index: Some(HEADER_ROW_COUNT),
        start_column_index: Some(0),
        end_column_index: Some(SHEET_COLUMN_COUNT),
        ..Default::default()
    }
}

fn ascending_sort(column_index: i32) -> SortSpec {
    SortSpec {
        dimension_index: Some(column_index),
        sort_order: Some("ASCENDING".to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn header_matches_exact_expected_values() {
        let values = vec![vec![
            json!("ID"),
            json!("Date"),
            json!("Time"),
            json!("Location"),
            json!("Item"),
            json!("Debit"),
            json!("Credit"),
        ]];

        assert!(header_matches(Some(&values)));
    }

    #[test]
    fn header_rejects_mismatched_values() {
        let values = vec![vec![
            json!("ID"),
            json!("Date"),
            json!("Time"),
            json!("Place"),
            json!("Item"),
            json!("Debit"),
            json!("Credit"),
        ]];

        assert!(!header_matches(Some(&values)));
    }

    #[test]
    fn header_empty_handles_missing_or_empty_rows() {
        let empty_row = vec![Vec::new()];

        assert!(header_is_empty(None));
        assert!(header_is_empty(Some(&[])));
        assert!(header_is_empty(Some(&empty_row)));
    }

    #[test]
    fn cell_values_convert_to_matchable_ids() {
        assert_eq!(cell_value_to_string(&json!("14987113607")), "14987113607");
        assert_eq!(cell_value_to_string(&json!(14987113607_i64)), "14987113607");
    }
}
