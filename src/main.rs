mod capture;
mod cli;
mod error;
mod google_sheets;
mod live;
#[cfg(target_os = "linux")]
mod tray;

use crate::{
    capture::process_capture,
    cli::Args,
    error::{DecodeError, Result},
    google_sheets::{GoogleSheetsClient, GoogleSheetsConfig, prepare_google_sheet},
};

#[cfg(not(target_os = "linux"))]
use crate::live::process_live_capture;
use albion_network_lib::{DecodedPacket, ExtractedPacket, models::OperationType};
use chrono::{DateTime, Local, TimeZone, Utc};
use clap::Parser;
use serde_json::{Value, json};
use std::path::Path;
use tokio::runtime::Handle;

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv()?;
    let args = Args::parse();
    let sheets_client = if !args.dry_run
        && let Some(config) = GoogleSheetsConfig::from_args(&args)?
    {
        Some(prepare_google_sheet(&config).await?)
    } else {
        None
    };

    if args.pcap_files.is_empty() {
        run_live(args, sheets_client)
    } else {
        run_replay(args, sheets_client).await
    }
}

#[derive(Clone, Copy)]
struct LiveCaptureSettings {
    debug: bool,
    all: bool,
    json: bool,
}

impl From<&Args> for LiveCaptureSettings {
    fn from(args: &Args) -> Self {
        Self {
            debug: args.debug,
            all: args.all,
            json: args.json,
        }
    }
}

#[cfg(target_os = "linux")]
fn run_live(args: Args, sheets_client: Option<GoogleSheetsClient>) -> Result<()> {
    tray::run_live_tray(
        LiveCaptureSettings::from(&args),
        sheets_client,
        Handle::current(),
    )
}

#[cfg(not(target_os = "linux"))]
fn run_live(args: Args, sheets_client: Option<GoogleSheetsClient>) -> Result<()> {
    let settings = LiveCaptureSettings::from(&args);
    let runtime_handle = Handle::current();
    process_live_capture(settings.debug, move |packet| {
        handle_live_packet(&packet, settings, sheets_client.as_ref(), &runtime_handle)
    })
}

fn handle_live_packet(
    packet: &DecodedPacket,
    settings: LiveCaptureSettings,
    sheets_client: Option<&GoogleSheetsClient>,
    runtime_handle: &Handle,
) -> Result<()> {
    if settings.all || has_structured_extract(packet) {
        print_packet(packet, settings.json)?;
    }
    if let Some(client) = sheets_client
        && let Some(row) = sheet_row_from_packet(packet)
    {
        runtime_handle.block_on(client.upsert_values(vec![row.into_values()]))?;
    }
    Ok(())
}

async fn run_replay(args: Args, sheets_client: Option<GoogleSheetsClient>) -> Result<()> {
    let mut decoded = Vec::new();
    for capture in &args.pcap_files {
        decoded.extend(process_capture(capture, args.debug)?);
    }

    if !args.all {
        decoded.retain(has_structured_extract);
    }

    if let Some(client) = sheets_client.as_ref() {
        let rows = decoded
            .iter()
            .filter_map(sheet_row_from_packet)
            .map(TradeSheetRow::into_values)
            .collect::<Vec<_>>();
        client.upsert_values(rows).await?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&decoded)?);
    } else {
        for packet in decoded {
            print_packet(&packet, false)?;
        }
    }
    Ok(())
}

fn load_dotenv() -> Result<()> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    match dotenvy::from_path(&path) {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(DecodeError(format!(
            "failed to load environment file '{}': {err}",
            path.display()
        ))),
    }
}

fn print_packet(packet: &DecodedPacket, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(packet)?);
        return Ok(());
    }

    let extracted = packet
        .extracted_json()
        .map(|value| format!(" extracted={}", serde_json::to_string(&value).unwrap()))
        .unwrap_or_default();

    match packet {
        DecodedPacket::Operation(packet) => println!(
            "{} #{} {} {} {} {}{}",
            packet.file,
            packet.packet_number,
            packet.direction,
            packet.message_type,
            packet.code as i32,
            packet.name,
            extracted
        ),
        DecodedPacket::Event(packet) => println!(
            "{} #{} {} {} {} {}{}",
            packet.file,
            packet.packet_number,
            packet.direction,
            packet.message_type,
            packet.code as i32,
            packet.name,
            extracted
        ),
        DecodedPacket::Unknown(_) => (),
    }
    Ok(())
}

