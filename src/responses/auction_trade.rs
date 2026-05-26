use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize)]
pub struct AuctionTrade {
    pub amount: Option<i64>,
    pub operation: &'static str,
    pub order: Value,
    pub order_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AuctionTradeResponse {
    pub confirmed_trade: Option<AuctionTrade>,
    pub success: bool,
}
