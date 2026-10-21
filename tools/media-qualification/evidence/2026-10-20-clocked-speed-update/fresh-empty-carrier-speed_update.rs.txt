//! Audited parameter updates of an affine stage owning exact pause clocks.
use std::collections::BTreeSet;

use crate::{
    AudioBindingEnvironment, AudioTimingId, DocumentError, EditError, EditErrorCode, ExactRatio,
    FrameDuration, FrozenAudioLayout, HoldAudio, InstancePath, MAX_DOCUMENT_DEPTH,
    MAX_DOCUMENT_NODES, MIX_SAMPLE_RATE, NodeId, NodeKind, PitchPolicy, ProjectDocument,
    ProjectFrame, RetimePurpose, RevisionId,
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

/// Current physical owners and ancestry of the bounded affine family. Each
/// owner has one concrete path: no Repeat representatives are sampled.
fn family(
    document: &ProjectDocument,
    node: &NodeId,
    pitch: PitchPolicy,
) -> Result<BTreeSet<NodeId>, EditError> {
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
    let mut pending = vec![node];
    let mut owners = BTreeSet::new();
    let mut visited = 0usize;
    while let Some(id) = pending.pop() {
        visited += 1;
        if visited > MAX_DOCUMENT_NODES {
            return Err(super::super::limit("exact speed update family"));
        }
        match &document.nodes()[id].kind {
            NodeKind::Repeat { .. } => {
                return Err(invalid(
                    "updating a stage with repeated exact pause contents is not available yet",
                ));
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
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {
                owners.insert(id.clone());
            }
            NodeKind::Sequence { .. } => {}
        }
        pending.extend(document.children(id));
    }
    Ok(owners)
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

    /// Preflight the actual retained reference transfer, including older
    /// lattices surviving successive updates. This never changes the document.
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
        let owners = family(self, node, requested_pitch)?;
        let frozen = FrozenAudioLayout::capture(self)?;
        let clock = super::clock_of(self, node)?;
        let factor = ExactRatio::integer(duration.frames())
            .checked_div(clock.extent)
            .map_err(DocumentError::from)?;
        let rate = self.presentation_basis().frame_rate;
        let frame_per_sample = ExactRatio::new(
            i128::from(rate.numerator()),
            i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
        )
        .map_err(DocumentError::from)?;
        let stage = frozen.project(
            &InstancePath {
                node: node.clone(),
                repeats: vec![],
            },
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )?;
        let mut work = stage.work;
        for owner in owners {
            if !audible_owner(self, &owner) {
                continue;
            }
            let path = InstancePath {
                node: owner.clone(),
                repeats: vec![],
            };
            let maximum = MAX_DOCUMENT_NODES
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
            let (current, allocation) =
                frozen.project_scoped_with_allocation(frozen.root(), &path, None, maximum)?;
            work = work
                .checked_add(current.work)
                .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
            let Some(allocation) = allocation else {
                continue;
            };
            // A positive mathematical interval can receive no mix samples
            // after this update. Such a carrier never invokes the transfer
            // resampler, so its unused historical rate cannot cap the edit.
            // Sequence ancestors preserve the stage origin and root rounding.
            let candidate_origin = current
                .origin
                .checked_sub(stage.origin)
                .and_then(|offset| offset.checked_mul(factor))
                .and_then(|offset| stage.origin.checked_add(offset))
                .map_err(DocumentError::from)?;
            let candidate_scale = current
                .frames_per_local_frame
                .checked_mul(factor)
                .map_err(DocumentError::from)?;
            let sample_boundary = |local: ExactRatio| {
                local
                    .checked_mul(candidate_scale)
                    .and_then(|offset| candidate_origin.checked_add(offset))
                    .and_then(|root| root.checked_div(frame_per_sample))
                    .and_then(ExactRatio::round_even)
                    .map_err(DocumentError::from)
            };
            if sample_boundary(allocation.start)? == sample_boundary(allocation.end)? {
                continue;
            }
            // The raw carrier uses current meaningful support, not the
            // narrower allocation of a transparent Partition. An empty
            // historical sample domain returns silence before transfer.
            let maximum = MAX_DOCUMENT_NODES
                .checked_sub(work)
                .filter(|left| *left > 0)
                .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
            let (raw, support) =
                frozen.project_scoped_with_support(frozen.root(), &path, None, maximum)?;
            work = work
                .checked_add(raw.work)
                .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
            let historical_unit = if self.audio_bindings().bindings().contains_key(&owner) {
                let maximum = MAX_DOCUMENT_NODES
                    .checked_sub(work)
                    .filter(|left| *left > 0)
                    .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
                let binding = self.audio_bindings().resolve_in(
                    &owner,
                    AudioBindingEnvironment::Occurrence(&path),
                    maximum,
                )?;
                work = work
                    .checked_add(binding.work)
                    .ok_or_else(|| super::super::limit("exact speed update transfer work"))?;
                if binding
                    .lattice
                    .sample_boundary(support.start)
                    .map_err(DocumentError::from)?
                    == binding
                        .lattice
                        .sample_boundary(support.end)
                        .map_err(DocumentError::from)?
                {
                    continue;
                }
                binding
                    .lattice
                    .local_frames_per_sample()
                    .map_err(DocumentError::from)?
            } else {
                // Commit captures every missing carrier on this PRE-edit map,
                // including its root-round-even raw allocation.
                let old_boundary = |local: ExactRatio| {
                    local
                        .checked_mul(raw.frames_per_local_frame)
                        .and_then(|offset| raw.origin.checked_add(offset))
                        .and_then(|root| root.checked_div(frame_per_sample))
                        .and_then(ExactRatio::round_even)
                        .map_err(DocumentError::from)
                };
                if old_boundary(support.start)? == old_boundary(support.end)? {
                    continue;
                }
                frame_per_sample
                    .checked_div(current.frames_per_local_frame)
                    .map_err(DocumentError::from)?
            };
            let transfer = frame_per_sample
                .checked_div(current.frames_per_local_frame)
                .and_then(|unit| unit.checked_div(factor))
                .and_then(|unit| unit.checked_div(historical_unit))
                .map_err(DocumentError::from)?;
            if transfer.compare_integer(64).is_gt()
                || transfer
                    .checked_mul(ExactRatio::integer(64))
                    .map_err(DocumentError::from)?
                    .compare_integer(1)
                    .is_lt()
            {
                return Err(invalid(
                    "the retained audio transfer would leave the supported 1/64..64 range; choose a closer speed",
                ));
            }
        }
        Ok(())
    }
}

fn audible_owner(document: &ProjectDocument, owner: &NodeId) -> bool {
    match &document.nodes()[owner].kind {
        NodeKind::Hold { recipe } => recipe.audio != HoldAudio::Silence,
        NodeKind::Repeat { gap, .. } => gap
            .as_ref()
            .is_some_and(|recipe| recipe.audio != HoldAudio::Silence),
        NodeKind::Source { .. } | NodeKind::Retime { .. } => true,
        NodeKind::Sequence { .. } => false,
    }
}

pub(crate) fn set_retime(
    document: &ProjectDocument,
    node: &NodeId,
    duration: FrameDuration,
    pitch: PitchPolicy,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    document.validate_retime_update(node, duration, pitch)?;
    let (old_duration, old_pitch) = ordinary(document, node)?;
    if old_duration == duration && old_pitch == pitch {
        document.validate()?;
        super::EXACT_EDIT.with(|flag| flag.set(true));
        return Ok(document.clone());
    }
    super::supported(document)?;
    let inner = family(document, node, pitch)?;
    let mut affected = super::super::composite::occurrence_owners_after(document, node)?;
    affected.extend(
        inner
            .iter()
            .filter(|owner| !document.audio_bindings().bindings().contains_key(*owner))
            .cloned(),
    );
    affected.retain(|owner| audible_owner(document, owner));
    let total =
        crate::derived_clock::integral(document.exact_durations()?[document.root()])?.frames();
    let timing = AudioTimingId {
        allocation: allocation.clone(),
        ordinal: 0,
    };
    let mut result = crate::command_work::with_scoped_timing_captures(false, || {
        super::super::composite::prepare_owners(
            document,
            &affected,
            ProjectFrame(0),
            total,
            &timing,
        )
    })?;
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
