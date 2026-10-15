use super::*;
use crate::{InsertionBias, IterationId, RepeatInstance, RepeatLayout, ScopedNodeTarget};

/// The ordinary insertion path or a single concrete Repeat play/gap. Existing
/// Sequence seams retain their owner; Repeat seams choose their left branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseInsertionTarget {
    Ordinary(InsertTimeTarget),
    Occurrence(OccurrencePauseTarget),
    /// Inside a FollowSpeed speed change, through `Command::InsertTime`.
    Retimed(RetimedPauseTarget),
}

/// A pause inside one FollowSpeed Retime. The Retime stays the live owner of
/// the pause, so a frozen picture captures only scopes below `picture_parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetimedPauseTarget {
    /// Ordinary Sequence containing the speed change.
    pub navigation_parent: NodeId,
    pub retime: NodeId,
    /// The structure that will own the pause: a Sequence inside the speed
    /// change, or the speed change itself when it wraps a single beat.
    pub picture_parent: NodeId,
    /// Fresh node identities the command consumes besides the pause itself.
    pub required_ids: usize,
    /// Child frames per output frame of the speed change.
    pub slope: ExactRatio,
    /// Whether the cut or the document already needs exact derived clocks.
    pub(crate) exact: bool,
}

impl RetimedPauseTarget {
    /// Whether a pause of `frames` leaves derived clocks in the document:
    /// a fractional child cut, a fractional child span for the pause, or a
    /// document that already has them.
    pub fn needs_exact_clocks(&self, frames: FrameDuration) -> bool {
        self.exact
            || ExactRatio::integer(frames.frames())
                .checked_mul(self.slope)
                .map_or(true, |span| span.denominator() != 1)
    }
}

