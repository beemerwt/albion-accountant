use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum FilterMode {
    Broad,
    Albion,
    Custom,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, value_delimiter = ',')]
    pub interface: Vec<String>,
    #[arg(long)]
    pub all_interfaces: bool,
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
    #[arg(long)]
    pub bpf: Option<String>,
    #[arg(long, value_enum, default_value_t = FilterMode::Broad)]
    pub filter_mode: FilterMode,
    #[arg(long)]
    pub albion_hosts_file: Option<PathBuf>,
    #[arg(long)]
    pub albion_port_expr: Option<String>,
    #[arg(long)]
    pub debug_tap_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 256)]
    pub debug_tap_max_files: usize,
    #[arg(long, default_value_t = 1)]
    pub debug_tap_sample_rate: usize,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub interfaces: Vec<String>,
    pub all_interfaces: bool,
    pub list_interfaces: bool,
    pub google_client_secret: Option<PathBuf>,
    pub google_token_cache: PathBuf,
    pub spreadsheet_id: Option<String>,
    pub sheet_name: String,
    pub dry_run: bool,
    pub bpf: Option<String>,
    pub filter_mode: FilterMode,
    pub albion_hosts_file: Option<PathBuf>,
    pub albion_port_expr: Option<String>,
    pub debug_tap_dir: Option<PathBuf>,
    pub debug_tap_max_files: usize,
    pub debug_tap_sample_rate: usize,
}

impl Config {
    pub fn load() -> Result<Self> {
        let args = Args::parse();
        Ok(Self {
            interfaces: if args.interface.is_empty() {
                std::env::var("ALBION_ACCOUNTANT_INTERFACE")
                    .ok()
                    .into_iter()
                    .collect()
            } else {
                args.interface
            },
            all_interfaces: args.all_interfaces,
            list_interfaces: args.list_interfaces,
            google_client_secret: args.google_client_secret,
            google_token_cache: args.google_token_cache,
            spreadsheet_id: args.spreadsheet_id,
            sheet_name: args.sheet_name,
            dry_run: args.dry_run,
            bpf: args.bpf,
            filter_mode: args.filter_mode,
            albion_hosts_file: args.albion_hosts_file,
            albion_port_expr: args.albion_port_expr,
            debug_tap_dir: args.debug_tap_dir,
            debug_tap_max_files: args.debug_tap_max_files,
            debug_tap_sample_rate: args.debug_tap_sample_rate.max(1),
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

    use super::{Args, Config, FilterMode};

    #[test]
    fn cli_overrides_env_interface() {
        temp_env::with_var("ALBION_ACCOUNTANT_INTERFACE", Some("eth0"), || {
            let args = Args::parse_from(["x", "--interface", "wlan0"]);
            assert_eq!(args.interface, vec!["wlan0"]);
        });
    }

    #[test]
    fn interface_allows_csv_or_repeated_values() {
        let args = Args::parse_from(["x", "--interface", "eth0,wlan0", "--interface", "en0"]);
        assert_eq!(args.interface, vec!["eth0", "wlan0", "en0"]);
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
    fn filter_mode_defaults_to_broad() {
        let args = Args::parse_from(["x"]);
        assert_eq!(args.filter_mode, FilterMode::Broad);
    }

    #[test]
    fn dry_run_does_not_require_google_config() {
        let config = Config {
            interfaces: vec![],
            all_interfaces: false,
            list_interfaces: false,
            google_client_secret: None,
            google_token_cache: ".albion-accountant-token.json".into(),
            spreadsheet_id: None,
            sheet_name: "Sheet1".to_string(),
            dry_run: true,
            bpf: None,
            filter_mode: FilterMode::Broad,
            albion_hosts_file: None,
            albion_port_expr: None,
            debug_tap_dir: None,
            debug_tap_max_files: 256,
            debug_tap_sample_rate: 1,
        };

        assert!(config.validate_google_config().is_ok());
    }

    #[test]
    fn non_dry_run_requires_google_config() {
        let config = Config {
            interfaces: vec![],
            all_interfaces: false,
            list_interfaces: false,
            google_client_secret: None,
            google_token_cache: ".albion-accountant-token.json".into(),
            spreadsheet_id: None,
            sheet_name: "Sheet1".to_string(),
            dry_run: false,
            bpf: None,
            filter_mode: FilterMode::Broad,
            albion_hosts_file: None,
            albion_port_expr: None,
            debug_tap_dir: None,
            debug_tap_max_files: 256,
            debug_tap_sample_rate: 1,
        };

        assert!(config.validate_google_config().is_err());
    }
}
