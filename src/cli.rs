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
    #[arg(
        long,
        help = "Run without authenticating with or modifying Google Sheets"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        env = "ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET",
        hide_env_values = true,
        help = "Path to the Google OAuth 2.0 client secret JSON file"
    )]
    pub client_secret: Option<PathBuf>,
    #[arg(
        long,
        env = "ALBION_ACCOUNTANT_SPREADSHEET_ID",
        hide_env_values = true,
        help = "Google spreadsheet ID this application is allowed to modify"
    )]
    pub spreadsheet_id: Option<String>,
    #[arg(
        long,
        env = "ALBION_ACCOUNTANT_SHEET_NAME",
        hide_env_values = true,
        help = "Sheet name inside the spreadsheet this application is allowed to use"
    )]
    pub sheet_name: Option<String>,
}
