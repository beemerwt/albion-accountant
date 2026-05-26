use crate::{
    cli::Args,
    error::{DecodeError, Result},
};
use google_sheets4::{
    Sheets,
    api::{
        AddSheetRequest, BatchUpdateSpreadsheetRequest, ClearValuesRequest, Request,
        SheetProperties,
    },
    hyper_rustls, hyper_util, yup_oauth2,
};
use std::path::PathBuf;

const TOKEN_CACHE_PATH: &str = ".albion-accountant-token.json";

pub struct GoogleSheetsConfig {
    client_secret: PathBuf,
    spreadsheet_id: String,
    sheet_name: String,
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

pub async fn prepare_google_sheet(config: &GoogleSheetsConfig) -> Result<()> {
    eprintln!(
        "Warning: Google Sheets setup will wipe existing data from sheet '{}' in spreadsheet '{}'.",
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
    .persist_tokens_to_disk(TOKEN_CACHE_PATH)
    .build()
    .await
    .map_err(|err| DecodeError(format!("failed to initialize Google OAuth flow: {err}")))?;
    let hub = Sheets::new(sheets_client, auth);

    ensure_sheet_exists(&hub, config).await?;
    clear_sheet_values(&hub, config).await?;
    Ok(())
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

fn sheet_range(sheet_name: &str) -> String {
    format!("'{}'", sheet_name.replace('\'', "''"))
}
