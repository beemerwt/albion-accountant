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
    fn row_conversion_has_expected_five_columns() {
        let tx = MarketTransaction::new("Martlock".into(), "T4_BAG".into(), 3, 1250, None).unwrap();
        let row = super::transaction_row(&tx);
        assert_eq!(
            row.values.unwrap()[0],
            vec![
                serde_json::Value::String("Martlock".into()),
                serde_json::Value::String("T4_BAG".into()),
                serde_json::Value::String("3".into()),
                serde_json::Value::String("1250".into()),
                serde_json::Value::String("3750".into()),
            ]
        );
    }

    #[test]
    fn header_row_has_expected_five_columns() {
        let row = super::header_row();
        assert_eq!(
            row.values.unwrap()[0],
            vec![
                serde_json::Value::String("Location".into()),
                serde_json::Value::String("Item".into()),
                serde_json::Value::String("Quantity".into()),
                serde_json::Value::String("Per Item Cost".into()),
                serde_json::Value::String("Total Cost".into()),
            ]
        );
    }
}