fn has_structured_extract(packet: &DecodedPacket) -> bool {
    matches!(packet, DecodedPacket::Operation(operation) if matches!(
            operation.message_type.as_str(),
            "operation_request" | "operation_response"
        ) && operation.extracted.is_some())
}

#[derive(Debug, PartialEq)]
struct TradeSheetRow {
    id: String,
    date: String,
    time: String,
    location: String,
    item: String,
    debit: Option<i64>,
    credit: Option<i64>,
}

impl TradeSheetRow {
    fn into_values(self) -> Vec<Value> {
        vec![
            json!(self.id),
            json!(self.date),
            json!(self.time),
            json!(self.location),
            json!(self.item),
            optional_silver_value(self.debit),
            optional_silver_value(self.credit),
        ]
    }
}

fn utc_seconds_to_local(timestamp: i64) -> Option<DateTime<Local>> {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|timestamp| timestamp.with_timezone(&Local))
}

fn sheet_row_from_packet(packet: &DecodedPacket) -> Option<TradeSheetRow> {
    auction_trade_row_from_packet(packet).or_else(|| mail_trade_row_from_packet(packet))
}

fn mail_trade_row_from_packet(packet: &DecodedPacket) -> Option<TradeSheetRow> {
    let operation = match packet {
        DecodedPacket::Operation(operation) => operation,
        DecodedPacket::Event(_) => return None,
        DecodedPacket::Unknown(_) => return None,
    };

    let Some(ExtractedPacket::AlbionMail(mail)) = operation.extracted.as_ref() else {
        return None;
    };

    let Some(timestamp) = utc_seconds_to_local(mail.received) else {
        return None;
    };

    let location = mail
        .location
        .friendly_location_name
        .as_ref()
        .or(mail.location.location_name.as_ref())
        .cloned()
        .unwrap_or_default();

    row_from_operation(
        mail.id.to_string(),
        timestamp,
        location,
        item_label(mail.item_name.as_deref(), &mail.item_id),
        mail.total_silver,
        OperationType::from_auction_type(&mail.auction_type, &mail.trade_type),
    )
}

fn auction_trade_row_from_packet(packet: &DecodedPacket) -> Option<TradeSheetRow> {
    let operation = match packet {
        DecodedPacket::Operation(operation) => operation,
        DecodedPacket::Event(_) => return None,
        DecodedPacket::Unknown(_) => return None,
    };

    let Some(ExtractedPacket::AuctionTradeResponse(response)) = operation.extracted.as_ref() else {
        return None;
    };

    if !response.success {
        return None;
    }

    let trade = response.confirmed_trade.as_ref()?;
    let order = trade.order.as_ref()?;
    let id = trade.id.to_string();
    let silver = trade.silver_amount?;
    let location = order
        .location
        .friendly_location_name
        .as_ref()
        .or(order.location.location_name.as_ref())
        .cloned()
        .unwrap_or_default();
    let timestamp = Local::now();

    row_from_operation(
        id,
        timestamp,
        location,
        item_label(order.item_name.as_deref(), &order.item_id),
        silver,
        trade.operation.clone(),
    )
}

fn row_from_operation(
    id: String,
    timestamp: DateTime<Local>,
    location: String,
    item: String,
    silver: i64,
    operation: OperationType,
) -> Option<TradeSheetRow> {
    let date = formatted_date(timestamp);
    let time = formatted_time(timestamp);

    match operation {
        OperationType::Buy => Some(TradeSheetRow {
            id,
            date,
            time,
            location,
            item,
            debit: Some(silver),
            credit: None,
        }),
        OperationType::Sell => Some(TradeSheetRow {
            id,
            date,
            time,
            location,
            item,
            debit: None,
            credit: Some(silver),
        }),
        OperationType::Unknown(_) => None,
    }
}

