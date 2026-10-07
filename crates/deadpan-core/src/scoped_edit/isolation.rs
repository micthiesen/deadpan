use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::{
    Anchor, MAX_DOCUMENT_MARK_BINDINGS, MAX_DOCUMENT_MARKS, MAX_DOCUMENT_NODES, RepeatInstance,
};

struct IsolationStep {
    index: usize,
    /// Original structural preorder. Every preceding isolation preserves it.
    nodes: Vec<NodeId>,
}

pub(super) struct IsolationPlan {
    pub(super) requirements: ScopedEditRequirements,
    steps: Vec<IsolationStep>,
}

#[derive(Debug)]
pub(crate) struct IsolationMap {
    /// Scope before this clone, including any earlier remapped ancestors.
    pub(super) prefix: Vec<RepeatEditStep>,
    pub(super) nodes: BTreeMap<NodeId, NodeId>,
    selected: RepeatInstance,
    root: NodeId,
}

impl IsolationMap {
    pub(crate) fn proof(self) -> super::ScopedIsolationStep {
        super::ScopedIsolationStep::from_execution(self.prefix, self.nodes)
    }
}

pub(super) struct ResolvedIsolation {
    pub(super) target: ScopedNodeTarget,
    pub(super) mappings: Vec<IsolationMap>,
}

/// Return the shared branch roots that each Play step must isolate. Existing
/// play overrides and owned gaps, including dormant final gaps, stay owned.
pub(super) fn validate_target(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
) -> Result<Vec<Option<NodeId>>, EditError> {
    if target.repeats.len() > MAX_DOCUMENT_DEPTH {
        return Err(limit("scoped target exceeds depth limit"));
    }
    if !document.nodes().contains_key(&target.node) {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "scoped target node is absent",
        ));
    }
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut selected = &target.node;
    let mut index = target.repeats.len();
    let mut isolate = vec![None; index];
    let mut depth = 0;
    while let Some(parent) = parents.get(selected) {
        depth += 1;
        if depth > MAX_DOCUMENT_DEPTH {
            return Err(limit("scoped target ancestry exceeds depth limit"));
        }
        if let NodeKind::Repeat {
            child, iterations, ..
        } = &document.nodes()[*parent].kind
        {
            index = index
                .checked_sub(1)
                .ok_or_else(|| invalid("scoped target omits a Repeat ancestor"))?;
            let step = &target.repeats[index];
            if &step.repeat != *parent {
                return Err(invalid("scoped target names the wrong Repeat ancestor"));
            }
            match &step.branch {
                RepeatEditBranch::Default => {
                    if selected != child {
                        return Err(invalid(
                            "Default must follow the Repeat's literal default child",
                        ));
                    }
                }
                RepeatEditBranch::Play { iteration } => {
                    if iterations.position(iteration).is_none() {
                        return Err(invalid("scoped target names a retired Repeat play"));
                    }
                    let owned = document
                        .overrides()
                        .get(*parent)
                        .and_then(|entries| entries.get(iteration));
                    let gap = document
                        .gap_overrides()
                        .get(*parent)
                        .and_then(|entries| entries.get(iteration));
                    if selected != owned.unwrap_or(child) && gap != Some(selected) {
                        return Err(invalid(
                            "scoped target is outside the selected play's owned branches",
                        ));
                    }
                    if selected == child && owned.is_none() {
                        isolate[index] = Some(child.clone());
                    }
                }
            }
        }
        selected = parent;
    }
    if index != 0 {
        return Err(invalid("scoped target contains extra Repeat ancestors"));
    }
    Ok(isolate)
}

pub(crate) fn matches_prefix(instance: &InstancePath, prefix: &[RepeatEditStep]) -> bool {
    instance.repeats.len() >= prefix.len()
        && instance
            .repeats
            .iter()
            .zip(prefix)
            .all(|(actual, selected)| {
                actual.node == selected.repeat
                    && match &selected.branch {
                        RepeatEditBranch::Default => true,
                        RepeatEditBranch::Play { iteration } => &actual.iteration == iteration,
                    }
            })
}

