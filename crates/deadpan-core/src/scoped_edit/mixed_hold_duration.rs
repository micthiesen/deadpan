//! Temporal isolation preserving literal mixed Default/Play authority.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, FrameDuration, HoldDurationVisibility,
    InsertionBias, ResolvedBoundary, RetimePurpose,
};

impl ProjectDocument {
    /// Exact identity pools for a mixed-scope Hold resize. This does not isolate
    /// or invent a concrete presentation for an unused definition.
    pub fn scoped_hold_duration_requirements(
        &self,
        target: &ScopedNodeTarget,
        duration: FrameDuration,
    ) -> Result<ScopedEditRequirements, EditError> {
        mixed_body_dormant(self, target)?;
        let NodeKind::Hold { recipe } = &self.nodes()[&target.node].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "a duration change applies to a pause (Hold)",
            ));
        };
        if duration == FrameDuration::ZERO {
            return Err(invalid("a pause needs positive duration"));
        }
        if recipe.duration == duration {
            return Err(invalid(
                "the pause already has that duration; no edit was made",
            ));
        }
        crate::sound_events::validate_command(
            self,
            &Command::SetScopedHoldDuration {
                target: target.clone(),
                duration,
                identities: OccurrenceIdentities::default(),
            },
        )?;
        temporal_requirements(self, target)
    }
}

/// Mixed body/gap liveness without inventing a concrete dormant occurrence.
pub(crate) fn mixed_body_dormant(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
) -> Result<bool, EditError> {
    if !target
        .repeats
        .iter()
        .any(|step| step.branch == RepeatEditBranch::Default)
        || !target
            .repeats
            .iter()
            .any(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
    {
        return Err(invalid(
            "mixed Hold duration requires both Default and explicit Play ancestry",
        ));
    }
    if let Some(fixed) = fixed_mixed_preflight(document, target)? {
        return Ok(fixed.dormant);
    }
    let mut parents = BTreeMap::new();
    for id in document.nodes().keys() {
        for child in document.children(id) {
            parents.insert(child.clone(), id.clone());
        }
    }
    super::duration_target_dormant(document, target, &parents)
}

pub(crate) struct FixedMixedDurationContext {
    pub dormant: bool,
}

/// Preserve literal Default authority while proving a body-only fixed window.
/// A live representative is used only to check geometry, never as the authoring
/// target. At most one more play than the number of stored overrides is visited.
pub(crate) fn fixed_mixed_preflight(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
) -> Result<Option<FixedMixedDurationContext>, EditError> {
    target.validate(document)?;
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut path = Vec::new();
    let mut current = &target.node;
    for _ in 0..=MAX_DOCUMENT_DEPTH {
        if current == document.root() {
            break;
        }
        let parent = parents
            .get(current)
            .copied()
            .ok_or_else(|| invalid("mixed Hold duration has no project ancestry"))?;
        path.push((parent, current));
        current = parent;
    }
    if current != document.root() {
        return Err(limit(
            "mixed Hold duration ancestry exceeds the depth bound",
        ));
    }
    if !path
        .iter()
        .any(|(parent, _)| matches!(document.nodes()[*parent].kind, NodeKind::Retime { .. }))
    {
        return Ok(None);
    }
    let clocked = document.retimes_with_derived_clocks();
    let mut remaining = target.repeats.len();
    let mut retimes = 0;
    let mut dormant = false;
    let mut concrete = Vec::new();
    for (parent, child) in path {
        match &document.nodes()[parent].kind {
            NodeKind::Sequence { .. } => {}
            NodeKind::Retime {
                purpose: RetimePurpose::Edit,
                ..
            } if remaining == target.repeats.len() && retimes == 0 && !clocked.contains(parent) => {
                retimes += 1;
            }
            NodeKind::Repeat {
                child: default,
                iterations,
                ..
            } if remaining > 0 && retimes == 1 => {
                remaining -= 1;
                let step = &target.repeats[remaining];
                if &step.repeat != parent {
                    return Err(invalid(
                        "mixed fixed duration differs from its Repeat ancestry",
                    ));
                }
                let owned = document.overrides().get(parent);
                let iteration = match &step.branch {
                    RepeatEditBranch::Default => {
                        if child != default {
                            return Err(invalid(
                                "mixed fixed Default must follow the literal body",
                            ));
                        }
                        let mut selected = None;
                        'segments: for (allocation, first, count) in iterations.segments() {
                            for offset in 0..count {
                                let ordinal = first
                                    .checked_add(offset)
                                    .ok_or_else(|| limit("Repeat identity overflows"))?;
                                let iteration = IterationId {
                                    allocation: allocation.clone(),
                                    ordinal,
                                };
                                if !owned.is_some_and(|entries| entries.get(&iteration).is_some()) {
                                    selected = Some(iteration);
                                    break 'segments;
                                }
                            }
                        }
                        dormant |= selected.is_none();
                        selected
                    }
                    RepeatEditBranch::Play { iteration } => {
                        let body = owned
                            .and_then(|entries| entries.get(iteration))
                            .unwrap_or(default);
                        if child != body {
                            return Err(invalid(
                                "mixed fixed duration requires a body, not an owned gap",
                            ));
                        }
                        Some(iteration.clone())
                    }
                };
                if let Some(iteration) = iteration {
                    concrete.push(crate::RepeatInstance {
                        node: parent.clone(),
                        iteration,
                    });
                }
            }
            _ => {
                return Err(invalid(
                    "mixed fixed duration requires one unclocked Edit Retime below all Repeats, otherwise Sequences",
                ));
            }
        }
    }
    if remaining != 0 || retimes != 1 || target.repeats.is_empty() {
        return Err(invalid(
            "mixed fixed duration needs one Edit Retime below its innermost Repeat",
        ));
    }
    if !dormant {
        concrete.reverse();
        let instance = InstancePath {
            node: target.node.clone(),
            repeats: concrete,
        };
        if !target.matches_instance(document, &instance)? {
            return Err(invalid("mixed fixed duration lost its live body"));
        }
        document.hold_duration_visibility_for_instance(&target.repeats[0].repeat, &instance)?;
    }
    Ok(Some(FixedMixedDurationContext { dormant }))
}

