//! Borrow existing checked Tape children without manufacturing a provider.
use super::*;
impl<'plan> AudioSignalTape<'plan> {
    pub(crate) fn provider_before_snapshot(&self, plan: &RenderPlan) -> serde_json::Value {
        assert!(self.belongs_to(plan));
        let Self {
            plan: _,
            support,
            route,
            grid,
            runs,
        } = self;
        serde_json::json!({"belongs_to_expected_plan":true,"support":support,"route":route,
            "grid":grid,"runs_capacity":runs.capacity(),
            "runs":runs.iter().map(|run| {
                let PlacedRun { destination, samples, signal, projection, policy_override } = run;
                serde_json::json!({"destination":destination,"samples":samples,
                    "signal":signal.provider_before_snapshot(plan),
                    "projection":projection.as_ref().map(|p|p.provider_before_snapshot(plan)),
                    "policy_override":policy_override.as_ref().map(|p|p.provider_before_snapshot(plan))})
            }).collect::<Vec<_>>()})
    }
    pub(crate) fn provider_before_visit(
        &self,
        mut visit: impl FnMut(usize, &AudioSignal<'plan>, Range<SignalSample>),
    ) {
        for (index, run) in self.runs.iter().enumerate() {
            visit(index, &run.signal, run.samples.clone());
        }
    }
}
