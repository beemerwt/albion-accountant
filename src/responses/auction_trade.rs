use crate::models::CachedOrder;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct AuctionTrade {
    pub amount: Option<i64>,
    pub operation: &'static str,
    pub order: Option<CachedOrder>,
    pub order_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AuctionTradeResponse {
    pub confirmed_trade: Option<AuctionTrade>,
    pub success: bool,
}