pub(crate) struct ResolvedDuration {
    pub document: ProjectDocument,
    pub command: Command,
    pub isolation: Vec<ScopedIsolationStep>,
    pub fixed_target: Option<(ScopedNodeTarget, bool)>,
}

pub(crate) fn prepare_command(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<Option<ResolvedDuration>, EditError> {
    let Command::SetScopedHoldDuration {
        target,
        duration,
        identities,
    } = &request.command
    else {
        return Ok(None);
    };
    let requirements = document.scoped_hold_duration_requirements(target, *duration)?;
    let fixed = fixed_mixed_preflight(document, target)?;
    if identities.nodes.len() != requirements.nodes || identities.marks.len() != requirements.marks
    {
        return Err(invalid(
            "mixed Hold duration requires its exact node and mark identity pools",
        ));
    }
    let (after, mapped, maps) = isolate_temporal(
        document,
        target,
        identities,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: None,
            sound_clocks: None,
        },
    )?;
    Ok(Some(ResolvedDuration {
        document: after,
        command: Command::SetHoldDuration {
            node: mapped.node.clone(),
            duration: *duration,
        },
        isolation: maps.into_iter().map(|mapping| mapping.proof()).collect(),
        fixed_target: fixed.map(|context| (mapped, context.dormant)),
    }))
}

/// The actual isolated resize, with revision-bound authoring and presentation
/// mappings. A missing presentation is accepted only for a proven dormant body.
#[derive(Debug)]
pub struct PreparedScopedHoldDuration {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
    /// Intrinsic occurrence start, absent for dormant definitions or when a
    /// visible fixed-window Hold's intrinsic start is outside its retained crop.
    pub boundary: Option<ResolvedBoundary>,
    isolation: ScopedIsolationRecord,
    fixed_visibility: Option<HoldDurationVisibility>,
}