impl PauseInsertionTarget {
    pub fn navigation_parent(&self) -> &NodeId {
        match self {
            Self::Ordinary(target) => &target.parent,
            Self::Occurrence(target) => &target.navigation_parent,
            Self::Retimed(target) => &target.navigation_parent,
        }
    }
    pub fn picture_parent(&self) -> InstancePath {
        match self {
            Self::Ordinary(target) => InstancePath {
                node: target.parent.clone(),
                repeats: Vec::new(),
            },
            Self::Occurrence(target) => target.picture_parent.clone(),
            Self::Retimed(target) => InstancePath {
                node: target.picture_parent.clone(),
                repeats: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrencePauseTarget {
    /// Ordinary Sequence containing the outermost selected Repeat.
    pub navigation_parent: NodeId,
    /// Existing live parent, before the implicit transparent container exists.
    pub picture_parent: InstancePath,
    pub isolation_nodes: usize,
    pub isolation_marks: usize,
    pub split_nodes: usize,
    pub container: bool,
    pub gap: bool,
    pub(crate) site: Site,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Site {
    Sequence {
        instance: InstancePath,
        index: usize,
    },
    Physical {
        instance: InstancePath,
        at: FrameDuration,
        sequence: Option<(NodeId, usize)>,
    },
    Gap {
        repeat: InstancePath,
        after: IterationId,
        at: FrameDuration,
        duration: FrameDuration,
    },
}

impl OccurrencePauseTarget {
    pub(crate) fn scope(&self) -> ScopedNodeTarget {
        let instance = match &self.site {
            Site::Sequence { instance, .. } | Site::Physical { instance, .. } => instance,
            Site::Gap { repeat, .. } => repeat,
        };
        ScopedNodeTarget {
            node: instance.node.clone(),
            repeats: instance
                .repeats
                .iter()
                .map(|step| crate::RepeatEditStep {
                    repeat: step.node.clone(),
                    branch: crate::RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    },
                })
                .collect(),
        }
    }
}

impl ProjectDocument {
    pub fn pause_insertion_target(
        &self,
        at: ProjectFrame,
    ) -> Result<PauseInsertionTarget, EditError> {
        resolve(self, at, None)
    }

    /// Resolve a pause within one inspected occurrence. Its start and end
    /// belong to that occurrence even when the adjacent play shares the same
    /// project boundary. The caller must choose a concrete Repeat instance.
    pub fn pause_insertion_within(
        &self,
        at: ProjectFrame,
        within: &InstancePath,
    ) -> Result<OccurrencePauseTarget, EditError> {
        for_command(self, at, Some(within))
    }
}

pub(super) fn for_command(
    document: &ProjectDocument,
    at: ProjectFrame,
    within: Option<&InstancePath>,
) -> Result<OccurrencePauseTarget, EditError> {
    match resolve(document, at, within)? {
        PauseInsertionTarget::Occurrence(target) => Ok(target),
        PauseInsertionTarget::Ordinary(_) | PauseInsertionTarget::Retimed(_) => Err(invalid(
            "occurrence pause requires a concrete Repeat branch",
        )),
    }
}

fn resolve(
    document: &ProjectDocument,
    at: ProjectFrame,
    within: Option<&InstancePath>,
) -> Result<PauseInsertionTarget, EditError> {
    if within.is_none()
        && let Some(target) = super::super::retimed::pause_target(document, at)?
    {
        return Ok(PauseInsertionTarget::Retimed(target));
    }
    let durations = document.durations()?;
    if at.0 < 0 || at.0 > durations[document.root()].frames() {
        return Err(invalid("pause boundary is outside the project"));
    }
    let mut instance = InstancePath {
        node: document.root().clone(),
        repeats: Vec::new(),
    };
    let mut position = at.0;
    let mut sequence = None;
    let mut parent = instance.clone();
    let mut navigation_parent = document.root().clone();
    if let Some(within) = within {
        let start = scoped_start(document, &durations, at, within)?;
        instance = within.clone();
        position = start.position;
        parent = start.parent;
        sequence = start.sequence;
        navigation_parent = start.navigation_parent;
    }
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        let (site, split_nodes, container, gap) = match &document.nodes()[&instance.node].kind {
            NodeKind::Sequence { children } => {
                if instance.repeats.is_empty() {
                    navigation_parent = instance.node.clone();
                }
                let boundary =
                    super::super::root_boundary(children, &durations, ProjectFrame(position))?;
                if let Some((child, cut)) = boundary.interior {
                    parent = instance.clone();
                    sequence = Some((instance.node.clone(), boundary.slot));
                    instance.node = child;
                    position = cut;
                    continue;
                }
                parent = instance.clone();
                (
                    Site::Sequence {
                        instance: instance.clone(),
                        index: boundary.slot,
                    },
                    0,
                    false,
                    false,
                )
            }
            NodeKind::Repeat {
                child,
                iterations,
                gap,
                ..
            } => {
                let layout = RepeatLayout::compile_with_gap_overrides(
                    iterations,
                    child,
                    document.overrides().get(&instance.node),
                    gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
                    document.gap_overrides().get(&instance.node),
                    &durations,
                )?;
                // A named scope can start at zero; inward bias selects its
                // first play rather than the preceding external object.
                let bias = if position == 0 {
                    InsertionBias::Right
                } else {
                    InsertionBias::Left
                };
                let location = layout.locate(ExactRatio::integer(position), bias)?;
                if location.position.denominator() != 1 {
                    return Err(invalid(
                        "pause insertion needs an exact local frame boundary",
                    ));
                }
                position = i64::try_from(location.position.numerator()).map_err(|_| overflow())?;
                parent = instance.clone();
                if location.in_gap && location.play.gap_child.is_none() {
                    let recipe = gap
                        .as_ref()
                        .ok_or_else(|| invalid("pause gap has no recipe"))?;
                    if !super::super::ordinary_hold(recipe) {
                        return Err(invalid("pause insertion needs an ordinary gap Hold"));
                    }
                    let interior = position > 0 && position < recipe.duration.frames();
                    (
                        Site::Gap {
                            repeat: instance.clone(),
                            after: location.play.iteration,
                            at: FrameDuration::new(position).map_err(crate::DocumentError::from)?,
                            duration: recipe.duration,
                        },
                        if interior { 4 } else { 0 },
                        !interior,
                        true,
                    )
                } else {
                    instance.repeats.push(RepeatInstance {
                        node: instance.node.clone(),
                        iteration: location.play.iteration,
                    });
                    instance.node = if location.in_gap {
                        location.play.gap_child.expect("owned gap")
                    } else {
                        location.play.child
                    };
                    sequence = None;
                    continue;
                }
            }
            _ => {
                super::super::physical(document, &instance.node)?;
                let interior = position > 0 && position < durations[&instance.node].frames();
                if !interior && let Some((owner, index)) = &sequence {
                    let site = Site::Sequence {
                        instance: InstancePath {
                            node: owner.clone(),
                            repeats: instance.repeats.clone(),
                        },
                        index: index + usize::from(position != 0),
                    };
                    return finish(document, navigation_parent, parent, site, 0, false, false);
                }
                let split_nodes = if interior {
                    super::super::split_node_count(document, &instance.node)?
                        + usize::from(sequence.is_none())
                } else {
                    0
                };
                let container = !interior && sequence.is_none();
                (
                    Site::Physical {
                        instance: instance.clone(),
                        at: FrameDuration::new(position).map_err(crate::DocumentError::from)?,
                        sequence,
                    },
                    split_nodes,
                    container,
                    false,
                )
            }
        };
        if instance.repeats.is_empty() && !gap {
            return document
                .insert_time_target(at)
                .map(PauseInsertionTarget::Ordinary);
        }
        return finish(
            document,
            navigation_parent,
            parent,
            site,
            split_nodes,
            container,
            gap,
        );
    }
    Err(limit("pause insertion exceeds the document depth limit"))
}

fn finish(
    document: &ProjectDocument,
    navigation_parent: NodeId,
    picture_parent: InstancePath,
    site: Site,
    split_nodes: usize,
    container: bool,
    gap: bool,
) -> Result<PauseInsertionTarget, EditError> {
    let mut target = OccurrencePauseTarget {
        navigation_parent,
        picture_parent,
        isolation_nodes: 0,
        isolation_marks: 0,
        split_nodes,
        container,
        gap,
        site,
    };
    let isolation = crate::scoped_edit::temporal_requirements(document, &target.scope())?;
    target.isolation_nodes = isolation.nodes;
    target.isolation_marks = isolation.marks;
    let required = isolation
        .nodes
        .checked_add(split_nodes)
        .and_then(|count| count.checked_add(usize::from(container) + usize::from(gap) + 1))
        .ok_or_else(|| limit("pause identity count"))?;
    if document
        .nodes()
        .len()
        .checked_add(required)
        .is_none_or(|count| count > crate::MAX_DOCUMENT_NODES)
    {
        return Err(limit("pause insertion exceeds the document node limit"));
    }
    Ok(PauseInsertionTarget::Occurrence(target))
}

struct ScopedStart {
    position: i64,
    parent: InstancePath,
    sequence: Option<(NodeId, usize)>,
    navigation_parent: NodeId,
}

fn scoped_start(
    document: &ProjectDocument,
    durations: &std::collections::BTreeMap<NodeId, FrameDuration>,
    at: ProjectFrame,
    within: &InstancePath,
) -> Result<ScopedStart, EditError> {
    within.validate(document)?;
    let outer = within
        .repeats
        .first()
        .ok_or_else(|| invalid("scoped pause requires an explicitly selected Repeat play"))?;
    let index = crate::AnchorIndex::from_durations(document, durations.clone())?;
    let mut ancestor = &within.node;
    while let Some((parent, _)) = index.parents.get(ancestor) {
        if matches!(document.nodes()[parent].kind, NodeKind::Retime { .. }) {
            return Err(invalid(
                "pause inside Retime needs exact derived-clock authoring",
            ));
        }
        ancestor = parent;
    }
    let start = index
        .resolve_target(&crate::AnchorTarget {
            boundary: crate::BoundaryAnchor {
                coordinate: crate::Anchor::Occurrence {
                    instance: within.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    if start.exact_frame.denominator() != 1 {
        return Err(invalid("scoped pause needs an integral occurrence clock"));
    }
    let position = at.0.checked_sub(start.frame.0).ok_or_else(overflow)?;
    if position < 0 || position > durations[&within.node].frames() {
        return Err(invalid("pause boundary is outside the selected occurrence"));
    }
    let parent_node = &index.parents[&within.node].0;
    let mut parent = InstancePath {
        node: parent_node.clone(),
        repeats: within.repeats.clone(),
    };
    if matches!(document.nodes()[parent_node].kind, NodeKind::Repeat { .. }) {
        parent.repeats.pop();
    }
    let sequence = if let NodeKind::Sequence { children } = &document.nodes()[parent_node].kind {
        Some((
            parent_node.clone(),
            children
                .iter()
                .position(|id| id == &within.node)
                .ok_or_else(|| invalid("scoped pause lost its Sequence parent"))?,
        ))
    } else {
        None
    };
    Ok(ScopedStart {
        position,
        parent,
        sequence,
        navigation_parent: index.parents[&outer.node].0.clone(),
    })
}
