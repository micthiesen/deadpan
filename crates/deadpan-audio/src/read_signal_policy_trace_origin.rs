//! Test-only routing keeps ordinary/key and native origins independent.
use super::{read_signal_policy_before_trace as before, read_signal_policy_native_trace as native};

#[derive(Clone, Copy, Default)]
pub(super) struct TraceOrigin {
    before: Option<before::Ticket>,
    native: Option<native::Ticket>,
}
impl TraceOrigin {
    pub(super) const NONE: Self = Self {
        before: None,
        native: None,
    };
    pub(super) fn new(before: Option<before::Ticket>, native: Option<native::Ticket>) -> Self {
        Self { before, native }
    }
    pub(super) fn before(self) -> Option<before::Ticket> {
        self.before
    }
    pub(super) fn native(self) -> Option<native::Ticket> {
        self.native
    }
}
