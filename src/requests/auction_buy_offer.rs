use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct AuctionBuyOffer {
    pub amount: Option<i64>,
    pub cached_order: Value,
    pub order_id: Option<i64>,
}
