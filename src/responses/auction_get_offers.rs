use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct AuctionGetOffersResult {
    pub market_order_count: usize,
    pub market_orders: Vec<Value>,
}
