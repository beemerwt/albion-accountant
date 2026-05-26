use crate::models::CachedOrder;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct AuctionBuyOffer {
    pub amount: Option<i64>,
    pub cached_order: Option<CachedOrder>,
    pub order_id: Option<i64>,
}
