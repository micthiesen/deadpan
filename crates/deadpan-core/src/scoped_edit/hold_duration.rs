//! One concrete Hold resize and its exact revision-bound continuation.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, FrameDuration, HoldDurationVisibility,
    InsertionBias, OccurrenceEdit, ResolvedBoundary, RetimePurpose,
};

impl ProjectDocument {
    /// Exact outside-in isolation needs for one visible Repeat occurrence's
    /// Hold resize. Unsupported temporal clocks fail before allocating names.
    pub fn occurrence_hold_duration_requirements(
        &self,
        instance: &InstancePath,
        duration: FrameDuration,
    ) -> Result<ScopedEditRequirements, EditError> {
        let target = target(instance);
        if target.repeats.is_empty() {
            return Err(invalid(
                "a one-play Hold duration needs explicit Repeat ancestry",
            ));
        }
        if !target.matches_instance(self, instance)? {
            return Err(invalid(
                "Hold duration presentation differs from its target",
            ));
        }
        let NodeKind::Hold { recipe } = &self.nodes()[&instance.node].kind else {
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
        fixed_occurrence_preflight(self, instance)?;
        // An occurrence wrapper does not yet transport duration-specific root
        // or attached sound intervals. Keep that existing refusal explicit.
        crate::sound_events::validate_command(
            self,
            &Command::EditOccurrence {
                instance: instance.clone(),
                edit: OccurrenceEdit::SetHoldDuration { duration },
                identities: OccurrenceIdentities::default(),
            },
        )?;
        isolation::temporal_requirements(self, &target)
    }

    /// Duration-only authority for one visible body Hold through explicit Plays,
    /// with one fixed unclocked Edit Retime below the innermost Repeat.
    /// This grants no authority for gaps, mixed choices or multiple targets.
    pub fn fixed_occurrence_hold_duration_visibility(
        &self,
        parent: &NodeId,
        root: &NodeId,
        instance: &InstancePath,
    ) -> Result<HoldDurationVisibility, EditError> {
        self.validate()?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(invalid(
                "fixed one-play Hold duration needs a Sequence parent",
            ));
        };
        if !children.contains(root) {
            return Err(invalid(
                "fixed one-play Hold duration root must be a direct child of its parent",
            ));
        }
        let eligible = fixed_occurrence_preflight(self, instance)?
            .ok_or_else(|| invalid("fixed one-play Hold duration needs one Edit Retime"))?;
        if &eligible.root != root {
            return Err(invalid(
                "fixed one-play Hold duration root differs from its explicit Repeat",
            ));
        }
        validate_duration_sounds(self, instance)?;
        Ok(eligible.visibility)
    }
}

pub(crate) struct FixedOccurrenceDurationContext {
    pub root: NodeId,
    pub visibility: HoldDurationVisibility,
}

