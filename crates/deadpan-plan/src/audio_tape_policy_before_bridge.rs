//! Borrow established complete H serializers without consuming query results.
use super::*;
impl AudioSignalQuery<'_> {
    pub(crate) fn tape_before_query_snapshot(&self, plan: &RenderPlan) -> Value {
        query(plan, self)
    }
}
impl PlanError {
    pub(crate) fn tape_before_error_snapshot(&self) -> Value {
        error(self)
    }
}
