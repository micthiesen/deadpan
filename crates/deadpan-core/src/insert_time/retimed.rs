//! Pause insertion inside a FollowSpeed Retime. The Retime stays the live
//! owner of the new Hold and keeps its slope, so its framing, gain and later
//! speed edits apply to the pause too. A cut at project frame `k` inside the
//! stage lands at child coordinate `start + k*s`, where `s` is the stage's
//! child frames per output frame; the pause occupies `h*s` child frames.
//! Neither value is rounded: fractional windows become derived clocks, while
//! integral results keep ordinary recipes.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BeatNode, Command, DerivedRetimeClock, DocumentError, EditError, ExactRatio, FrameDuration,
    FrameRange, HoldRecipe, MAX_DOCUMENT_NODES, NodeId, NodeKind, PitchPolicy, ProjectDocument,
    ProjectFrame, RetimePurpose, Subtree,
};

/// A strict interior of one FollowSpeed Retime reached through ordinary
/// Sequences from the project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetimedTarget {
    pub retime: NodeId,
    /// Integral output position inside the Retime.
    pub local: i64,
}

/// Where the pause enters the Retime's child structure.
enum Site {
    /// An existing seam of an ordinary Sequence below the Retime.
    Seam { parent: NodeId, slot: usize },
    /// A strict interior of a physical leaf, at an exact local cut.
    Leaf {
        parent: NodeId,
        leaf: NodeId,
        cut: ExactRatio,
    },
}

fn extent_of(
    extents: &dyn Fn(&NodeId) -> Option<ExactRatio>,
    id: &NodeId,
) -> Result<ExactRatio, EditError> {
    extents(id).ok_or_else(|| super::invalid(&format!("pause target {id} has no extent")))
}

fn follow_speed_edit(node: &BeatNode) -> bool {
    matches!(
        node.kind,
        NodeKind::Retime {
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
            ..
        }
    )
}

/// Find a FollowSpeed Retime whose strict interior contains `at`. Seams and
/// every other structure return `None`, leaving them to the existing paths.
pub(crate) fn locate(
    document: &ProjectDocument,
    extents: &dyn Fn(&NodeId) -> Option<ExactRatio>,
    at: ProjectFrame,
) -> Result<Option<RetimedTarget>, EditError> {
    let mut parent = document.root();
    let mut position = ExactRatio::integer(at.0);
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
            return Ok(None);
        };
        let mut offset = ExactRatio::ZERO;
        let mut inside = None;
        for child in children {
            let end = offset
                .checked_add(extent_of(extents, child)?)
                .map_err(DocumentError::from)?;
            if offset.compare(position).is_lt() && position.compare(end).is_lt() {
                inside = Some((
                    child,
                    position.checked_sub(offset).map_err(DocumentError::from)?,
                ));
                break;
            }
            offset = end;
        }
        let Some((mut child, mut local)) = inside else {
            return Ok(None);
        };
        // Transparent windows above the speed change pass their unity
        // selection through, so a split slowed beat still admits a pause.
        for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
            let node = &document.nodes()[child];
            match &node.kind {
                NodeKind::Retime {
                    child: inner,
                    purpose: RetimePurpose::Partition,
                    ..
                } => {
                    local = clock_of(document, child)?
                        .child_start
                        .checked_add(local)
                        .map_err(DocumentError::from)?;
                    child = inner;
                }
                _ => break,
            }
        }
        let node = &document.nodes()[child];
        if matches!(node.kind, NodeKind::Sequence { .. }) {
            parent = child;
            position = local;
            continue;
        }
        if !follow_speed_edit(node) || local.denominator() != 1 {
            return Ok(None);
        }
        return Ok(Some(RetimedTarget {
            retime: child.clone(),
            local: i64::try_from(local.numerator()).map_err(|_| super::overflow())?,
        }));
    }
    Err(super::limit("pause Sequence depth"))
}