pub(super) fn preflight(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    edit: &ScopedNodeEdit,
) -> Result<IsolationPlan, EditError> {
    let roots = validate_target(document, target)?;
    edit.validate(document, &target.node)?;
    let mut requirements = ScopedEditRequirements {
        nodes: 0,
        marks: 0,
        unchanged: edit.unchanged(&document.nodes()[&target.node]),
    };
    let reasserts_fallback = matches!(edit, ScopedNodeEdit::RevertGeneratedHold)
        && matches!(&document.nodes()[&target.node].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video,
                crate::HoldVideo::Freeze { .. } | crate::HoldVideo::Background));
    if requirements.unchanged || reasserts_fallback {
        // Reasserting a provider keeps its exact scoped command for the host,
        // but does not create a Play override for identical picture state.
        return Ok(IsolationPlan {
            requirements,
            steps: Vec::new(),
        });
    }
    let mut bindings = 0_usize;
    let mut owned_marks: BTreeMap<&NodeId, Vec<&crate::MarkId>> = BTreeMap::new();
    for (id, mark) in document.marks() {
        bindings = bindings
            .checked_add(mark.binding_count())
            .filter(|count| *count <= MAX_DOCUMENT_MARK_BINDINGS)
            .ok_or_else(|| limit("scoped isolation exceeds the mark binding limit"))?;
        for (owner, coordinate) in std::iter::once((&mark.owner, &mark.boundary.coordinate)).chain(
            mark.fragments
                .iter()
                .map(|binding| (&binding.owner, &binding.coordinate)),
        ) {
            if matches!(coordinate, Anchor::Local { .. } | Anchor::Source { .. }) {
                owned_marks.entry(owner).or_default().push(id);
            }
        }
    }
    let mut steps = Vec::new();
    for (index, root) in roots.into_iter().enumerate() {
        let Some(root) = root else { continue };
        let nodes = crate::occurrence_edit::subtree_order(document, &root)?;
        requirements.nodes = requirements
            .nodes
            .checked_add(nodes.len())
            .filter(|count| {
                document
                    .nodes()
                    .len()
                    .checked_add(*count)
                    .is_some_and(|total| total <= MAX_DOCUMENT_NODES)
            })
            .ok_or_else(|| limit("scoped isolation exceeds the document node limit"))?;
        // Earlier outer clones copy exactly the subset of these same local /
        // source bindings in their subtree. One nested branch has one such copy;
        // concrete occurrence marks move instead and never consume fresh IDs.
        let mut copies = BTreeSet::new();
        for node in &nodes {
            let Some(marks) = owned_marks.get(node) else {
                continue;
            };
            copies.extend(marks.iter().copied());
            bindings = bindings
                .checked_add(marks.len())
                .filter(|count| *count <= MAX_DOCUMENT_MARK_BINDINGS)
                .ok_or_else(|| limit("scoped isolation exceeds the mark binding limit"))?;
        }
        requirements.marks = requirements
            .marks
            .checked_add(copies.len())
            .filter(|count| {
                document
                    .marks()
                    .len()
                    .checked_add(*count)
                    .is_some_and(|total| total <= MAX_DOCUMENT_MARKS)
            })
            .ok_or_else(|| limit("scoped isolation exceeds the document mark limit"))?;
        steps.push(IsolationStep { index, nodes });
    }
    Ok(IsolationPlan {
        requirements,
        steps,
    })
}

