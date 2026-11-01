//! Complete borrowed projection fields for the bounded Tape-input fixtures.
use super::*;
impl AudioStageProjection<'_> {
    pub(crate) fn provider_before_snapshot(&self, plan: &RenderPlan) -> serde_json::Value {
        assert!(self.belongs_to(plan));
        let Self {
            stage,
            input,
            output_policy,
            duration,
            identity,
            source_key,
            owned_key,
        } = self;
        let AudioProjectionIdentity(token) = identity;
        let ProjectionIdentityToken {
            _private: token_value,
        } = token.as_ref();
        let input = match input {
            ProjectionInput::Tape(tape) => {
                serde_json::json!({"tag":"Tape","value":tape.provider_before_snapshot(plan)})
            }
            ProjectionInput::Mix(_) => panic!(
                "provider BEFORE fixture reached Mix: add exhaustive borrowed mix snapshot before publishing"
            ),
        };
        serde_json::json!({"stage":stage.provider_before_stage_snapshot(plan),"input":input,
        "output_policy":output_policy.provider_before_snapshot(plan),"duration":duration,
        "identity":{"private_token_value":token_value},
        "source_key":source_key.as_ref().map(|key|{
            let AudioSourceProjectionKey { recipe, instance, unclipped_before, stage } = key.as_ref();
            serde_json::json!({"recipe":{"source":recipe.source,"mapping":recipe.mapping,"offset":recipe.offset},
                "instance":instance,"unclipped_before":unclipped_before,"stage":stage})
        }),
        "owned_key":owned_key.as_ref().map(|key|{
            let AudioOwnedProjectionKey { definition, instance }=key.as_ref();
            serde_json::json!({"definition":definition,"instance":instance})
        })})
    }
}
