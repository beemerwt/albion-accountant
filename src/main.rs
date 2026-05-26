mod capture;
mod cli;
mod error;
mod event_codes;
mod hosts;
mod live;
mod names;
mod operation_codes;
mod packet;
mod pcap;
mod photon;
mod protocol18;
mod requests;
mod responses;
mod util;

use crate::{capture::process_capture, cli::Args, error::Result, live::process_live_capture};
use clap::Parser;
use packet::DecodedPacket;

fn main() -> Result<()> {
    let args = Args::parse();
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