impl IsolationPlan {
    pub(super) fn resolve(
        self,
        document: &ProjectDocument,
        target: &ScopedNodeTarget,
        identities: &OccurrenceIdentities,
    ) -> Result<ResolvedIsolation, EditError> {
        if identities.nodes.len() != self.requirements.nodes
            || identities.marks.len() != self.requirements.marks
        {
            return Err(invalid(
                "scoped edit requires its exact node and mark identity pools",
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
        if identities.nodes.iter().any(|node| !occupied.insert(node)) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "scoped node identities must be fresh and distinct, including retained audio names",
            ));
        }
        let mut marks: BTreeSet<_> = document.marks().keys().collect();
        if identities.marks.iter().any(|mark| !marks.insert(mark)) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "scoped mark identities must be fresh and distinct",
            ));
        }
        let mut target = target.clone();
        let mut supplied = identities.nodes.iter();
        let mut current: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        let mut mappings = Vec::new();
        for step in self.steps {
            let prefix = target.repeats[..=step.index].to_vec();
            let selected = &prefix[step.index];
            let RepeatEditBranch::Play { iteration } = &selected.branch else {
                unreachable!("only concrete shared plays require isolation")
            };
            let mut nodes = BTreeMap::new();
            let mut root = None;
            for original in step.nodes {
                let old = current.get(&original).unwrap_or(&original).clone();
                let new = supplied.next().expect("exact node pool admitted").clone();
                root.get_or_insert_with(|| old.clone());
                nodes.insert(old, new.clone());
                current.insert(original, new);
            }
            let mapping = IsolationMap {
                selected: RepeatInstance {
                    node: selected.repeat.clone(),
                    iteration: iteration.clone(),
                },
                prefix,
                nodes,
                root: root.expect("a shared branch contains its root"),
            };
            if let Some(node) = mapping.nodes.get(&target.node) {
                target.node = node.clone();
            }
            for scope in &mut target.repeats {
                if let Some(repeat) = mapping.nodes.get(&scope.repeat) {
                    scope.repeat = repeat.clone();
                }
            }
            mappings.push(mapping);
        }
        Ok(ResolvedIsolation { target, mappings })
    }
}

/// Bound for one multi-target scoped edit, such as "plays 2-3 only".
pub const MAX_SCOPED_TARGETS: usize = 1024;

/// Apply one value edit to several explicit targets in order, as one result.
/// Each target isolates only its own selected plays; a later target follows
/// the identities an earlier isolation gave its shared ancestors. Targets
/// whose value is already current are skipped and take an empty pool.
pub(crate) fn apply_many_mapped(
    document: &ProjectDocument,
    edits: &[ScopedTargetEdit],
    identities: &[OccurrenceIdentities],
    mut context: crate::command::EditContext<'_>,
) -> Result<(ProjectDocument, Vec<IsolationMap>), EditError> {
    if edits.is_empty() || edits.len() > MAX_SCOPED_TARGETS {
        return Err(limit("a multi-target scoped edit names 1..=1024 targets"));
    }
    if identities.len() != edits.len() {
        return Err(invalid(
            "a multi-target scoped edit needs one pool per target",
        ));
    }
    let mut pending: Vec<_> = edits.iter().map(|edit| edit.target.clone()).collect();
    let mut result = document.clone();
    let mut changed = false;
    let mut all_mappings = Vec::new();
    for index in 0..pending.len() {
        let target = pending[index].clone();
        let edit = &edits[index].edit;
        if preflight(&result, &target, edit)?.requirements.unchanged {
            if !identities[index].nodes.is_empty() || !identities[index].marks.is_empty() {
                return Err(invalid(
                    "an unchanged scoped target requires an empty identity pool",
                ));
            }
            continue;
        }
        let (next, mappings) = apply_mapped(
            &result,
            &target,
            edit,
            &identities[index],
            crate::command::EditContext {
                allocation: context.allocation,
                allowances: context.allowances.as_deref_mut(),
            },
        )?;
        for later in &mut pending[index + 1..] {
            remap_target(later, &mappings);
        }
        all_mappings.extend(mappings);
        result = next;
        changed = true;
    }
    if !changed {
        return Err(invalid("scoped edit does not change any selected value"));
    }
    Ok((result, all_mappings))
}

fn remap_target(target: &mut ScopedNodeTarget, mappings: &[IsolationMap]) {
    for mapping in mappings {
        if super::proof::matches_scoped_prefix(target, &mapping.prefix) {
            if let Some(node) = mapping.nodes.get(&target.node) {
                target.node = node.clone();
            }
            for scope in &mut target.repeats {
                if let Some(repeat) = mapping.nodes.get(&scope.repeat) {
                    scope.repeat = repeat.clone();
                }
            }
        }
    }
}

