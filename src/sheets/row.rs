use google_sheets4::api::ValueRange;

use crate::albion::transaction::MarketTransaction;

pub const HEADER: [&str; 5] = [
    "Location",
    "Item",
    "Quantity",
    "Per Item Cost",
    "Total Cost",
];

pub fn header_row() -> ValueRange {
    ValueRange {
        values: Some(vec![
            HEADER
                .iter()
                .map(|s| serde_json::Value::String((*s).to_string()))
                .collect(),
        ]),
        ..Default::default()
    }
}

pub fn transaction_row(txn: &MarketTransaction) -> ValueRange {
    ValueRange {
        values: Some(vec![vec![
            serde_json::Value::String(txn.location.clone()),
            serde_json::Value::String(txn.item.clone()),
            serde_json::Value::String(txn.quantity.to_string()),
            serde_json::Value::String(txn.per_item_cost.to_string()),
            serde_json::Value::String(txn.total_cost.to_string()),
        ]]),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use crate::albion::transaction::MarketTransaction;

    #[test]
    fn row_conversion() {
        let tx = MarketTransaction::new("Martlock".into(), "T4_BAG".into(), 3, 1250, None).unwrap();
        let row = super::transaction_row(&tx);
        assert_eq!(
            row.values.unwrap()[0][4],
            serde_json::Value::String("3750".into())
        );
    }
}
