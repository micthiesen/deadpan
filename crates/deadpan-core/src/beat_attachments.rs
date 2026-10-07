//! Keep the complete owned attachment set on one Repeat play. This moves
//! logical marks, including every fragment, rather than assigning new names.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Anchor, EditError, EditErrorCode, MAX_DOCUMENT_NODES, NodeId, NodeKind, OccurrenceIdentities,
    ProjectDocument, RepeatInstance,
};

impl ProjectDocument {
    /// Exact fresh node count for `KeepFirstPlayAttachments`. Zero means the
    /// one-play Repeat or its attachment-free body needs no isolation. No new
    /// mark identities are required because attachment ownership moves.
    pub fn first_play_attachment_nodes(&self, repeat: &NodeId) -> Result<usize, EditError> {
        let (child, first, plays) = target(self, repeat)?;
        if self
            .overrides()
            .get(repeat)
            .is_some_and(|entries| entries.get(&first).is_some())
        {
            return Err(invalid(
                "first-play attachment isolation requires the shared first play",
            ));
        }
        let nodes = crate::occurrence_edit::subtree_order(self, &child)?;
        let owned: BTreeSet<_> = nodes.iter().collect();
        let attached = nodes.iter().any(|id| {
            let node = &self.nodes()[id];
            !node.captions.is_empty()
                || !node.cutaways.is_empty()
                || self.beat_sounds().contains_key(id)
        }) || self.marks().values().any(|mark| {
            mark.bindings().any(|binding| {
                owned.contains(&binding.owner)
                    && matches!(
                        binding.coordinate,
                        Anchor::Local { .. } | Anchor::Source { .. }
                    )
            })
        });
        if plays == 1 || !attached {
            return Ok(0);
        }
        if self
            .nodes()
            .len()
            .checked_add(nodes.len())
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(EditError::new(
                EditErrorCode::LimitExceeded,
                "first-play attachments exceed the document node limit",
            ));
        }
        Ok(nodes.len())
    }
}

fn target(
    document: &ProjectDocument,
    repeat: &NodeId,
) -> Result<(NodeId, crate::IterationId, u32), EditError> {
    let Some(crate::BeatNode {
        kind: NodeKind::Repeat {
            child, iterations, ..
        },
        ..
    }) = document.nodes().get(repeat)
    else {
        return Err(invalid("first-play attachments require a Repeat"));
    };
    let parent = document
        .parent_of(repeat)
        .ok_or_else(|| invalid("Repeat needs an ordinary Sequence parent"))?;
    // The grammar wraps one direct child in an ordinary Sequence scope. Keep
    // that same admission for callers of the resolved command.
    document.sequence_children(&parent, repeat, repeat)?;
    Ok((
        child.clone(),
        iterations.at(0).expect("validated nonempty Repeat"),
        iterations.len(),
    ))
}

pub(crate) fn apply(
    document: &ProjectDocument,
    repeat: &NodeId,
    identities: &OccurrenceIdentities,
    mut context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let required = document.first_play_attachment_nodes(repeat)?;
    if required == 0 || identities.nodes.len() != required || !identities.marks.is_empty() {
        return Err(invalid(
            "first-play attachments require their exact nonempty node pool and no mark identities",
        ));
    }
    let mut occupied: BTreeSet<_> = document.nodes().keys().collect();
    occupied.extend(
        document
            .audio_lineage()
            .values()
            .map(|lineage| &lineage.origin),
    );
    for layout in document.audio_bindings().timings.values() {
        occupied.extend(layout.nodes().keys());
        occupied.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| &lineage.origin),
        );
    }
    if identities.nodes.iter().any(|id| !occupied.insert(id)) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "first-play attachment node identities must be fresh and distinct, including retained audio names",
        ));
    }
    let (child, iteration, _) = target(document, repeat)?;
    let mapping: BTreeMap<_, _> = crate::occurrence_edit::subtree_order(document, &child)?
        .into_iter()
        .zip(identities.nodes.iter().cloned())
        .collect();
    let selected = RepeatInstance {
        node: repeat.clone(),
        iteration: iteration.clone(),
    };
    let mut result = document.clone();
    result.marks = crate::marks::move_occurrence_marks(document, &selected, &mapping)?;
    crate::occurrence_edit::clone_nodes(&mut result, &mapping, context.allocation)?;
    if let Some(allowances) = context.allowances.as_deref_mut() {
        allowances.isolate(std::slice::from_ref(&selected), &mapping)?;
    }
    result
        .overrides
        .entry(repeat.clone())
        .or_default()
        .insert(iteration, mapping[&child].clone());
    for original in mapping.keys() {
        let node = result
            .nodes
            .get_mut(original)
            .expect("admitted original subtree");
        node.captions.clear();
        node.cutaways.clear();
        result.beat_sounds.remove(original);
        result.audio_bindings.sound_clocks.remove(original);
    }
    // Cloning inherits the exact physical audio lattice. Removing temporal
    // picture attachments does not change that lattice or creative effects.
    crate::compound::wire::size(&result, crate::MAX_DOCUMENT_JSON_BYTES)?;
    Ok(result)
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