thread_local! {
    static EXACT_EDIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the current command leaf authored exact clocks through this path,
/// which the ordinary sealed-content check must not refuse. Clears the flag.
pub(crate) fn take_exact_edit() -> bool {
    EXACT_EDIT.with(|flag| flag.replace(false))
}

/// Document content the retimed path cannot yet transform.
fn supported(document: &ProjectDocument) -> Result<(), EditError> {
    if !document.marks().is_empty()
        || !document.sounds().is_empty()
        || !document.beat_sounds().is_empty()
        || !document.sound_routes().is_empty()
        || !document.sound_allowances().is_empty()
    {
        return Err(super::invalid(
            "a pause inside a speed change cannot yet retain marks or placed sounds",
        ));
    }
    Ok(())
}

/// The public query for hosts. Integral documents use their validated
/// durations; documents with exact clocks must target a FollowSpeed interior.
pub(crate) fn pause_target(
    document: &ProjectDocument,
    at: ProjectFrame,
) -> Result<Option<crate::RetimedPauseTarget>, EditError> {
    let extents = if document.derived_clocks().is_empty() {
        let durations = document.durations()?;
        let found = locate(
            document,
            &|id| durations.get(id).map(|d| ExactRatio::integer(d.frames())),
            at,
        )?;
        if found.is_none() {
            return Ok(None);
        }
        document.exact_durations()?
    } else {
        document.exact_durations()?
    };
    let Some(target) = locate(document, &|id| extents.get(id).copied(), at)? else {
        return Ok(None);
    };
    supported(document)?;
    let plan = Plan::new(document, &extents, &target)?;
    let picture_parent = match &plan.site {
        Site::Seam { parent, .. } | Site::Leaf { parent, .. } => parent.clone(),
    };
    Ok(Some(crate::RetimedPauseTarget {
        navigation_parent: plan
            .outer
            .last()
            .map_or_else(|| document.root().clone(), |(parent, _)| parent.clone()),
        retime: target.retime,
        picture_parent,
        required_ids: plan.required_ids(document)?,
        slope: plan.slope,
        exact: !document.derived_clocks().is_empty()
            || matches!(plan.site, Site::Leaf { cut, .. } if cut.denominator() != 1),
    }))
}

/// Fresh node identities consumed, in order: a left partition unless the leaf
/// is refined, the right partition, the right copy in preorder, a container
/// when the leaf's parent is not a Sequence, then the speed adapter when the
/// Retime is not unity. Unused identities are not persisted.
struct Plan {
    retime: NodeId,
    window: (ExactRatio, ExactRatio),
    extent: ExactRatio,
    /// Child frames per output frame.
    slope: ExactRatio,
    site: Site,
    /// Ordinary Sequences from the root through the Retime's ancestors, each
    /// with the slot of the child that leads to the pause.
    outer: Vec<(NodeId, usize)>,
    /// Ordinary Sequences inside the Retime down to the site's parent.
    inner: Vec<(NodeId, usize)>,
    /// Transparent windows above the Retime, innermost first. Each shows the
    /// pause, so each grows by its length.
    windows: Vec<NodeId>,
}

fn clock_of(document: &ProjectDocument, id: &NodeId) -> Result<DerivedRetimeClock, EditError> {
    let NodeKind::Retime {
        duration, mapping, ..
    } = &document.nodes()[id].kind
    else {
        return Err(super::invalid("pause clock owner is not a Retime"));
    };
    Ok(document
        .derived_clocks
        .get(id)
        .cloned()
        .unwrap_or(DerivedRetimeClock {
            extent: ExactRatio::integer(duration.frames()),
            child_start: ExactRatio::integer(mapping.start().0),
            child_end: ExactRatio::integer(mapping.end().0),
            preserve_input: None,
        }))
}

/// An untreated transparent window can be refined in place.
fn refinable(node: &BeatNode) -> bool {
    matches!(
        node.kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ) && node.framing.is_none()
        && node.audio_treatments.is_empty()
        && node.audio_editorial_edges.is_empty()
}

impl Plan {
    fn new(
        document: &ProjectDocument,
        extents: &BTreeMap<NodeId, ExactRatio>,
        target: &RetimedTarget,
    ) -> Result<Self, EditError> {
        let mut outer = Vec::new();
        let mut windows = Vec::new();
        let mut current = target.retime.clone();
        while let Some(parent) = document.parent_of(&current) {
            match &document.nodes()[&parent].kind {
                NodeKind::Sequence { children } => {
                    let slot = children
                        .iter()
                        .position(|child| child == &current)
                        .ok_or_else(|| super::invalid("pause Retime is missing from its parent"))?;
                    outer.push((parent.clone(), slot));
                }
                NodeKind::Retime {
                    purpose: RetimePurpose::Partition,
                    ..
                } => windows.push(parent.clone()),
                _ => {
                    return Err(super::invalid(
                        "a pause inside a speed change needs ordinary Sequence or transparent window ancestors",
                    ));
                }
            }
            current = parent;
        }
        outer.reverse();
        let clock = clock_of(document, &target.retime)?;
        let selected = clock
            .child_end
            .checked_sub(clock.child_start)
            .map_err(DocumentError::from)?;
        let slope = selected
            .checked_div(clock.extent)
            .map_err(DocumentError::from)?;
        let cut = clock
            .child_start
            .checked_add(
                ExactRatio::integer(target.local)
                    .checked_mul(slope)
                    .map_err(DocumentError::from)?,
            )
            .map_err(DocumentError::from)?;
        let NodeKind::Retime { child, .. } = &document.nodes()[&target.retime].kind else {
            return Err(super::invalid("pause clock owner is not a Retime"));
        };
        let mut inner = Vec::new();
        let mut parent = target.retime.clone();
        let mut node = child.clone();
        let mut position = cut;
        let site = loop {
            if inner.len() > crate::MAX_DOCUMENT_DEPTH {
                return Err(super::limit("pause Sequence depth"));
            }
            let NodeKind::Sequence { children } = &document.nodes()[&node].kind else {
                super::physical_context(document, &node, true)?;
                break Site::Leaf {
                    parent,
                    leaf: node,
                    cut: position,
                };
            };
            let mut offset = ExactRatio::ZERO;
            let mut found = None;
            for (slot, candidate) in children.iter().enumerate() {
                if offset == position {
                    found = Some((slot, None));
                    break;
                }
                let end = offset
                    .checked_add(extent_of(&|id| extents.get(id).copied(), candidate)?)
                    .map_err(DocumentError::from)?;
                if offset.compare(position).is_lt() && position.compare(end).is_lt() {
                    found = Some((
                        slot,
                        Some(position.checked_sub(offset).map_err(DocumentError::from)?),
                    ));
                    break;
                }
                offset = end;
            }
            let (slot, local) = match found {
                Some(found) => found,
                None if offset == position => (children.len(), None),
                None => return Err(super::invalid("pause cut is outside the speed change")),
            };
            match local {
                None => break Site::Seam { parent: node, slot },
                Some(local) => {
                    let child = children[slot].clone();
                    if !matches!(document.nodes()[&child].kind, NodeKind::Sequence { .. }) {
                        super::physical_context(document, &child, true)?;
                        break Site::Leaf {
                            parent: node,
                            leaf: child,
                            cut: local,
                        };
                    }
                    inner.push((node.clone(), slot));
                    parent = node;
                    node = child;
                    position = local;
                }
            }
        };
        Ok(Self {
            retime: target.retime.clone(),
            window: (clock.child_start, clock.child_end),
            extent: clock.extent,
            slope,
            site,
            outer,
            inner,
            windows,
        })
    }

    fn adapter(&self) -> bool {
        self.slope != ExactRatio::ONE
    }

    fn required_ids(&self, document: &ProjectDocument) -> Result<usize, EditError> {
        let mut count = usize::from(self.adapter());
        if let Site::Leaf { parent, leaf, .. } = &self.site {
            let node = &document.nodes()[leaf];
            let context = if refinable(node) {
                let NodeKind::Retime { child, .. } = &node.kind else {
                    unreachable!("refinable windows are Retimes")
                };
                child
            } else {
                count += 1;
                leaf
            };
            count += 1 + crate::occurrence_edit::subtree_order(document, context)?.len();
            if !matches!(document.nodes()[parent].kind, NodeKind::Sequence { .. }) {
                count += 1;
            }
        }
        Ok(count)
    }
}

/// A transparent window over `child` with exact local bounds. Integral bounds
/// use an ordinary recipe; fractional ones keep `fallback` as the integral
/// recipe and carry the exact window as a derived clock.
#[expect(
    clippy::too_many_arguments,
    reason = "one private constructor for both window halves"
)]
fn window(
    document: &mut ProjectDocument,
    id: &NodeId,
    label: &str,
    child: NodeId,
    start: ExactRatio,
    end: ExactRatio,
    fallback: FrameRange,
    pitch: PitchPolicy,
) -> Result<(), EditError> {
    let integral = start.denominator() == 1 && end.denominator() == 1;
    let range = if integral {
        FrameRange::new(
            ProjectFrame(i64::try_from(start.numerator()).map_err(|_| super::overflow())?),
            ProjectFrame(i64::try_from(end.numerator()).map_err(|_| super::overflow())?),
        )
        .map_err(DocumentError::from)?
    } else {
        fallback
    };
    let mut node = crate::split::partition(label, child, range.start().0, range.end().0, pitch)?;
    if let Some(existing) = document.nodes.get(id) {
        // Refinement keeps the window's identity and label.
        node.label.clone_from(&existing.label);
    }
    document.nodes.insert(id.clone(), node);
    if integral {
        document.derived_clocks.remove(id);
    } else {
        document.derived_clocks.insert(
            id.clone(),
            DerivedRetimeClock {
                extent: end.checked_sub(start).map_err(DocumentError::from)?,
                child_start: start,
                child_end: end,
                preserve_input: None,
            },
        );
    }
    Ok(())
}

