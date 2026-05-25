use serde_json::Value;
use std::collections::BTreeMap;
pub fn extract_market_orders(parameters: &BTreeMap<i32, Value>) -> Vec<Value> {
    match parameters.get(&0) {
        Some(Value::Array(v)) => v.clone(),
        Some(v) => vec![v.clone()],
        None => vec![],
    }
}
