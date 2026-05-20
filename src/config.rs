use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long)]
    pub interface: Option<String>,
    #[arg(long)]
    pub list_interfaces: bool,
    #[arg(long, env = "ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET")]
    pub google_client_secret: Option<PathBuf>,
    #[arg(
        long,
        env = "ALBION_ACCOUNTANT_GOOGLE_TOKEN_CACHE",
        default_value = ".albion-accountant-token.json"
    )]
    pub google_token_cache: PathBuf,
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
    pub google_client_secret: Option<PathBuf>,
    pub google_token_cache: PathBuf,
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
            google_client_secret: args.google_client_secret,
            google_token_cache: args.google_token_cache,
            spreadsheet_id: args.spreadsheet_id,
            sheet_name: args.sheet_name,
            dry_run: args.dry_run,
        })
    }

    pub fn validate_google_config(&self) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }
        if self.google_client_secret.is_none() {
            bail!("missing --google-client-secret or ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET");
        }
        if self.spreadsheet_id.is_none() {
            bail!("missing --spreadsheet-id or ALBION_ACCOUNTANT_SPREADSHEET_ID");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser;

    use super::{Args, Config};

    #[test]
    fn cli_overrides_env_interface() {
        temp_env::with_var("ALBION_ACCOUNTANT_INTERFACE", Some("eth0"), || {
            let args = Args::parse_from(["x", "--interface", "wlan0"]);
            assert_eq!(args.interface.as_deref(), Some("wlan0"));
        });
    }

    #[test]
    fn cli_overrides_env_google_client_secret() {
        temp_env::with_var(
            "ALBION_ACCOUNTANT_GOOGLE_CLIENT_SECRET",
            Some("./from-env.json"),
            || {
                let args = Args::parse_from(["x", "--google-client-secret", "./from-cli.json"]);
                assert_eq!(
                    args.google_client_secret.as_deref(),
                    Some(Path::new("./from-cli.json"))
                );
            },
        );
    }

    #[test]
    fn dry_run_does_not_require_google_config() {
        let config = Config {
            interface: None,
            list_interfaces: false,
            google_client_secret: None,
            google_token_cache: ".albion-accountant-token.json".into(),
            spreadsheet_id: None,
            sheet_name: "Sheet1".to_string(),
            dry_run: true,
        };

        assert!(config.validate_google_config().is_ok());
    }

    #[test]
    fn non_dry_run_requires_google_config() {
        let config = Config {
            interface: None,
            list_interfaces: false,
            google_client_secret: None,
            google_token_cache: ".albion-accountant-token.json".into(),
            spreadsheet_id: None,
            sheet_name: "Sheet1".to_string(),
            dry_run: false,
        };

        assert!(config.validate_google_config().is_err());
    }
}