fn formatted_date(timestamp: DateTime<Local>) -> String {
    timestamp.format("%m/%d/%Y").to_string()
}

fn formatted_time(timestamp: DateTime<Local>) -> String {
    timestamp.format("%I:%M %p").to_string()
}

fn item_label(item_name: Option<&str>, item_id: &str) -> String {
    item_name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(item_id)
        .to_string()
}

fn optional_silver_value(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or_else(|| json!(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use albion_network_lib::{DecodedOperation, OperationCode};
    use std::collections::BTreeMap;

    #[test]
    fn row_from_operation_maps_buy_to_debit() {
        let timestamp = Local.with_ymd_and_hms(2026, 5, 27, 21, 41, 0).unwrap();

        let row = row_from_operation(
            "14987113607".to_string(),
            timestamp,
            "Bridgewatch".to_string(),
            "T4_BAG".to_string(),
            1000,
            OperationType::Buy,
        )
        .unwrap();

        assert_eq!(row.date, "05/27/2026");
        assert_eq!(row.time, "09:41 PM");
        assert_eq!(row.id, "14987113607");
        assert_eq!(row.location, "Bridgewatch");
        assert_eq!(row.item, "T4_BAG");
        assert_eq!(row.debit, Some(1000));
        assert_eq!(row.credit, None);
    }

    #[test]
    fn row_from_operation_maps_sell_to_credit() {
        let timestamp = Local.with_ymd_and_hms(2026, 5, 27, 21, 41, 0).unwrap();

        let row = row_from_operation(
            "14987113607".to_string(),
            timestamp,
            "Bridgewatch".to_string(),
            "T4_BAG".to_string(),
            1500,
            OperationType::Sell,
        )
        .unwrap();

        assert_eq!(row.debit, None);
        assert_eq!(row.credit, Some(1500));
    }

    #[test]
    fn missing_or_unknown_rows_do_not_map() {
        assert!(sheet_row_from_packet(&empty_packet()).is_none());
        assert!(
            row_from_operation(
                "14987113607".to_string(),
                Local.with_ymd_and_hms(2026, 5, 27, 21, 41, 0).unwrap(),
                "Bridgewatch".to_string(),
                "T4_BAG".to_string(),
                1000,
                OperationType::Unknown("missing_cached_order".to_string()),
            )
            .is_none()
        );
    }

    #[test]
    fn mail_received_seconds_are_utc_instants() {
        let timestamp = utc_seconds_to_local(1_717_171_717).unwrap();

        assert_eq!(timestamp.timestamp(), 1_717_171_717);
    }

    #[test]
    fn row_values_have_expected_shape() {
        let values = TradeSheetRow {
            id: "14987113607".to_string(),
            date: "05/27/2026".to_string(),
            time: "09:41 PM".to_string(),
            location: "Bridgewatch".to_string(),
            item: "T4_BAG".to_string(),
            debit: None,
            credit: Some(1500),
        }
        .into_values();

        assert_eq!(
            values,
            vec![
                json!("14987113607"),
                json!("05/27/2026"),
                json!("09:41 PM"),
                json!("Bridgewatch"),
                json!("T4_BAG"),
                json!(""),
                json!(1500),
            ]
        );
    }

    #[test]
    fn item_label_prefers_name_and_falls_back_to_id() {
        assert_eq!(
            item_label(Some("Scraps of Hide"), "T1_HIDE"),
            "Scraps of Hide"
        );
        assert_eq!(item_label(None, "T1_HIDE"), "T1_HIDE");
        assert_eq!(item_label(Some("   "), "T1_HIDE"), "T1_HIDE");
    }

    fn empty_packet() -> DecodedPacket {
        DecodedPacket::Operation(DecodedOperation {
            file: "test".to_string(),
            packet_number: 1,
            direction: "server_to_client".to_string(),
            source: "server:5056".to_string(),
            destination: "client:1".to_string(),
            message_type: "operation_response".to_string(),
            code: OperationCode::AuctionAbortAuction,
            name: "AuctionBuyOffer".to_string(),
            return_code: Some(0),
            debug_message: String::new(),
            parameters: BTreeMap::new(),
            extracted: None,
        })
    }
}