/// Set a Retime's effective output and child window, keeping an ordinary
/// recipe when every value is integral.
fn set_clock(
    document: &mut ProjectDocument,
    id: &NodeId,
    extent: ExactRatio,
    start: ExactRatio,
    end: ExactRatio,
) -> Result<(), EditError> {
    let integral = [extent, start, end]
        .iter()
        .all(|value| value.denominator() == 1);
    let node = document
        .nodes
        .get_mut(id)
        .ok_or_else(|| super::invalid("pause clock owner is missing"))?;
    let NodeKind::Retime {
        duration, mapping, ..
    } = &mut node.kind
    else {
        return Err(super::invalid("pause clock owner is not a Retime"));
    };
    if integral {
        let frames =
            |value: ExactRatio| i64::try_from(value.numerator()).map_err(|_| super::overflow());
        *duration = FrameDuration::new(frames(extent)?).map_err(DocumentError::from)?;
        *mapping = FrameRange::new(ProjectFrame(frames(start)?), ProjectFrame(frames(end)?))
            .map_err(DocumentError::from)?;
        document.derived_clocks.remove(id);
    } else {
        // Keep the integral recipe covering the effective clock, so recipe
        // totals above this Retime match the exact ones. Its child recipe
        // always covers the effective window here: new windows and adapters
        // use recipes at least as long as their exact extents.
        let frames = |value: i128| i64::try_from(value).map_err(|_| super::overflow());
        let recipe_end = frames(end.ceil().map_err(DocumentError::from)?)?;
        let recipe_start = frames(start.floor())?;
        *duration = FrameDuration::new(frames(extent.ceil().map_err(DocumentError::from)?)?)
            .map_err(DocumentError::from)?;
        *mapping = FrameRange::new(ProjectFrame(recipe_start), ProjectFrame(recipe_end))
            .map_err(DocumentError::from)?;
        let preserve_input = document
            .derived_clocks
            .get(id)
            .and_then(|clock| clock.preserve_input.clone());
        document.derived_clocks.insert(
            id.clone(),
            DerivedRetimeClock {
                extent,
                child_start: start,
                child_end: end,
                preserve_input,
            },
        );
    }
    Ok(())
}

