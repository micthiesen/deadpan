//! Persisted effective clocks, separate from integral processing recipes.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, ExactRepeatLayout, FrameDuration, NodeId,
    NodeKind, PitchPolicy, ProjectDocument, RetimePurpose, TimeError,
};

#[cfg(test)]
mod tests;

/// An exact allocation and child window of a structurally derived Retime.
/// The ordinary Retime fields retain its integral recipe, independently of
/// this effective clock. Neither count may be inferred by rounding the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedRetimeClock {
    pub extent: ExactRatio,
    pub child_start: ExactRatio,
    pub child_end: ExactRatio,
    /// Complete current-tree input to the unchanged intrinsic Preserve recipe.
    /// Output splice placement is independent of this processing operand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_input: Option<crate::PreserveInputProjection>,
}

thread_local! {
    static SEALED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the current thread runs inside [`with_sealed`].
pub(crate) fn sealed_active() -> bool {
    SEALED.with(Cell::get)
}

/// Run an ordinary command on a document with derived clocks. Its integral
/// queries read recipes, which equal exact extents outside clocked Retimes;
/// the caller must refuse a result that changed anything inside them
/// ([`sealed_preserved`]). Restored on return, including on unwind.
pub(crate) fn with_sealed<R>(operation: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SEALED.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(SEALED.with(|flag| flag.replace(true)));
    operation()
}

/// Parent of every node, built once instead of repeated linear lookups.
fn parents(document: &ProjectDocument) -> BTreeMap<&NodeId, &NodeId> {
    let mut parents = BTreeMap::new();
    for id in document.nodes.keys() {
        for child in document.children(id) {
            parents.insert(child, id);
        }
    }
    parents
}

/// Top-level clocked speed changes: Retimes whose subtree holds a derived
/// clock and that sit outside every other such Retime. Their strict
/// descendants are [`ProjectDocument::sealed_nodes`].
fn clocked_owners(document: &ProjectDocument) -> BTreeSet<NodeId> {
    let parents = parents(document);
    let mut clocked = BTreeSet::new();
    for id in document.derived_clocks.keys() {
        let mut current = Some(id);
        while let Some(id) = current {
            if !clocked.insert(id.clone()) {
                break;
            }
            current = parents.get(id).copied();
        }
    }
    clocked
        .iter()
        .filter(|id| matches!(document.nodes[*id].kind, NodeKind::Retime { .. }))
        .filter(|id| {
            let mut current = parents.get(id).copied();
            while let Some(ancestor) = current {
                if clocked.contains(ancestor)
                    && matches!(document.nodes[ancestor].kind, NodeKind::Retime { .. })
                {
                    return false;
                }
                current = parents.get(ancestor).copied();
            }
            true
        })
        .cloned()
        .collect()
}

/// The structure of `id` with child identities replaced by their own
/// structure, including derived clocks and the play/gap keys of sparse Repeat
/// branches. Equal values denote the same timing and content under any node
/// identities.
fn canonical(
    document: &ProjectDocument,
    id: &NodeId,
    budget: &mut usize,
) -> Result<serde_json::Value, DocumentError> {
    *budget = budget.checked_sub(1).ok_or_else(|| {
        DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "speed-change content is too large to compare",
        )
    })?;
    let node = &document.nodes[id];
    let mut value = serde_json::to_value(node).map_err(|_| {
        DocumentError::new(DocumentErrorCode::InvalidTree, "node does not serialize")
    })?;
    if let Some(kind) = value.get_mut("kind") {
        for field in ["child", "children"] {
            if let Some(slot) = kind.get_mut(field) {
                *slot = serde_json::Value::Null;
            }
        }
    }
    let children = node
        .kind
        .children()
        .iter()
        .map(|child| canonical(document, child, budget))
        .collect::<Result<Vec<_>, _>>()?;
    let mut branches = Vec::new();
    for (name, map) in [
        ("play", document.overrides.get(id)),
        ("gap", document.gap_overrides.get(id)),
    ] {
        for (iteration, root) in map.into_iter().flat_map(|entries| entries.iter()) {
            branches.push(serde_json::json!({
                "branch": name,
                "iteration": serde_json::to_value(iteration).unwrap_or_default(),
                "root": canonical(document, root, budget)?,
            }));
        }
    }
    let clock = document
        .derived_clocks
        .get(id)
        .map(|clock| serde_json::to_value(clock).unwrap_or(serde_json::Value::Null));
    Ok(serde_json::json!({
        "node": value,
        "children": children,
        "branches": branches,
        "clock": clock,
    }))
}

