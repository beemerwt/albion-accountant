use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct AuctionGetOffers {
    pub market_order_count: usize,
    pub market_orders: Vec<Value>,
}
