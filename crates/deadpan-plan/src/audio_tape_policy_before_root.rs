//! Read only the policy Tape actually constructed by the public Root factory.
use super::*;
impl<'plan> AudioProjectedRoot<'plan> {
    pub(crate) fn tape_before_visit(&self, visit: impl FnOnce(&AudioSignalTape<'plan>)) {
        visit(&self.policy)
    }
    pub(crate) fn tape_before_snapshot(&self, plan: &RenderPlan) -> serde_json::Value {
        let Self {
            projection,
            placement,
            extent,
            meaningful_extent,
            samples,
            meaningful_samples,
            sampling,
            reference_sample_at_anchor,
            root_grid,
            policy,
        } = self;
        assert!(self.belongs_to(plan));
        serde_json::json!({"projection":projection.provider_before_snapshot(plan),"placement":placement,
            "extent":extent,"meaningful_extent":meaningful_extent,"samples":samples,
            "meaningful_samples":meaningful_samples,"sampling":sampling,
            "reference_sample_at_anchor":reference_sample_at_anchor,"root_grid":root_grid,
            "policy":policy.provider_before_snapshot(plan)})
    }
}
