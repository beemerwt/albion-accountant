pub mod EventCodes {
    pub const MARKETPLACE_BUILDING_INFO: u8 = 58;
    pub const MARKETPLACE_NOTIFICATION: u8 = 183;
}

// EventDataCode values from Albion's wire protocol enums.
pub const MARKET_EVENT_CODES: &[u8] = &[
    EventCodes::MARKETPLACE_BUILDING_INFO,
    EventCodes::MARKETPLACE_NOTIFICATION,
];

pub mod OperationCodes {
    pub const AUCTION_GET_OFFERS: u16 = 75;
    pub const AUCTION_GET_REQUESTS: u16 = 76;
}

// OperationCode values for market interactions.
pub const MARKET_OPERATION_CODES: &[u16] = &[
    OperationCodes::AUCTION_GET_OFFERS,
    OperationCodes::AUCTION_GET_REQUESTS,
];

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
