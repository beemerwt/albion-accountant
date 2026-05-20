pub const MARKET_EVENT_CODES: &[u8] = &[0x00, 0x2a, 0x2b];
pub const MARKET_OPERATION_CODES: &[u8] = &[0x00, 0x5a];
pub const SUCCESS_RETURN_CODES: &[i16] = &[0];

pub const KEY_PARAMS: &str = "params";
pub const KEY_EVENT_CODE: &str = "code";
pub const KEY_OP_CODE: &str = "op_code";
pub const KEY_RETURN_CODE: &str = "return_code";

pub const ITEM_ID_KEYS: &[&str] = &["item", "item_id", "itemType", "itemtype"];
pub const QUANTITY_KEYS: &[&str] = &["qty", "quantity", "amount"];
pub const SILVER_KEYS: &[&str] = &["price", "silver", "unit_price", "cost"];
pub const LOCATION_KEYS: &[&str] = &["location", "city", "market", "cluster"];
