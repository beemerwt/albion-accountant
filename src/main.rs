mod capture;
mod cli;
mod error;
mod google_sheets;
mod live;

use crate::{
    capture::process_capture,
    cli::Args,
    error::{DecodeError, Result},
    google_sheets::{GoogleSheetsConfig, prepare_google_sheet},
    live::process_live_capture,
};
use albion_network_lib::{DecodedPacket, ExtractedPacket, models::OperationType};
use chrono::{DateTime, Local};
use clap::Parser;
use serde_json::{Value, json};
use std::path::Path;

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
        let runtime_handle = tokio::runtime::Handle::current();
        return process_live_capture(args.debug, move |packet| {
            if args.all || has_structured_extract(&packet) {
                print_packet(&packet, args.json)?;
            }
            if let Some(client) = sheets_client.as_ref()
                && let Some(row) = trade_row_from_packet(&packet)
            {
                tokio::task::block_in_place(|| {
                    runtime_handle.block_on(client.append_values(vec![row.into_values()]))
                })?;
            }
            Ok(())
        });
    }

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
            .filter_map(trade_row_from_packet)
            .map(TradeSheetRow::into_values)
            .collect::<Vec<_>>();
        client.append_values(rows).await?;
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
        .extracted
        .as_ref()
        .map(|value| format!(" extracted={}", serde_json::to_string(value).unwrap()))
        .unwrap_or_default();
    println!(
        "{} #{} {} {} {} {}{}",
        packet.file,
        packet.packet_number,
        packet.direction,
        packet.message_type,
        packet.code,
        packet.name,
        extracted
    );
    Ok(())
}

fn has_structured_extract(packet: &DecodedPacket) -> bool {
    matches!(
        packet.message_type.as_str(),
        "operation_request" | "operation_response"
    ) && packet.extracted.is_some()
}

#[derive(Debug, PartialEq)]
struct TradeSheetRow {
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
            json!(self.date),
            json!(self.time),
            json!(self.location),
            json!(self.item),
            optional_silver_value(self.debit),
            optional_silver_value(self.credit),
        ]
    }
}

fn trade_row_from_packet(packet: &DecodedPacket) -> Option<TradeSheetRow> {
    let Some(ExtractedPacket::AuctionTradeResponse(response)) = packet.extracted.as_ref() else {
        return None;
    };
    if !response.success {
        return None;
    }

    let trade = response.confirmed_trade.as_ref()?;
    let order = trade.order.as_ref()?;
    let silver = trade.silver_amount?;
    let location = order
        .friendly_location_name
        .as_ref()
        .or(order.location_name.as_ref())
        .cloned()
        .unwrap_or_default();
    let timestamp = Local::now();
    let date = formatted_date(timestamp);
    let time = formatted_time(timestamp);

    match trade.operation {
        OperationType::Buy => Some(TradeSheetRow {
            date,
            time,
            location,
            item: order.item_type_id.clone(),
            debit: Some(silver),
            credit: None,
        }),
        OperationType::Sell => Some(TradeSheetRow {
            date,
            time,
            location,
            item: order.item_type_id.clone(),
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

fn optional_silver_value(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or_else(|| json!(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use albion_network_lib::{
        PhotonParser,
        models::{AuctionType, CachedOrder, TradeType},
        responses::auction_trade::{AuctionTrade, AuctionTradeResponse},
    };
    use std::collections::BTreeMap;

    #[test]
    fn app_can_instantiate_network_parser() {
        let parser = PhotonParser::new("smoke".to_string(), false);

        assert_eq!(parser.decoded_packets().len(), 0);
        assert_eq!(parser.market_order_count(), 0);
    }

    #[test]
    fn sell_trade_maps_to_credit() {
        let packet = trade_packet(OperationType::Sell, Some(1500), Some(order()));

        let row = trade_row_from_packet(&packet).unwrap();

        assert_eq!(row.location, "Bridgewatch");
        assert_eq!(row.item, "T4_BAG");
        assert_eq!(row.debit, None);
        assert_eq!(row.credit, Some(1500));
    }

    #[test]
    fn buy_trade_maps_to_debit() {
        let packet = trade_packet(OperationType::Buy, Some(1000), Some(order()));

        let row = trade_row_from_packet(&packet).unwrap();

        assert_eq!(row.debit, Some(1000));
        assert_eq!(row.credit, None);
    }

    #[test]
    fn incomplete_or_unknown_trades_do_not_map_to_rows() {
        assert!(
            trade_row_from_packet(&trade_packet(OperationType::Sell, None, Some(order())))
                .is_none()
        );
        assert!(
            trade_row_from_packet(&trade_packet(OperationType::Sell, Some(1000), None)).is_none()
        );
        assert!(
            trade_row_from_packet(&trade_packet(
                OperationType::Unknown("missing_cached_order".to_string()),
                Some(1000),
                Some(order()),
            ))
            .is_none()
        );
    }

    #[test]
    fn location_falls_back_to_location_name() {
        let mut order = order();
        order.friendly_location_name = None;
        order.location_name = Some("Fort Sterling".to_string());

        let row = trade_row_from_packet(&trade_packet(OperationType::Sell, Some(500), Some(order)))
            .unwrap();

        assert_eq!(row.location, "Fort Sterling");
    }

    #[test]
    fn row_values_have_expected_shape() {
        let values = TradeSheetRow {
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
                json!("05/27/2026"),
                json!("09:41 PM"),
                json!("Bridgewatch"),
                json!("T4_BAG"),
                json!(""),
                json!(1500),
            ]
        );
    }

    fn trade_packet(
        operation: OperationType,
        silver_amount: Option<i64>,
        order: Option<CachedOrder>,
    ) -> DecodedPacket {
        DecodedPacket {
            file: "test".to_string(),
            packet_number: 1,
            direction: "server_to_client".to_string(),
            source: "server:5056".to_string(),
            destination: "client:1".to_string(),
            message_type: "operation_response".to_string(),
            code: 85,
            name: "AuctionBuyOffer".to_string(),
            return_code: Some(0),
            debug_message: String::new(),
            parameters: BTreeMap::new(),
            extracted: Some(ExtractedPacket::AuctionTradeResponse(
                AuctionTradeResponse {
                    confirmed_trade: Some(AuctionTrade {
                        amount: Some(1),
                        silver_amount,
                        operation,
                        trade_type: TradeType::Instant,
                        order,
                        order_id: Some(1),
                    }),
                    success: true,
                },
            )),
        }
    }

    fn order() -> CachedOrder {
        CachedOrder {
            amount: 10,
            auction_type: AuctionType::Offer,
            buyer_character_id: None,
            buyer_name: None,
            distance_fee: 0,
            enchantment_level: 0,
            expires: "soon".to_string(),
            has_buyer_fetched: false,
            has_seller_fetched: false,
            id: 1,
            is_finished: false,
            item_group_type_id: "T4_BAG".to_string(),
            item_type_id: "T4_BAG".to_string(),
            location_id: Some("2000".to_string()),
            location_name: Some("Bridgewatch".to_string()),
            friendly_location_name: Some("Bridgewatch".to_string()),
            quality_level: 1,
            reference_id: "ref".to_string(),
            seller_character_id: None,
            seller_name: None,
            tier: 4,
            total_price_silver: 5000,
            unit_price_silver: 500,
        }
    }
}