/// Exact per-target identity needs of [`apply_many_mapped`], staged in order.
pub(crate) fn many_requirements(
    document: &ProjectDocument,
    edits: &[ScopedTargetEdit],
) -> Result<Vec<ScopedEditRequirements>, EditError> {
    if edits.is_empty() || edits.len() > MAX_SCOPED_TARGETS {
        return Err(limit("a multi-target scoped edit names 1..=1024 targets"));
    }
    let allocation = crate::RevisionId::new("scoped-preflight")?;
    let mut pending: Vec<_> = edits.iter().map(|edit| edit.target.clone()).collect();
    let mut staged = document.clone();
    let mut output = Vec::with_capacity(edits.len());
    for index in 0..pending.len() {
        let target = pending[index].clone();
        let edit = &edits[index].edit;
        let requirements = preflight(&staged, &target, edit)?.requirements;
        output.push(requirements);
        if requirements.unchanged {
            continue;
        }
        let placeholders = OccurrenceIdentities {
            nodes: (0..requirements.nodes)
                .map(|n| NodeId::new(format!("scoped-preflight-{index}-{n}")))
                .collect::<Result<_, _>>()?,
            marks: (0..requirements.marks)
                .map(|n| crate::MarkId::new(format!("scoped-preflight-{index}-{n}")))
                .collect::<Result<_, _>>()?,
        };
        let (next, mappings) = apply_mapped(
            &staged,
            &target,
            edit,
            &placeholders,
            crate::command::EditContext {
                allocation: &allocation,
                allowances: None,
            },
        )?;
        for later in &mut pending[index + 1..] {
            remap_target(later, &mappings);
        }
        staged = next;
    }
    Ok(output)
}

pub(crate) fn apply_mapped(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    edit: &ScopedNodeEdit,
    identities: &OccurrenceIdentities,
    mut context: crate::command::EditContext<'_>,
) -> Result<(ProjectDocument, Vec<IsolationMap>), EditError> {
    let plan = preflight(document, target, edit)?;
    let unchanged = plan.requirements.unchanged;
    let resolved = plan.resolve(document, target, identities)?;
    if unchanged {
        return Err(invalid("scoped edit does not change the selected value"));
    }
    let mut result = document.clone();
    let mut marks = identities.marks.iter();
    for mapping in &resolved.mappings {
        let copied = crate::marks::clone_occurrence_marks(
            &result,
            &mapping.selected,
            &mapping.nodes,
            || {
                marks.next().cloned().ok_or_else(|| {
                    crate::DocumentError::new(
                        crate::DocumentErrorCode::InvalidIdentity,
                        "scoped mark allocation differs from preflight",
                    )
                })
            },
        )?;
        crate::occurrence_edit::clone_nodes(&mut result, &mapping.nodes, context.allocation)?;
        if let Some(allowances) = context.allowances.as_deref_mut() {
            allowances.isolate_scoped(&mapping.prefix, &mapping.nodes)?;
        }
        result
            .overrides
            .entry(mapping.selected.node.clone())
            .or_default()
            .insert(
                mapping.selected.iteration.clone(),
                mapping.nodes[&mapping.root].clone(),
            );
        result.marks = copied;
    }
    if marks.next().is_some() {
        return Err(invalid("scoped mark allocation differs from preflight"));
    }
    resolved.target.validate(&result)?;
    result.validate_isolated_context()?;
    let before = result.clone();
    let command = edit.command(resolved.target.node);
    crate::command::reduce(&mut result, &command, context.allocation)?;
    if let Some(allowances) = context.allowances.as_deref_mut() {
        allowances.apply_hold_audio_command(&command);
    }
    crate::audio_lineage::reconcile(&before, &mut result, &command)?;
    // Value edits do not move marks. Isolation already moved concrete bindings
    // and copied owned definitions, including dormant and unresolved intent.
    crate::compound::wire::size(&result, crate::MAX_DOCUMENT_JSON_BYTES)?;
    Ok((result, resolved.mappings))
}
