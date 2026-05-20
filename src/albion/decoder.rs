use regex::Regex;

use super::transaction::MarketTransaction;

pub fn decode_transaction(packet: &[u8]) -> Option<MarketTransaction> {
    // Reference behavior from AlbionDataAvalonia: market trade history arrives in Photon UDP traffic.
    // This parser intentionally extracts only a minimal transaction-shaped payload from UTF-8 fragments.
    let text = std::str::from_utf8(packet).ok()?;
    let re = Regex::new(r#"location=(?P<location>[A-Za-z_]+);item=(?P<item>[A-Z0-9_@.]+);qty=(?P<qty>\d+);price=(?P<price>\d+)"#).ok()?;
    let captures = re.captures(text)?;
    let location = captures.name("location")?.as_str().to_string();
    let item = captures.name("item")?.as_str().to_string();
    let qty: u32 = captures.name("qty")?.as_str().parse().ok()?;
    let price: u64 = captures.name("price")?.as_str().parse().ok()?;
    MarketTransaction::new(location, item, qty, price, None).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_packet() {
        let data = b"xxx location=Martlock;item=T4_BAG;qty=3;price=1250 yyy";
        let tx = decode_transaction(data).unwrap();
        assert_eq!(tx.location, "Martlock");
        assert_eq!(tx.total_cost, 3750);
    }
}
