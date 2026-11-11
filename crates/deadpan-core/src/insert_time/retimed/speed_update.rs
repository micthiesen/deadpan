//! Audited parameter updates of an affine stage owning exact pause clocks.

use crate::{
    DocumentError, EditError, EditErrorCode, ExactRatio, FrameDuration, MAX_DOCUMENT_DEPTH,
    MAX_DOCUMENT_NODES, NodeId, NodeKind, PitchPolicy, ProjectDocument, RetimePurpose,
};

pub(crate) fn clocked_retime_target(document: &ProjectDocument, node: &NodeId) -> bool {
    !document.derived_clocks().is_empty()
        && (document.retimes_with_derived_clocks().contains(node)
            || document.sealed_nodes().contains(node))
}

fn ordinary(
    document: &ProjectDocument,
    node: &NodeId,
) -> Result<(FrameDuration, PitchPolicy), EditError> {
    match document.nodes().get(node).map(|beat| &beat.kind) {
        Some(NodeKind::Retime {
            duration,
            pitch,
            purpose: RetimePurpose::Edit,
            ..
        }) => Ok((*duration, *pitch)),
        Some(_) => Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "set-retime updates an ordinary Retime; wrap a transparent partition to preserve its retained context",
        )),
        None => Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "the Retime no longer exists",
        )),
    }
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

/// Admit the bounded affine family below `node`: tape pitch, no processing
/// stage and at most one Repeat level.
fn family(document: &ProjectDocument, node: &NodeId, pitch: PitchPolicy) -> Result<(), EditError> {
    let (_, current_pitch) = ordinary(document, node)?;
    if current_pitch != PitchPolicy::FollowSpeed || pitch != PitchPolicy::FollowSpeed {
        return Err(invalid(
            "updating a stage with exact pause clocks currently requires tape pitch",
        ));
    }
    let clock = super::clock_of(document, node)?;
    if clock.extent.denominator() != 1 {
        return Err(invalid(
            "a fractional stage extent cannot change speed directly yet; change its outer stage",
        ));
    }
    let mut current = node;
    for depth in 0..=MAX_DOCUMENT_DEPTH {
        let Some(parent) = document.parent_of(current) else {
            break;
        };
        if !matches!(document.nodes()[&parent].kind, NodeKind::Sequence { .. }) {
            return Err(invalid(
                "updating exact pause clocks requires ordinary Sequence ancestors",
            ));
        }
        current = document
            .nodes()
            .get_key_value(&parent)
            .expect("parent exists")
            .0;
        if depth == MAX_DOCUMENT_DEPTH {
            return Err(super::super::limit("exact speed update ancestry"));
        }
    }
    let mut pending = vec![(node, false)];
    let mut visited = 0usize;
    while let Some((id, repeated)) = pending.pop() {
        visited += 1;
        if visited > MAX_DOCUMENT_NODES {
            return Err(super::super::limit("exact speed update family"));
        }
        match &document.nodes()[id].kind {
            NodeKind::Repeat { child, .. } => {
                if repeated {
                    return Err(invalid(
                        "updating exact pause clocks cannot yet change nested Repeat families",
                    ));
                }
                pending.push((child, true));
                for overrides in [document.overrides(), document.gap_overrides()] {
                    if let Some(overrides) = overrides.get(id) {
                        pending.extend(overrides.iter().map(|(_, root)| (root, true)));
                    }
                }
                continue;
            }
            NodeKind::Retime {
                duration,
                mapping,
                pitch,
                ..
            } => {
                let clock = super::clock_of(document, id)?;
                let unity = clock
                    .child_end
                    .checked_sub(clock.child_start)
                    .map_err(DocumentError::from)?
                    == clock.extent;
                if pitch.processes(mapping.duration() == *duration)
                    || pitch.processes(unity)
                    || clock.preserve_input.is_some()
                {
                    return Err(invalid(
                        "updating exact pause clocks cannot yet change a processing family",
                    ));
                }
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } | NodeKind::Sequence { .. } => {}
        }
        pending.extend(document.children(id).map(|child| (child, repeated)));
    }
    Ok(())
}

impl ProjectDocument {
    /// Exact selected child span for an ordinary parameter update. A clocked
    /// family is admitted only for the requested affine tape-pitch policy.
    pub fn retime_update_input(
        &self,
        node: &NodeId,
        requested_pitch: PitchPolicy,
    ) -> Result<ExactRatio, EditError> {
        ordinary(self, node)?;
        if clocked_retime_target(self, node) {
            family(self, node, requested_pitch)?;
        }
        let clock = super::clock_of(self, node)?;
        clock
            .child_end
            .checked_sub(clock.child_start)
            .map_err(DocumentError::from)
            .map_err(EditError::from)
    }

    /// Preflight a parameter update. This never changes the document.
    /// Identical raw Edit parameters retain their established no-op admission.
    pub fn validate_retime_update(
        &self,
        node: &NodeId,
        duration: FrameDuration,
        requested_pitch: PitchPolicy,
    ) -> Result<(), EditError> {
        let (old_duration, old_pitch) = ordinary(self, node)?;
        if old_duration == duration && old_pitch == requested_pitch {
            return Ok(());
        }
        if duration == FrameDuration::ZERO {
            return Err(EditError::new(
                EditErrorCode::InvalidDuration,
                "a Retime needs a positive output duration",
            ));
        }
        if !clocked_retime_target(self, node) {
            let NodeKind::Retime { mapping, .. } = &self.nodes()[node].kind else {
                unreachable!()
            };
            return crate::command::check_preserve_bound(
                self,
                mapping.duration(),
                duration,
                requested_pitch,
            );
        }
        // A clocked tape-pitch family resamples like any other tape speed
        // change; only its structure needs admission.
        family(self, node, requested_pitch)?;
        Ok(())
    }
}

pub(crate) fn set_retime(
    document: &ProjectDocument,
    node: &NodeId,
    duration: FrameDuration,
    pitch: PitchPolicy,
) -> Result<ProjectDocument, EditError> {
    document.validate_retime_update(node, duration, pitch)?;
    let (old_duration, old_pitch) = ordinary(document, node)?;
    if old_duration == duration && old_pitch == pitch {
        document.validate()?;
        super::EXACT_EDIT.with(|flag| flag.set(true));
        return Ok(document.clone());
    }
    super::supported(document)?;
    family(document, node, pitch)?;
    // Refuse a change whose suffix would cross a Retime ancestor.
    super::super::composite::occurrence_owners_after(document, node)?;
    let mut result = document.clone();
    let clock = super::clock_of(document, node)?;
    super::set_clock(
        &mut result,
        node,
        ExactRatio::integer(duration.frames()),
        clock.child_start,
        clock.child_end,
    )?;
    crate::audio_lineage::reconcile_exact(document, &mut result, std::slice::from_ref(node));
    result.validate()?;
    super::EXACT_EDIT.with(|flag| flag.set(true));
    Ok(result)
}
