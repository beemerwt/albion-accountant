use std::{fs, path::Path};

use anyhow::{Context, Result};

const DEFAULT_HOSTS_FILE: &str = "src/albion/hosts.txt";

pub fn load_hosts(hosts_file_override: Option<&Path>) -> Result<Vec<String>> {
    let path = hosts_file_override.unwrap_or_else(|| Path::new(DEFAULT_HOSTS_FILE));
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read Albion hosts file at {}", path.display()))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}
