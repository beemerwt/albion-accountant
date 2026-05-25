use crate::albion::event_codes::EventCodes;
use crate::albion::operation_codes::OperationCodes;

// EventDataCode values from Albion's wire protocol enums.
pub const MARKET_EVENT_CODES: &[u16] = &[
    EventCodes::MarketPlaceBuildingInfo as u16,
    EventCodes::MarketPlaceNotification as u16,
];

// OperationCode values for market interactions.
pub const MARKET_OPERATION_CODES: &[u16] = &[
    OperationCodes::AuctionGetOffers as u16,
    OperationCodes::AuctionGetRequests as u16,
    OperationCodes::AuctionBuyOffer as u16,
    OperationCodes::AuctionSellSpecificItemRequest as u16,
    OperationCodes::QuickSellAuctionSellAction as u16,
];

// ReturnCode.Success only; non-zero return codes are ignored by design.
pub const SUCCESS_RETURN_CODES: &[i16] = &[0];

pub const KEY_PARAMS: &str = "params";
pub const KEY_EVENT_CODE: &str = "code";
pub const KEY_OP_CODE: &str = "op_code";
pub const KEY_RETURN_CODE: &str = "return_code";