/// None preserves the existing ordinary Sequence/Repeat duration path. Any
/// Retime ancestry must satisfy this narrower policy before isolation.
pub(crate) fn fixed_occurrence_preflight(
    document: &ProjectDocument,
    instance: &InstancePath,
) -> Result<Option<FixedOccurrenceDurationContext>, EditError> {
    instance.validate(document)?;
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut path = Vec::new();
    let mut current = &instance.node;
    for _ in 0..=MAX_DOCUMENT_DEPTH {
        if current == document.root() {
            break;
        }
        let parent = parents
            .get(current)
            .copied()
            .ok_or_else(|| invalid("Hold duration target has no project ancestry"))?;
        path.push((parent, current));
        current = parent;
    }
    if current != document.root() {
        return Err(limit("Hold duration ancestry exceeds the depth bound"));
    }
    if !path
        .iter()
        .any(|(id, _)| matches!(document.nodes()[*id].kind, NodeKind::Retime { .. }))
    {
        return Ok(None);
    }
    let Some(repeat) = instance.repeats.first() else {
        return Err(invalid(
            "fixed one-play Hold duration needs explicit Repeat ancestry",
        ));
    };
    let clocked = document.retimes_with_derived_clocks();
    let mut remaining = instance.repeats.len();
    let mut retimes = 0usize;
    for (parent, child) in path {
        match &document.nodes()[parent].kind {
            NodeKind::Sequence { .. } => {}
            NodeKind::Retime {
                purpose: RetimePurpose::Edit,
                ..
            } if remaining == instance.repeats.len()
                && retimes == 0
                && !clocked.contains(parent) =>
            {
                retimes += 1;
            }
            NodeKind::Repeat { child: default, .. } if remaining > 0 && retimes == 1 => {
                remaining -= 1;
                let step = &instance.repeats[remaining];
                if parent != &step.node {
                    return Err(invalid(
                        "fixed one-play Hold duration differs from its Repeat ancestry",
                    ));
                }
                let effective = document
                    .overrides()
                    .get(parent)
                    .and_then(|entries| entries.get(&step.iteration))
                    .unwrap_or(default);
                if child != effective {
                    return Err(invalid(
                        "fixed one-play Hold duration requires a body, not an owned gap",
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "fixed one-play Hold duration requires one fixed unclocked Edit Retime below all explicit Repeats, otherwise Sequences",
                ));
            }
        }
    }
    if remaining != 0 || retimes != 1 {
        return Err(invalid(
            "fixed one-play Hold duration needs one Edit Retime below its innermost Repeat",
        ));
    }
    let visibility = document.hold_duration_visibility_for_instance(&repeat.node, instance)?;
    Ok(Some(FixedOccurrenceDurationContext {
        root: repeat.node.clone(),
        visibility,
    }))
}

fn validate_duration_sounds(
    document: &ProjectDocument,
    instance: &InstancePath,
) -> Result<(), EditError> {
    let Some(NodeKind::Hold { recipe }) =
        document.nodes().get(&instance.node).map(|node| &node.kind)
    else {
        return Err(invalid("fixed one-play duration requires a Hold"));
    };
    crate::sound_events::validate_command(
        document,
        &Command::EditOccurrence {
            instance: instance.clone(),
            edit: OccurrenceEdit::SetHoldDuration {
                duration: recipe.duration,
            },
            identities: OccurrenceIdentities::default(),
        },
    )
}

pub(crate) struct ResolvedFixedDuration {
    pub document: ProjectDocument,
    pub command: Command,
    pub isolation: Vec<ScopedIsolationStep>,
    pub root: NodeId,
    pub presentation: InstancePath,
}

/// Isolate before the ordinary duration reducer captures physical allocations.
/// The original wrapper has already passed its sound and context validators.
pub(crate) fn prepare_fixed_command(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<Option<ResolvedFixedDuration>, EditError> {
    let Command::EditOccurrence {
        instance,
        edit: OccurrenceEdit::SetHoldDuration { duration },
        identities,
    } = &request.command
    else {
        return Ok(None);
    };
    let Some(eligible) = fixed_occurrence_preflight(document, instance)? else {
        return Ok(None);
    };
    let requirements = document.occurrence_hold_duration_requirements(instance, *duration)?;
    if identities.nodes.len() != requirements.nodes || identities.marks.len() != requirements.marks
    {
        return Err(invalid(
            "one-play Hold duration requires its exact node and mark identity pools",
        ));
    }
    let (after, mapped, mappings) = isolation::isolate_temporal(
        document,
        &target(instance),
        identities,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: None,
            sound_clocks: None,
        },
    )?;
    let steps: Vec<_> = mappings
        .into_iter()
        .map(|mapping| mapping.proof())
        .collect();
    let presentation = map_instance(instance, &steps);
    if !mapped.matches_instance(&after, &presentation)? {
        return Err(invalid("fixed duration isolation lost its presentation"));
    }
    Ok(Some(ResolvedFixedDuration {
        document: after,
        command: Command::SetHoldDuration {
            node: mapped.node,
            duration: *duration,
        },
        isolation: steps,
        root: eligible.root,
        presentation,
    }))
}

fn target(instance: &InstancePath) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .map(|step| RepeatEditStep {
                repeat: step.node.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect(),
    }
}

/// Validate an ordinary duration path against one shared parent index. Owned
/// gaps follow their preceding stable Play, including a dormant final branch.
/// A valid address alone does not prove that it has a project occurrence.
pub(crate) fn duration_target_dormant(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    parents: &BTreeMap<NodeId, NodeId>,
) -> Result<bool, EditError> {
    if target.repeats.is_empty() || target.repeats.len() > MAX_DOCUMENT_DEPTH {
        return Err(invalid(
            "Hold duration requires bounded explicit Repeat ancestry",
        ));
    }
    if !document.nodes().contains_key(&target.node) {
        return Err(invalid("Hold duration target is absent"));
    }
    let mut current = &target.node;
    let mut remaining = target.repeats.len();
    let mut dormant = false;
    for _ in 0..=MAX_DOCUMENT_DEPTH {
        if current == document.root() {
            return if remaining == 0 {
                Ok(dormant)
            } else {
                Err(invalid(
                    "Hold duration target contains extra Repeat ancestry",
                ))
            };
        }
        let parent = parents
            .get(current)
            .ok_or_else(|| invalid("Hold duration target has no project ancestry"))?;
        match &document.nodes()[parent].kind {
            NodeKind::Sequence { .. } => {}
            NodeKind::Repeat {
                child, iterations, ..
            } => {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Hold duration target omits a Repeat ancestor"))?;
                let step = &target.repeats[remaining];
                if &step.repeat != parent {
                    return Err(invalid(
                        "Hold duration target names the wrong Repeat ancestor",
                    ));
                }
                match &step.branch {
                    RepeatEditBranch::Default => {
                        if current != child {
                            return Err(invalid("Default must follow the Repeat's literal body"));
                        }
                        let owned = document
                            .overrides()
                            .get(parent)
                            .map_or(0, |entries| entries.len());
                        dormant |= u64::try_from(owned)
                            .is_ok_and(|count| count == u64::from(iterations.len()));
                    }
                    RepeatEditBranch::Play { iteration } => {
                        let position = iterations
                            .position(iteration)
                            .ok_or_else(|| invalid("Hold duration target names a retired Play"))?;
                        let body = document
                            .overrides()
                            .get(parent)
                            .and_then(|entries| entries.get(iteration))
                            .unwrap_or(child);
                        let gap = document
                            .gap_overrides()
                            .get(parent)
                            .and_then(|entries| entries.get(iteration));
                        if gap == Some(current) {
                            dormant |= position + 1 == iterations.len();
                        } else if current != body {
                            return Err(invalid(
                                "Hold duration target is outside its selected Play's owned branches",
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(invalid(
                    "Hold duration requires ordinary Sequence/Repeat ancestry without Retime or Partition",
                ));
            }
        }
        current = parent;
    }
    Err(limit("Hold duration ancestry exceeds the depth bound"))
}

#[derive(Debug)]
pub struct PreparedOccurrenceHoldDuration {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    pub presentation: InstancePath,
    /// Intrinsic start, when that anchor belongs to every retained input crop.
    /// None is permitted only for a visible fixed-window Hold whose intrinsic
    /// start is cropped away; fixed_visibility supplies its continuation.
    pub boundary: Option<ResolvedBoundary>,
    isolation: ScopedIsolationRecord,
    fixed_visibility: Option<HoldDurationVisibility>,
}

impl PreparedOccurrenceHoldDuration {
    pub fn isolation(&self) -> &ScopedIsolationRecord {
        &self.isolation
    }

    /// Visible sampled continuation for the narrow fixed-window path. The
    /// public boundary continues to describe the intrinsic occurrence start.
    pub fn fixed_visibility(&self) -> Option<HoldDurationVisibility> {
        self.fixed_visibility
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
        self.isolation.bind_prepared(before, &self.document)?;
        instance.validate(before)?;
        let result = map_instance(instance, self.isolation.steps());
        result.validate(&self.document)?;
        Ok(result)
    }
}

/// Execute precisely one EditOccurrence::SetHoldDuration and retain its actual
/// isolation plus the resized Hold's exact final start, without sampling time.
pub fn prepare_occurrence_hold_duration(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<PreparedOccurrenceHoldDuration, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::EditOccurrence {
        instance,
        edit: OccurrenceEdit::SetHoldDuration { duration },
        identities,
    } = &request.command
    else {
        return Err(invalid(
            "Hold duration preparation requires an EditOccurrence duration command",
        ));
    };
    let requirements = document.occurrence_hold_duration_requirements(instance, *duration)?;
    if identities.nodes.len() != requirements.nodes || identities.marks.len() != requirements.marks
    {
        return Err(invalid(
            "one-play Hold duration requires its exact node and mark identity pools",
        ));
    }
    let fixed_before = fixed_occurrence_preflight(document, instance)?;
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    let mapped = isolation
        .bind_prepared(document, &after)?
        .map_forward(&target(instance))?;
    let presentation = map_instance(instance, isolation.steps());
    if !mapped.matches_instance(&after, &presentation)? {
        return Err(invalid("resized Hold lost its concrete presentation"));
    }
    let fixed_visibility = match fixed_before {
        Some(before) => {
            let visible = fixed_occurrence_preflight(&after, &presentation)?
                .ok_or_else(|| invalid("fixed Hold duration lost its Retime"))?;
            if before.root != visible.root {
                return Err(invalid("fixed Hold duration changed its Repeat root"));
            }
            Some(visible.visibility)
        }
        None => None,
    };
    let resolve_boundary = || {
        AnchorIndex::new(&after).and_then(|index| {
            index.resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: presentation.clone(),
                        position: crate::ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            })
        })
    };
    let boundary = if fixed_visibility.is_some() {
        crate::derived_clock::with_sealed(resolve_boundary)
    } else {
        resolve_boundary()
    };
    let boundary = match boundary {
        Ok(boundary) => Some(boundary),
        Err(error)
            if fixed_visibility.is_some()
                && error.code == crate::AnchorErrorCode::OutsideMapping =>
        {
            None
        }
        Err(error) => return Err(invalid(&error.to_string())),
    };
    Ok(PreparedOccurrenceHoldDuration {
        transaction,
        document: after,
        target: mapped,
        presentation,
        boundary,
        isolation,
        fixed_visibility,
    })
}

fn map_instance(instance: &InstancePath, steps: &[ScopedIsolationStep]) -> InstancePath {
    let mut mapped = instance.clone();
    for step in steps {
        if isolation::matches_prefix(&mapped, step.before_prefix()) {
            crate::occurrence_edit::remap_instance(&mut mapped, step.nodes());
        }
    }
    mapped
}
