use super::*;
use crate::{InsertionBias, IterationId, RepeatInstance, RepeatLayout, ScopedNodeTarget};

/// The ordinary insertion path or a single concrete Repeat play/gap. Existing
/// Sequence seams retain their owner; Repeat seams choose their left branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseInsertionTarget {
    Ordinary(InsertTimeTarget),
    Occurrence(OccurrencePauseTarget),
}

impl PauseInsertionTarget {
    pub fn navigation_parent(&self) -> &NodeId {
        match self {
            Self::Ordinary(target) => &target.parent,
            Self::Occurrence(target) => &target.navigation_parent,
        }
    }
    pub fn picture_parent(&self) -> InstancePath {
        match self {
            Self::Ordinary(target) => InstancePath {
                node: target.parent.clone(),
                repeats: Vec::new(),
            },
            Self::Occurrence(target) => target.picture_parent.clone(),
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
        resolve(self, at)
    }
}

fn resolve(
    document: &ProjectDocument,
    at: ProjectFrame,
) -> Result<PauseInsertionTarget, EditError> {
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
                let location = layout.locate(ExactRatio::integer(position), InsertionBias::Left)?;
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
        let mut target = OccurrencePauseTarget {
            navigation_parent,
            picture_parent: parent,
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
        return Ok(PauseInsertionTarget::Occurrence(target));
    }
    Err(limit("pause insertion exceeds the document depth limit"))
}
