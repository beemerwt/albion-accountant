use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct TradeRecord {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub location: String,
    pub item: String,
    pub operation: TradeOperation,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeOperation {
    Buy,
    Sell,
}

impl TradeRecord {
    pub fn date(&self) -> String {
        self.timestamp.format("%m/%d/%Y").to_string()
    }

    pub fn time(&self) -> String {
        self.timestamp.format("%I:%M %p").to_string()
    }

    pub fn into_sheet_values(self) -> Vec<Value> {
        vec![
            json!(self.id),
            json!(self.date()),
            json!(self.time()),
            json!(self.location),
            json!(self.item),
            optional_silver_value(self.debit),
            optional_silver_value(self.credit),
        ]
    }

    pub fn operation_str(&self) -> &'static str {
        self.operation.as_str()
    }
}

impl TradeOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

impl TryFrom<&str> for TradeOperation {
    type Error = String;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "buy" => Ok(Self::Buy),
            "sell" => Ok(Self::Sell),
            other => Err(format!("unknown trade operation {other:?}")),
        }
    }
}

fn optional_silver_value(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or_else(|| json!(""))
}