/// Whether `id`'s subtree is literally unchanged: the same identities, nodes,
/// clocks and sparse branches. A cheap test before any structural comparison.
fn same_subtree(before: &ProjectDocument, after: &ProjectDocument, id: &NodeId) -> bool {
    let mut pending = vec![id];
    while let Some(id) = pending.pop() {
        let (Some(old), Some(new)) = (before.nodes.get(id), after.nodes.get(id)) else {
            return false;
        };
        if old != new
            || before.derived_clocks.get(id) != after.derived_clocks.get(id)
            || before.overrides.get(id) != after.overrides.get(id)
            || before.gap_overrides.get(id) != after.gap_overrides.get(id)
        {
            return false;
        }
        pending.extend(after.children(id));
    }
    true
}

/// A clocked owner's timing: its kind without the child identity, and its
/// derived clock. Label, framing and treatments may change freely.
fn owner_timing(
    document: &ProjectDocument,
    id: &NodeId,
) -> Option<(serde_json::Value, Option<DerivedRetimeClock>)> {
    let mut kind = serde_json::to_value(&document.nodes[id].kind).ok()?;
    if let Some(child) = kind.get_mut("child") {
        *child = serde_json::Value::Null;
    }
    Some((kind, document.derived_clocks.get(id).cloned()))
}

/// Refuse a command result that changed timing or content inside a clocked
/// speed change. Each top-level clocked Retime must keep its timing and
/// unchanged content, be removed, or be an exact copy of a previous one; a
/// new ordinary wrapper may hold one. Attributes outside timing may change.
pub(crate) fn sealed_preserved(
    before: &ProjectDocument,
    after: &ProjectDocument,
) -> Result<(), DocumentError> {
    if before.derived_clocks.is_empty() && after.derived_clocks.is_empty() {
        return Ok(());
    }
    let refused = || {
        DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "this edit would change content inside a speed change with exact pause clocks; edit outside it or undo those pauses first",
        )
    };
    let previous = clocked_owners(before);
    let current = clocked_owners(after);
    // A previous owner that survives must remain an owner or sit inside one
    // (a new wrapper); otherwise its clocks were dropped around kept content.
    let sealed = after.sealed_nodes();
    for owner in &previous {
        if after.nodes.contains_key(owner) && !current.contains(owner) && !sealed.contains(owner) {
            return Err(refused());
        }
    }
    let mut budget = crate::MAX_DOCUMENT_NODES.saturating_mul(4);
    let mut originals: Option<BTreeSet<String>> = None;
    let mut matches_original = |document: &ProjectDocument,
                                id: &NodeId,
                                budget: &mut usize|
     -> Result<bool, DocumentError> {
        if originals.is_none() {
            let mut values = BTreeSet::new();
            for owner in &previous {
                values.insert(canonical(before, owner, budget)?.to_string());
            }
            originals = Some(values);
        }
        let value = canonical(document, id, budget)?.to_string();
        Ok(originals
            .as_ref()
            .is_some_and(|values| values.contains(&value)))
    };
    for owner in &current {
        let NodeKind::Retime { child, .. } = &after.nodes[owner].kind else {
            unreachable!("clocked owners are Retimes")
        };
        let unchanged = if previous.contains(owner) {
            owner_timing(before, owner) == owner_timing(after, owner)
                && (same_subtree(before, after, child)
                    || matches!(&before.nodes[owner].kind, NodeKind::Retime { child: old, .. }
                        if canonical(before, old, &mut budget)? == canonical(after, child, &mut budget)?))
        } else if after.derived_clocks.contains_key(owner) {
            // A copy of a clocked owner.
            matches_original(after, owner, &mut budget)?
        } else {
            // A new ordinary wrapper around a previous clocked owner.
            matches_original(after, child, &mut budget)?
        };
        if !unchanged {
            return Err(refused());
        }
    }
    if after.derived_clocks.is_empty() {
        Ok(())
    } else {
        after.sealed_recipes_exact()
    }
}

