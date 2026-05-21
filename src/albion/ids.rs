// EventDataCode values from Albion's wire protocol enums.
// - 58: MarketPlaceBuildingInfo
// - 183: MarketPlaceNotification
pub const MARKET_EVENT_CODES: &[u8] = &[58, 183];

// OperationCode values for market interactions.
// Core market operations:
// - 81: AuctionGetOffers
// - 82: AuctionGetRequests
// - 83: AuctionBuyOffer
// - 88: AuctionSellRequest
// - 95: AuctionGetItemAverageStats
// Modern quick-sell/loadout flows:
// - 454: AuctionGetLoadoutOffers
// - 455: AuctionBuyLoadoutOffer
// - 484: QuickSellAuctionQueryAction
// - 485: QuickSellAuctionSellAction
pub const MARKET_OPERATION_CODES: &[u16] = &[81, 82, 83, 88, 95, 454, 455, 484, 485];

// ReturnCode.Success only; non-zero return codes are ignored by design.
pub const SUCCESS_RETURN_CODES: &[i16] = &[0];

pub const KEY_PARAMS: &str = "params";
pub const KEY_EVENT_CODE: &str = "code";
pub const KEY_OP_CODE: &str = "op_code";
pub const KEY_RETURN_CODE: &str = "return_code";

pub const LOCATION_KEY: &str = "LocationId";
pub const ITEM_ID_KEY: &str = "ItemTypeId";
pub const QUANTITY_KEY: &str = "Amount";
pub const SILVER_KEY: &str = "UnitPriceSilver";

pub const LOCATION_KEY_ALIASES: &[&str] = &["location"];
pub const ITEM_ID_KEY_ALIASES: &[&str] = &["item"];
pub const QUANTITY_KEY_ALIASES: &[&str] = &["qty"];
pub const SILVER_KEY_ALIASES: &[&str] = &["price"];
