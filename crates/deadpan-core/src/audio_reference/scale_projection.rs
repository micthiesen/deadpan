use super::{FrozenAudioKind, FrozenAudioLayout, invalid, limit, spend};
use crate::{DocumentError, ExactRatio, MAX_DOCUMENT_NODES, NodeId};

impl FrozenAudioLayout {
    /// Exact physical-local-frame scale inside one explicit node-output scope.
    /// Repeat play and gap offsets are translations, so this deliberately does
    /// not select or enumerate any Repeat occurrence.
    pub(crate) fn scale_in_scope(
        &self,
        root: &NodeId,
        physical: &NodeId,
        maximum_work: usize,
    ) -> Result<(ExactRatio, usize), DocumentError> {
        if maximum_work == 0 || maximum_work > MAX_DOCUMENT_NODES {
            return Err(limit("invalid frozen scale-scope budget"));
        }
        if !self.projects(root) || !self.projects(physical) {
            return Err(invalid(
                "frozen scale-scope alias is missing or not projected",
            ));
        }

        let mut work = 0usize;
        let mut scale = ExactRatio::ONE;
        let mut node = physical;
        loop {
            spend(&mut work, 1, maximum_work)?;
            if node == root {
                return Ok((scale, work));
            }
            let (parent, _) = self
                .index
                .parents
                .get(node)
                .ok_or_else(|| invalid("frozen scale target is outside its explicit scope"))?;

            // Match project_scoped_inner: apply a Retime scope root's factor
            // before reaching that root. Sequence and Repeat only translate.
            let parent_node = &self.nodes[parent];
            match &parent_node.kind {
                FrozenAudioKind::Sequence { .. } | FrozenAudioKind::Repeat { .. } => {}
                FrozenAudioKind::Retime { .. } => {
                    if self.retime_processes(parent, parent_node) {
                        return Err(invalid("frozen scale scope crosses opaque processing"));
                    }
                    let clock = self.retime_clock(parent)?;
                    let factor = clock
                        .extent
                        .checked_div(clock.child_end.checked_sub(clock.child_start)?)?;
                    scale = scale.checked_mul(factor)?;
                }
                FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => {
                    return Err(invalid("frozen scale projection has a leaf parent"));
                }
            }
            node = parent;
        }
    }
}