impl PreparedScopedHoldDuration {
    /// Visible sampled continuation for the narrow fixed-window path. This
    /// never substitutes a live occurrence for dormant Default authority.
    pub fn fixed_visibility(&self) -> Option<HoldDurationVisibility> {
        self.fixed_visibility
    }
    pub fn isolation(&self) -> &ScopedIsolationRecord {
        &self.isolation
    }
    pub fn map_target(
        &self,
        before: &ProjectDocument,
        target: &ScopedNodeTarget,
    ) -> Result<ScopedNodeTarget, EditError> {
        self.isolation
            .bind_prepared(before, &self.document)?
            .map_forward(target)
    }
    pub fn map_instance(
        &self,
        before: &ProjectDocument,
        instance: &InstancePath,
    ) -> Result<InstancePath, EditError> {
        self.isolation
            .bind_prepared(before, &self.document)?
            .map_instance_forward(instance)
    }
}

pub fn prepare_scoped_hold_duration(
    document: &ProjectDocument,
    request: &CommandRequest,
    presentation: Option<&InstancePath>,
) -> Result<PreparedScopedHoldDuration, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::SetScopedHoldDuration {
        target, duration, ..
    } = &request.command
    else {
        return Err(invalid(
            "mixed Hold duration preparation requires SetScopedHoldDuration",
        ));
    };
    document.scoped_hold_duration_requirements(target, *duration)?;
    let fixed = fixed_mixed_preflight(document, target)?.is_some();
    let dormant = mixed_body_dormant(document, target)?;
    match (presentation, dormant) {
        (None, true) => {}
        (Some(instance), false) if target.matches_instance(document, instance)? => {}
        _ => {
            return Err(invalid(
                "mixed Hold duration requires its live presentation or a proven dormant definition",
            ));
        }
    }
    let before_duration = document.structural_duration()?;
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    let proof = isolation.bind_prepared(document, &after)?;
    let mapped = proof.map_forward(target)?;
    let presentation = presentation
        .map(|instance| proof.map_instance_forward(instance))
        .transpose()?;
    if mixed_body_dormant(&after, &mapped)? != dormant
        || (dormant && after.structural_duration()? != before_duration)
    {
        return Err(invalid(
            "mixed Hold duration changed dormant definition visibility",
        ));
    }
    let fixed_visibility = if fixed {
        presentation
            .as_ref()
            .map(|instance| {
                after.hold_duration_visibility_for_instance(&mapped.repeats[0].repeat, instance)
            })
            .transpose()?
    } else {
        None
    };
    let boundary = presentation
        .as_ref()
        .map(|instance| {
            if !mapped.matches_instance(&after, instance)? {
                return Err(invalid("resized Hold lost its concrete presentation"));
            }
            let resolved = crate::derived_clock::with_sealed(|| {
                AnchorIndex::new(&after).and_then(|index| {
                    index.resolve_target(&AnchorTarget {
                        boundary: BoundaryAnchor {
                            coordinate: Anchor::Occurrence {
                                instance: instance.clone(),
                                position: crate::ExactRatio::ZERO,
                            },
                            bias: InsertionBias::Right,
                        },
                        occurrence: None,
                    })
                })
            });
            match resolved {
                Ok(boundary) => Ok(Some(boundary)),
                Err(error)
                    if fixed_visibility.is_some()
                        && error.code == crate::AnchorErrorCode::OutsideMapping =>
                {
                    Ok(None)
                }
                Err(error) => Err(invalid(&error.to_string())),
            }
        })
        .transpose()?
        .flatten();
    Ok(PreparedScopedHoldDuration {
        transaction,
        document: after,
        target: mapped,
        presentation,
        boundary,
        isolation,
        fixed_visibility,
    })
}
