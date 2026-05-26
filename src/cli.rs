use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Decode Albion Photon Protocol18 operations and events from pcapng captures.")]
pub struct Args {
    #[arg(default_values = ["full_market_quick_buy.pcapng", "full_market_quick_sell.pcapng"])]
    pub captures: Vec<PathBuf>,
    #[arg(long, help = "Print decoded packets as JSON")]
    pub json: bool,
    #[arg(long, help = "Print Photon command debugging details")]
    pub debug: bool,
    #[arg(long, help = "Output all decoded packets, including events")]
    pub all: bool,
    #[arg(long, help = "Capture live traffic from every available interface")]
    pub live: bool,
}