fn unsupported() -> DocumentError {
    DocumentError::new(
        DocumentErrorCode::InvalidTree,
        "this part of the project is inside a speed change with exact pause timing, which this view or command cannot use yet",
    )
}

pub(crate) fn integral(value: ExactRatio) -> Result<FrameDuration, DocumentError> {
    if value.denominator() != 1 {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidDuration,
            "effective duration is fractional; use the exact duration query",
        ));
    }
    Ok(FrameDuration::new(
        i64::try_from(value.numerator()).map_err(|_| TimeError::Overflow)?,
    )?)
}

impl ProjectDocument {
    pub fn derived_clocks(&self) -> &BTreeMap<NodeId, DerivedRetimeClock> {
        &self.derived_clocks
    }

    /// Integral queries refuse derived clocks, except inside a sealed command
    /// scope whose document keeps every recipe outside clocked Retimes equal
    /// to its exact extent (see [`with_sealed`]).
    pub(crate) fn require_integral_clocks(&self) -> Result<(), DocumentError> {
        if self.derived_clocks.is_empty()
            || (SEALED.with(Cell::get) && self.sealed_recipes_exact().is_ok())
        {
            Ok(())
        } else {
            Err(unsupported())
        }
    }

    /// Strict descendants of every Retime whose subtree holds a derived
    /// clock. Their integral recipes need not equal their exact extents.
    pub(crate) fn sealed_nodes(&self) -> BTreeSet<NodeId> {
        let mut sealed = BTreeSet::new();
        let mut pending: Vec<&NodeId> = Vec::new();
        let owners = clocked_owners(self);
        for owner in &owners {
            pending.extend(self.children(owner));
        }
        while let Some(id) = pending.pop() {
            if sealed.insert(id.clone()) {
                pending.extend(self.children(id));
            }
        }
        sealed
    }

