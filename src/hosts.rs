use crate::error::Result;
use std::{fs, net::IpAddr, path::Path, str::FromStr};

pub struct HostFilter {
    ranges: Vec<CidrRange>,
}

impl HostFilter {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut ranges = Vec::new();

        for (line_number, line) in content.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            ranges.push(CidrRange::from_str(line).map_err(|message| {
                format!(
                    "{}:{} invalid CIDR entry {:?}: {}",
                    path.display(),
                    line_number + 1,
                    line,
                    message
                )
            })?);
        }

        Ok(Self { ranges })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        self.ranges.iter().any(|range| range.contains(ip))
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }
}

enum CidrRange {
    V4 { network: u32, mask: u32 },
    V6 { network: u128, mask: u128 },
}

impl CidrRange {
    fn contains(&self, ip: IpAddr) -> bool {
        match (self, ip) {
            (Self::V4 { network, mask }, IpAddr::V4(ip)) => (u32::from(ip) & mask) == *network,
            (Self::V6 { network, mask }, IpAddr::V6(ip)) => (u128::from(ip) & mask) == *network,
            _ => false,
        }
    }
}

impl FromStr for CidrRange {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (address, prefix) = value
            .split_once('/')
            .ok_or_else(|| "missing /prefix".to_string())?;
        let ip = address
            .parse::<IpAddr>()
            .map_err(|error| format!("invalid IP address: {error}"))?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(|error| format!("invalid prefix length: {error}"))?;

        match ip {
            IpAddr::V4(ip) => {
                if prefix > 32 {
                    return Err("IPv4 prefix length must be <= 32".to_string());
                }
                let mask = prefix_mask_u32(prefix);
                Ok(Self::V4 {
                    network: u32::from(ip) & mask,
                    mask,
                })
            }
            IpAddr::V6(ip) => {
                if prefix > 128 {
                    return Err("IPv6 prefix length must be <= 128".to_string());
                }
                let mask = prefix_mask_u128(prefix);
                Ok(Self::V6 {
                    network: u128::from(ip) & mask,
                    mask,
                })
            }
        }
    }
}

fn prefix_mask_u32(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn prefix_mask_u128(prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    }
}
