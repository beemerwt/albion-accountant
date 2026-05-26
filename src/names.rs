pub fn operation(code: i32) -> Option<crate::operation_codes::OperationCode> {
    crate::operation_codes::OperationCode::try_from(code).ok()
}

pub fn event(code: i32) -> Option<crate::event_codes::EventCode> {
    crate::event_codes::EventCode::try_from(code).ok()
}
