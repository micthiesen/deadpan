//! Read-only, feature-gated observations of the production Trim draft.

use super::*;
use serde_json::{Value, json};

impl Capture {
    pub(in crate::preview) fn state_for_check(&self) -> Value {
        json!({"session":self.target.session,"project":self.target.project,
            "base_revision":self.target.base_revision,"parent":self.target.parent,
            "node":self.target.node,"right":self.target.right,"scope":self.target.scope.groups(),
            "range":[self.target.range.start().0,self.target.range.end().0],"cursor":self.target.cursor.0,
            "source_cursor":self.source_cursor,"pane":format!("{:?}",self.pane),"view":format!("{:?}",self.view)})
    }
}

impl Draft {
    pub(in crate::preview) fn prepared_for_check(
        &self,
    ) -> Option<&Arc<crate::project::trim::Prepared>> {
        self.input.ready()
    }

    pub(in crate::preview) fn identity_for_check(
        &self,
    ) -> Option<&crate::worker::EditJunctionIdentity> {
        self.inspection
            .as_ref()
            .map(|inspection| &inspection.identity)
    }

    pub(in crate::preview) fn ready_for_check(&self, pictures: &splice::JunctionDisplay) -> bool {
        self.can_apply(pictures)
    }

    pub(in crate::preview) fn state_for_check(&self, pictures: &splice::JunctionDisplay) -> Value {
        let intent = self.input.accepted;
        json!({
            "capture":self.capture.state_for_check(),
            "accepted":{"in":intent.in_frames,"out":intent.out_frames,"slip":intent.slip_frames,"roll":intent.roll_frames,"policy":format!("{:?}",intent.policy)},
            "control":format!("{:?}",self.control),"side":format!("{:?}",self.side),
            "waiting":self.input.waiting(),"feedback":self.input.feedback.iter().map(|event| json!({
                "event":format!("{:?}",event.event),"error":event.error,
                "adjustment":event.adjustment.as_ref().map(|adjustment|json!({"requested":adjustment.requested_value,"applied":adjustment.applied_value,"clamp":format!("{:?}",adjustment.clamp)})),
            })).collect::<Vec<_>>(),
            "amount":self.amount,"amount_dirty":self.amount_dirty,"text_error":self.input.text_error,
            "input_error":self.input.input_error,"proposal_error":self.input.error,"error":self.error,
            "apply_ready":self.can_apply(pictures),"applying":self.applying.is_some(),"invalidated":self.input.invalidated,
            "position":self.position.map(|sample|sample.0),"looping":self.looping,
            "inspection":self.inspection.as_ref().map(|inspection|json!({
                "identity":format!("{:?}",inspection.identity),"boundary":inspection.identity.boundary.0,
                "outgoing":inspection.identity.outgoing.map(|frame|frame.0),"incoming":inspection.identity.incoming.map(|frame|frame.0),
                "samples":[inspection.samples.start.0,inspection.samples.end.0],"boundary_sample":inspection.boundary_sample.0,
                "duration":inspection.duration.frames(),"incoming_label":inspection.incoming_label,"right_label":inspection.right_label,
                "pair_ready":pictures.ready_for_apply(&inspection.identity),
            })),
            "waveform":self.waveform.state_for_check(),
            "prepared_revision":self.input.ready().and_then(|prepared|prepared.snapshot.as_ref()).map(|snapshot|snapshot.document.revision_id()),
        })
    }
}
