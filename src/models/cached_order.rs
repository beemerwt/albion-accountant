use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CachedOrder {
    pub amount: i64,
    pub auction_type: String,
    pub buyer_character_id: Option<String>,
    pub buyer_name: Option<String>,
    pub distance_fee: i64,
    pub enchantment_level: i64,
    pub expires: String,
    pub has_buyer_fetched: bool,
    pub has_seller_fetched: bool,
    pub id: i64,
    pub is_finished: bool,
    pub item_group_type_id: String,
    pub item_type_id: String,
    pub location_id: Option<i64>,
    pub quality_level: i64,
    pub reference_id: String,
    pub seller_character_id: Option<String>,
    pub seller_name: Option<String>,
    pub tier: i64,
    pub total_price_silver: i64,
    pub unit_price_silver: i64,
}

#[cfg(test)]
mod tests {
    use super::CachedOrder;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct CachedOrderFixture {
        cached_order: CachedOrder,
    }

    #[derive(Deserialize)]
    struct MarketOrdersFixture {
        market_orders: Vec<CachedOrder>,
    }

    #[test]
    fn parses_cached_order_request_examples() {
        let buy_offer: CachedOrderFixture = serde_json::from_str(include_str!(
            "../../examples/auction_buy_offer_request.json"
        ))
        .unwrap();
        let sell_specific_item: CachedOrderFixture = serde_json::from_str(include_str!(
            "../../examples/auction_sell_specific_item_request.json"
        ))
        .unwrap();

        assert_eq!(buy_offer.cached_order.id, 14978117778);
        assert_eq!(sell_specific_item.cached_order.id, 14977174637);
    }

    #[test]
    fn parses_market_orders_response_example() {
        let get_requests: MarketOrdersFixture = serde_json::from_str(include_str!(
            "../../examples/auction_get_requests_response.json"
        ))
        .unwrap();

        assert_eq!(get_requests.market_orders.len(), 1);
        assert_eq!(get_requests.market_orders[0].id, 14977174637);
    }
}
