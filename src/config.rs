use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long)]
    pub interface: Option<String>,
    #[arg(long)]
    pub list_interfaces: bool,
    #[arg(long, env = "ALBION_ACCOUNTANT_GOOGLE_CREDENTIALS")]
    pub google_credentials: Option<PathBuf>,
    #[arg(long, env = "ALBION_ACCOUNTANT_SPREADSHEET_ID")]
    pub spreadsheet_id: Option<String>,
    #[arg(long, env = "ALBION_ACCOUNTANT_SHEET_NAME", default_value = "Sheet1")]
    pub sheet_name: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub interface: Option<String>,
    pub list_interfaces: bool,
    pub google_credentials: Option<PathBuf>,
    pub spreadsheet_id: Option<String>,
    pub sheet_name: String,
    pub dry_run: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let args = Args::parse();
        Ok(Self {
            interface: args
                .interface
                .or_else(|| std::env::var("ALBION_ACCOUNTANT_INTERFACE").ok()),
            list_interfaces: args.list_interfaces,
            google_credentials: args.google_credentials,
            spreadsheet_id: args.spreadsheet_id,
            sheet_name: args.sheet_name,
            dry_run: args.dry_run,
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Args;

    #[test]
    fn cli_overrides_env() {
        temp_env::with_var("ALBION_ACCOUNTANT_INTERFACE", Some("eth0"), || {
            let args = Args::parse_from(["x", "--interface", "wlan0"]);
            assert_eq!(args.interface.as_deref(), Some("wlan0"));
        });
    }
}