    /// Whether integral recipes equal exact extents outside sealed nodes, so
    /// an ordinary command that changes nothing sealed reads exact timing.
    pub(crate) fn sealed_recipes_exact(&self) -> Result<(), DocumentError> {
        let recipes = self.recipe_durations_shared()?;
        let exact = self.effective_durations(&recipes)?;
        let sealed = self.sealed_nodes();
        for (id, duration) in recipes.iter() {
            if !sealed.contains(id) && exact[id] != ExactRatio::integer(duration.frames()) {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    format!(
                        "{id} has an integral recipe that differs from its exact extent outside a speed change"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Whether a Retime prepares audio through pitch processing on either its
    /// integral recipe or its effective clock. Unity Preserve partitions and
    /// every FollowSpeed stage are affine sampling, not processing stages.
    fn retime_processes(&self, id: &NodeId, node: &crate::BeatNode) -> bool {
        let NodeKind::Retime {
            duration,
            mapping,
            pitch,
            ..
        } = &node.kind
        else {
            return false;
        };
        let effective_unity = self.derived_clocks.get(id).is_none_or(|clock| {
            clock
                .child_end
                .checked_sub(clock.child_start)
                .is_ok_and(|selected| selected == clock.extent)
        });
        pitch.processes(mapping.duration() == *duration) || pitch.processes(effective_unity)
    }

    /// Actual allocation of every node. Source/Hold recipes remain integral;
    /// Sequence and Repeat compose exact child extents without rounding.
    pub fn exact_durations(&self) -> Result<BTreeMap<NodeId, ExactRatio>, DocumentError> {
        let recipes = self.recipe_durations_shared()?;
        self.effective_durations(&recipes)
    }

    /// Full structural validation must precede this bounded postorder pass.
    pub(crate) fn effective_durations(
        &self,
        recipes: &BTreeMap<NodeId, FrameDuration>,
    ) -> Result<BTreeMap<NodeId, ExactRatio>, DocumentError> {
        if self.derived_clocks.is_empty() {
            return Ok(recipes
                .iter()
                .map(|(id, duration)| (id.clone(), ExactRatio::integer(duration.frames())))
                .collect());
        }
        // These consumers still use integral owner clocks. Refuse at document
        // admission, including when a clock is installed through a raw patch.
        if !self.marks.is_empty()
            || !self.sounds.is_empty()
            || !self.beat_sounds.is_empty()
            || !self.sound_routes.is_empty()
            || !self.sound_allowances.is_empty()
            || ((!self.audio_lineage.is_empty() || !self.audio_bindings.is_empty())
                && self
                    .nodes
                    .iter()
                    .any(|(id, node)| self.retime_processes(id, node)))
            || self.nodes.iter().any(|(id, node)| {
                self.retime_processes(id, node)
                    && !matches!(node.kind, NodeKind::Retime { pitch: PitchPolicy::Preserve, .. }
                        if self.derived_clocks.get(id).is_some_and(|clock| clock.preserve_input.is_some()))
            })
        {
            return Err(unsupported());
        }
        for id in self.derived_clocks.keys() {
            if !matches!(
                self.nodes.get(id).map(|node| &node.kind),
                Some(NodeKind::Retime { .. })
            ) {
                return Err(DocumentError::new(
                    DocumentErrorCode::InvalidTree,
                    "a derived clock must belong to an existing Retime",
                ));
            }
        }
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut pending = vec![&self.root];
        while let Some(id) = pending.pop() {
            order.push(id);
            pending.extend(self.children(id));
        }
        let mut extents = BTreeMap::new();
        for id in order.into_iter().rev() {
            let extent = match &self.nodes[id].kind {
                NodeKind::Source { .. } | NodeKind::Hold { .. } => {
                    ExactRatio::integer(recipes[id].frames())
                }
                NodeKind::Sequence { children } => {
                    children.iter().try_fold(ExactRatio::ZERO, |sum, child| {
                        sum.checked_add(extents[child])
                    })?
                }
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                    ..
                } => ExactRepeatLayout::compile_with_gap_overrides(
                    iterations,
                    child,
                    self.overrides.get(id),
                    gap.as_ref().map_or(ExactRatio::ZERO, |recipe| {
                        ExactRatio::integer(recipe.duration.frames())
                    }),
                    self.gap_overrides.get(id),
                    &extents,
                )?
                .duration(),
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    purpose,
                    ..
                } => {
                    let ordinary = DerivedRetimeClock {
                        extent: ExactRatio::integer(duration.frames()),
                        child_start: ExactRatio::integer(mapping.start().0),
                        child_end: ExactRatio::integer(mapping.end().0),
                        preserve_input: None,
                    };
                    let clock = self.derived_clocks.get(id).unwrap_or(&ordinary);
                    if !clock.extent.compare_integer(0).is_gt()
                        || clock.child_start.compare_integer(0).is_lt()
                        || !clock.child_end.compare(clock.child_start).is_gt()
                        || clock.child_end.compare(extents[child]).is_gt()
                    {
                        return Err(DocumentError::new(
                            DocumentErrorCode::SourceRangeInvalid,
                            format!(
                                "derived Retime {id} needs a positive extent and a window inside child {child}"
                            ),
                        ));
                    }
                    let selected = clock.child_end.checked_sub(clock.child_start)?;
                    if *purpose == RetimePurpose::Partition && selected != clock.extent {
                        return Err(DocumentError::new(
                            DocumentErrorCode::InvalidTree,
                            "a derived partition must retain unity timing",
                        ));
                    }
                    // Prove the actual compiled map is representable now.
                    selected.checked_div(clock.extent)?;
                    clock.extent
                }
            };
            if extent.compare_integer(i64::MAX).is_gt() {
                return Err(TimeError::Overflow.into());
            }
            extents.insert(id.clone(), extent);
        }
        integral(extents[&self.root])?;
        crate::preserve_input::validate(self, &extents)?;
        Ok(extents)
    }
}
