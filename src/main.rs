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
use albion_network_lib::DecodedPacket;
use clap::Parser;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv()?;
    let args = Args::parse();
    if !args.dry_run
        && let Some(config) = GoogleSheetsConfig::from_args(&args)?
    {
        prepare_google_sheet(&config).await?;
    }

    if args.live {
        return process_live_capture(args.debug, |packet| {
            if args.all || has_structured_extract(&packet) {
                print_packet(&packet, args.json)?;
            }
            Ok(())
        });
    }

    let mut decoded = Vec::new();
    for capture in &args.captures {
        decoded.extend(process_capture(capture, args.debug)?);
    }

    if !args.all {
        decoded.retain(has_structured_extract);
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

#[cfg(test)]
mod tests {
    use albion_network_lib::PhotonParser;

    #[test]
    fn app_can_instantiate_network_parser() {
        let parser = PhotonParser::new("smoke".to_string(), false);

        assert_eq!(parser.decoded_packets().len(), 0);
        assert_eq!(parser.market_order_count(), 0);
    }
}
