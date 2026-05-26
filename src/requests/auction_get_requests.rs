use crate::models::CachedOrder;
use serde::Serialize;

#[derive(Serialize)]
pub struct AuctionGetRequests {
    pub market_order_count: usize,
    pub market_orders: Vec<CachedOrder>,
}