fn ceil_frames(value: ExactRatio) -> Result<i64, EditError> {
    let value = value.ceil().map_err(DocumentError::from)?;
    i64::try_from(value.max(1)).map_err(|_| super::overflow())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the InsertTime command fields plus its allocation"
)]
pub(crate) fn apply(
    document: &ProjectDocument,
    target: &RetimedTarget,
    at: ProjectFrame,
    hold: &HoldRecipe,
    id: &NodeId,
    identities: &crate::SplitIdentities,
    timing: &crate::AudioTimingId,
    allocation: &crate::RevisionId,
) -> Result<ProjectDocument, EditError> {
    supported(document)?;
    // Heard or played-back pause content would be resampled by the stage.
    if !matches!(
        hold.video,
        crate::HoldVideo::Background | crate::HoldVideo::Freeze { .. }
    ) || hold.audio != crate::HoldAudio::Silence
    {
        return Err(super::invalid(
            "a pause inside a speed change must be a silent freeze or black pause for now",
        ));
    }
    let extents = document.exact_durations()?;
    let total = extents[document.root()];
    let plan = Plan::new(document, &extents, target)?;
    let required = plan.required_ids(document)?;
    if identities.nodes.len() < required {
        return Err(super::invalid(
            "pause insertion needs more Split node identities",
        ));
    }
    if document
        .nodes()
        .len()
        .checked_add(required + 1)
        .is_none_or(|count| count > MAX_DOCUMENT_NODES)
    {
        return Err(super::limit(
            "pause insertion exceeds the document node limit",
        ));
    }
    let hold_frames = ExactRatio::integer(hold.duration.frames());
    let child_span = hold_frames
        .checked_mul(plan.slope)
        .map_err(DocumentError::from)?;
    let placement_timing = crate::AudioTimingId {
        allocation: timing.allocation.clone(),
        ordinal: timing
            .ordinal
            .checked_add(1)
            .ok_or_else(|| super::limit("pause needs a second timing identity"))?,
    };

    // Capture sampling lattices before copying any physical owner, so the
    // right copy inherits the original's phase rather than a new origin.
    let mut working = document.clone();
    working.audio_bindings =
        crate::audio_binding_lifecycle::capture_unbound_audio_bindings(document, timing.clone())?;
    let mut supplied = identities.nodes.iter().cloned();
    let mut clocked = vec![plan.retime.clone()];
    // The Sequence that receives the pause, its slot, and the owners whose
    // allocation moves later.
    let (parent, slot, mut moved) = match &plan.site {
        Site::Seam { parent, slot } => {
            let NodeKind::Sequence { children } = &working.nodes()[parent].kind else {
                unreachable!("seams belong to Sequences")
            };
            (parent.clone(), *slot, children[*slot..].to_vec())
        }
        Site::Leaf { parent, leaf, cut } => {
            let node = working.nodes()[leaf].clone();
            let leaf_extent = extents[leaf];
            let (left, context, start, end) = if refinable(&node) {
                let NodeKind::Retime { child, .. } = &node.kind else {
                    unreachable!("refinable windows are Retimes")
                };
                let clock = clock_of(&working, leaf)?;
                (
                    leaf.clone(),
                    child.clone(),
                    clock.child_start,
                    clock.child_end,
                )
            } else {
                let left = supplied.next().expect("preflighted partition");
                (left, leaf.clone(), ExactRatio::ZERO, leaf_extent)
            };
            let seam = start.checked_add(*cut).map_err(DocumentError::from)?;
            let right = supplied.next().expect("preflighted partition");
            // An integral recipe that stays valid for the context: its whole
            // recipe span, or the refined window's own recipe.
            let fallback = match &node.kind {
                NodeKind::Retime {
                    mapping,
                    purpose: RetimePurpose::Partition,
                    ..
                } if left == *leaf => *mapping,
                _ => FrameRange::new(
                    ProjectFrame(0),
                    ProjectFrame(document.recipe_durations_shared()?[&context].frames()),
                )
                .map_err(DocumentError::from)?,
            };
            // A refined window keeps its pitch label on both halves; new
            // windows are affine FollowSpeed selections.
            let pitch = match &node.kind {
                NodeKind::Retime { pitch, .. } if left == *leaf => *pitch,
                _ => PitchPolicy::FollowSpeed,
            };
            let mut copies = BTreeMap::new();
            for old in crate::occurrence_edit::subtree_order(&working, &context)? {
                copies.insert(old, supplied.next().expect("preflighted subtree"));
            }
            crate::occurrence_edit::clone_nodes(&mut working, &copies, allocation)?;
            window(
                &mut working,
                &left,
                &node.label,
                context.clone(),
                start,
                seam,
                fallback,
                pitch,
            )?;
            window(
                &mut working,
                &right,
                &node.label,
                copies[&context].clone(),
                seam,
                end,
                fallback,
                pitch,
            )?;
            clocked.extend([left.clone(), right.clone()]);
            let pair = if left == *leaf {
                vec![right.clone()]
            } else {
                vec![left.clone(), right.clone()]
            };
            let (container, index) = if let NodeKind::Sequence { children } = &mut working
                .nodes
                .get_mut(parent)
                .expect("validated parent")
                .kind
            {
                let index = children
                    .iter()
                    .position(|child| child == leaf)
                    .ok_or_else(|| super::invalid("pause leaf is missing from its parent"))?;
                if left == *leaf {
                    children.insert(index + 1, right.clone());
                } else {
                    children.splice(index..=index, pair);
                }
                (parent.clone(), index + 1)
            } else {
                let container = supplied.next().expect("preflighted container");
                let mut children = pair;
                if left == *leaf {
                    children.insert(0, leaf.clone());
                }
                working
                    .nodes
                    .insert(container.clone(), BeatNode::sequence(&node.label, children));
                crate::command::replace_child(&mut working, parent, leaf, container.clone())?;
                (container, 1)
            };
            let NodeKind::Sequence { children } = &working.nodes()[&container].kind else {
                unreachable!("container is a Sequence")
            };
            (container.clone(), index, children[index..].to_vec())
        }
    };
    // Later siblings at every enclosing Sequence also move.
    for (sequence, slot) in plan.inner.iter().rev().chain(plan.outer.iter().rev()) {
        let NodeKind::Sequence { children } = &working.nodes()[sequence].kind else {
            unreachable!("path Sequences are unchanged")
        };
        moved.extend(children[slot + 1..].iter().cloned());
    }
    let mut affected = BTreeSet::new();
    while let Some(id) = moved.pop() {
        if affected.len() >= MAX_DOCUMENT_NODES || !affected.insert(id.clone()) {
            return Err(super::limit("retimed pause traversal"));
        }
        let node = &working.nodes()[&id];
        let processes = matches!(
            &node.kind,
            NodeKind::Retime { duration, mapping, pitch, .. }
                if pitch.processes(mapping.duration() == *duration)
        );
        if !processes {
            moved.extend(working.children(&id).cloned());
        }
    }
    let at_frame = ProjectFrame(at.0);
    let total_frames = crate::derived_clock::integral(total)?.frames();
    working = super::composite::prepare_owners(
        &working,
        &affected,
        at_frame,
        total_frames,
        &placement_timing,
    )?;

    // The pause keeps its authored project duration. Under a non-unity stage,
    // an adapter maps it onto its exact child span.
    let hold_node = BeatNode::hold("Pause", hold.clone());
    let (root, nodes) = if plan.adapter() {
        let adapter = supplied.next().expect("preflighted adapter");
        let mut node = crate::split::partition(
            "Pause",
            id.clone(),
            0,
            hold.duration.frames(),
            PitchPolicy::FollowSpeed,
        )?;
        let NodeKind::Retime {
            duration, purpose, ..
        } = &mut node.kind
        else {
            unreachable!("partition builds a Retime")
        };
        *purpose = RetimePurpose::Edit;
        *duration = FrameDuration::new(ceil_frames(child_span)?).map_err(DocumentError::from)?;
        clocked.push(adapter.clone());
        (
            adapter.clone(),
            BTreeMap::from([(adapter, node), (id.clone(), hold_node)]),
        )
    } else {
        (id.clone(), BTreeMap::from([(id.clone(), hold_node)]))
    };
    // Split stages keep lineage, as an ordinary Split does; only the
    // insertion and the clocks it changes detach their owners.
    let clocked: Vec<NodeId> = clocked
        .into_iter()
        .filter(|id| id == &plan.retime || id == &root)
        .collect();
    let mut result = working.clone();
    crate::command::reduce(
        &mut result,
        &Command::Insert {
            parent,
            index: slot,
            subtree: Subtree {
                root: root.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        allocation,
    )?;
    if plan.adapter() {
        set_clock(
            &mut result,
            &root,
            child_span,
            ExactRatio::ZERO,
            hold_frames,
        )?;
    }
    set_clock(
        &mut result,
        &plan.retime,
        plan.extent
            .checked_add(hold_frames)
            .map_err(DocumentError::from)?,
        plan.window.0,
        plan.window
            .1
            .checked_add(child_span)
            .map_err(DocumentError::from)?,
    )?;
    for window in &plan.windows {
        let clock = clock_of(&result, window)?;
        set_clock(
            &mut result,
            window,
            clock
                .extent
                .checked_add(hold_frames)
                .map_err(DocumentError::from)?,
            clock.child_start,
            clock
                .child_end
                .checked_add(hold_frames)
                .map_err(DocumentError::from)?,
        )?;
    }
    let clocked: Vec<NodeId> = clocked
        .into_iter()
        .chain(plan.windows.iter().cloned())
        .collect();
    crate::audio_lineage::reconcile_exact(&working, &mut result, &clocked);
    crate::audio_binding_lifecycle::prune(&mut result);
    result.validate()?;
    EXACT_EDIT.with(|flag| flag.set(true));
    Ok(result)
}
