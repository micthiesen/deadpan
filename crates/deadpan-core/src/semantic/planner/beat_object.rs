//! `rib` keeps owned temporal attachments on the first play in one typed leaf.
//! The core command moves complete logical marks and all nested hosts together.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    /// Resolution still requires a complete live beat; sound admission and
    /// retained processing clocks are checked by the atomic Repeat command.
    pub(super) fn check_first_play_attachments(&self, beat: &NodeId) -> Result<(), EditError> {
        crate::occurrence_edit::subtree_order(&self.current, beat)?;
        Ok(())
    }

    pub(super) fn keep_attachments_on_first_play(
        &mut self,
        repeat: &NodeId,
    ) -> Result<(), EditError> {
        let nodes = self.current.first_play_attachment_nodes(repeat)?;
        if nodes == 0 {
            return Ok(());
        }
        self.charge_step(false)?;
        let SemanticAllocation::Isolation {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Isolation {
            step_index: self.steps.len(),
            nodes,
            marks: 0,
        })?
        else {
            return Err(invalid("rib requires an Isolation allocation"));
        };
        if identities.nodes.len() != nodes || !identities.marks.is_empty() {
            return Err(invalid("rib requires its exact isolation identities"));
        }
        self.reserve_revision(&new_revision)?;
        for node in &identities.nodes {
            self.reserve_node(node)?;
        }
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::KeepFirstPlayAttachments {
                node: repeat.clone(),
                identities,
            },
        )?)
    }
}
