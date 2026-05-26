use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct AuctionGetRequests {
    pub market_order_count: usize,
    pub market_orders: Vec<Value>,
}
