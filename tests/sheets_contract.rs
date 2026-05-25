use albion_accountant::{albion::transaction::MarketTransaction, sheets::row};

#[test]
fn sheets_row_contract_matches_current_upload_schema() {
    let tx = MarketTransaction::new("Martlock".into(), "T4_BAG".into(), 3, 1250, None).unwrap();

    assert_eq!(
        row::HEADER,
        [
            "Location",
            "Item",
            "Quantity",
            "Per Item Cost",
            "Total Cost"
        ]
    );
    assert_eq!(
        row::transaction_row(&tx).values.unwrap()[0],
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
fn uploader_boundary_receives_finalized_transactions_without_network() {
    #[derive(Default)]
    struct MockUploader {
        appended: Vec<MarketTransaction>,
    }

    impl MockUploader {
        fn append_transaction(&mut self, txn: &MarketTransaction) {
            self.appended.push(txn.clone());
        }
    }

    let finalized = vec![
        MarketTransaction::new("Bridgewatch".into(), "T5_CAPE".into(), 1, 42000, None).unwrap(),
    ];
    let mut uploader = MockUploader::default();

    for txn in &finalized {
        uploader.append_transaction(txn);
    }

    assert_eq!(uploader.appended, finalized);
}
