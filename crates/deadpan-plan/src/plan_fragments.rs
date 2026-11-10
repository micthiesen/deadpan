//! Reuse of compiled node fragments from a previous plan.
//!
//! A [`PlanNode`] is a pure function of its authored node, its own derived
//! clock and sparse overrides, the document's presentation basis and asset
//! catalog, its own recipe duration and extent, and the duration and extent of
//! each direct structural child (Sequence prefix entries and Repeat layouts
//! read nothing deeper). When all of those inputs equal the previous plan's,
//! its fragment is shared; only child positions are renumbered, because
//! positions follow the sorted node identities. A Sequence whose node is
//! unchanged but whose children's sizes changed (an ancestor of a resized
//! edit) keeps its other values and re-derives its prefix entries from zero
//! with the new extents. Every other changed node compiles afresh. Whole-plan
//! passes (parents, the definition index with its absolute definition
//! offsets, sound caches and plan-wide flags) always run in full, and plans
//! hold no root sample allocation, so no absolute coordinate is ever shifted
//! from an older plan.

use std::{cell::Cell, collections::BTreeMap, sync::Arc};

use deadpan_core::{
    BeatNode, DerivedRetimeClock, ExactRatio, FrameDuration, NodeId, PlayOverrides,
    ProjectDocument, SharedSnapshot,
};

use super::{CompiledKind, PlanNode, RenderPlan, SequenceEntry};
use crate::PlanError;

/// The previous revision's inputs that a fragment depends on beyond its node.
#[derive(Debug)]
pub(super) struct FragmentInputs {
    nodes: SharedSnapshot<NodeId, BeatNode>,
    derived_clocks: BTreeMap<NodeId, DerivedRetimeClock>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    exact: bool,
}

impl FragmentInputs {
    pub(super) fn capture(document: &ProjectDocument, exact: bool) -> Self {
        Self {
            nodes: document.shared_nodes(),
            derived_clocks: document.derived_clocks().clone(),
            overrides: document.overrides().clone(),
            gap_overrides: document.gap_overrides().clone(),
            exact,
        }
    }
}

/// Plan-wide reuse decision, made once per compilation. Old and new nodes
/// are matched by one merge of their sorted identities, so the per-node test
/// and every child lookup are positional rather than map searches.
pub(super) struct Reuse<'a> {
    previous: &'a RenderPlan,
    inputs: &'a FragmentInputs,
    same_nodes: bool,
    /// The previous revision's node of each old position.
    old_nodes: Vec<&'a BeatNode>,
    /// New position of each old position, when the identity survives.
    old_to_new: Vec<Option<usize>>,
    /// Old position of each new position, when the identity existed.
    new_to_old: Vec<Option<usize>>,
    /// Recipe duration and extent of each new position.
    sizes: Vec<(FrameDuration, ExactRatio)>,
    pub(super) reused: usize,
}

impl<'a> Reuse<'a> {
    /// Reuse is refused outright when an input every fragment may read
    /// changed: the basis (frame rate), the asset catalog or the exact/integral
    /// extent mode. Both maps describe exactly the document's nodes.
    pub(super) fn new(
        previous: &'a RenderPlan,
        document: &ProjectDocument,
        durations: &BTreeMap<NodeId, FrameDuration>,
        extents: Option<&BTreeMap<NodeId, ExactRatio>>,
    ) -> Option<Self> {
        let inputs = previous.fragments.as_deref()?;
        let sizes: Vec<_> = match extents {
            None => durations
                .values()
                .map(|duration| (*duration, ExactRatio::integer(duration.frames())))
                .collect(),
            Some(extents) => {
                if extents.len() != durations.len() || extents.keys().ne(durations.keys()) {
                    return None;
                }
                durations
                    .values()
                    .copied()
                    .zip(extents.values().copied())
                    .collect()
            }
        };
        if inputs.exact != extents.is_some()
            || previous.metadata.presentation_basis != *document.presentation_basis()
            || previous.audio_assets != *document.assets()
            || inputs.nodes.len() != previous.nodes.len()
        {
            return None;
        }
        let mut old_to_new = vec![None; previous.nodes.len()];
        let mut new_to_old = vec![None; sizes.len()];
        let mut old_nodes = Vec::with_capacity(previous.nodes.len());
        let mut old = inputs.nodes.iter().enumerate().peekable();
        for (new, id) in document.nodes().keys().enumerate() {
            while let Some((position, (old_id, old_node))) = old.peek() {
                match (*old_id).cmp(id) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal => {
                        old_to_new[*position] = Some(new);
                        new_to_old[new] = Some(*position);
                    }
                    std::cmp::Ordering::Greater => break,
                }
                old_nodes.push(*old_node);
                old.next();
            }
        }
        old_nodes.extend(old.map(|(_, (_, node))| node));
        Some(Self {
            previous,
            inputs,
            same_nodes: inputs.nodes.same_allocation(&document.shared_nodes()),
            old_nodes,
            old_to_new,
            new_to_old,
            sizes,
            reused: 0,
        })
    }

    /// Whether the child at old position `old` kept its identity, duration
    /// and extent; its new position when it did.
    fn unchanged_child(&self, old: usize) -> Option<usize> {
        let new = self.old_to_new[old]?;
        let before = &self.previous.nodes[old];
        ((before.inspection.duration, before.extent) == self.sizes[new]).then_some(new)
    }

    /// The previous fragment of the node at new position `new`, when every
    /// input it was compiled from is unchanged, with child positions
    /// renumbered. A Sequence whose only changed inputs are its children's
    /// sizes keeps its other compiled values, takes its new size and
    /// re-derives its prefix entries from the new extents (unchanged leading
    /// entries are equal by construction), exactly as compilation does.
    pub(super) fn fragment(
        &mut self,
        document: &ProjectDocument,
        new: usize,
        id: &NodeId,
        node: &BeatNode,
    ) -> Result<Option<Arc<PlanNode>>, PlanError> {
        let Some(old) = self.new_to_old[new] else {
            return Ok(None);
        };
        let previous = self.previous;
        let fragment = &previous.nodes[old];
        // A Sequence fragment stores its own size but derives nothing from
        // it; its entries are re-derived below. Every other kind compiles
        // values from its own size, so a changed size recompiles it.
        let resized = (fragment.inspection.duration, fragment.extent) != self.sizes[new];
        if (resized && !matches!(fragment.kind, CompiledKind::Sequence { .. }))
            || !(self.same_nodes || self.old_nodes[old] == node)
            || self.inputs.derived_clocks.get(id) != document.derived_clocks().get(id)
            || self.inputs.overrides.get(id) != document.overrides().get(id)
            || self.inputs.gap_overrides.get(id) != document.gap_overrides().get(id)
        {
            return Ok(None);
        }
        // Counted as reused unless its Sequence prefix entries changed.
        let mut rederived = resized;
        let renumbered = match &fragment.kind {
            CompiledKind::Source { .. } | CompiledKind::Hold { .. } => None,
            CompiledKind::Sequence { entries } => {
                let mut stable = true;
                let mut first_changed = None;
                for (index, entry) in entries.iter().enumerate() {
                    let child = self.old_to_new[entry.child];
                    stable &= child == Some(entry.child);
                    if first_changed.is_none() && self.unchanged_child(entry.child).is_none() {
                        first_changed = Some(index);
                    }
                }
                match first_changed {
                    None if stable && !resized => None,
                    first_changed => {
                        rederived |= first_changed.is_some();
                        let first_changed = first_changed.unwrap_or(entries.len());
                        let mut rebuilt = Vec::with_capacity(entries.len());
                        let mut end = ExactRatio::ZERO;
                        for (index, entry) in entries.iter().enumerate() {
                            // Every identity survives: the node is unchanged.
                            let Some(child) = self.old_to_new[entry.child] else {
                                return Ok(None);
                            };
                            if index < first_changed {
                                end = entry.end;
                                rebuilt.push(SequenceEntry { child, ..*entry });
                                continue;
                            }
                            let start = end;
                            end = end.checked_add(self.sizes[child].1)?;
                            rebuilt.push(SequenceEntry { child, start, end });
                        }
                        Some(CompiledKind::Sequence { entries: rebuilt })
                    }
                }
            }
            CompiledKind::Repeat { default_child, .. } => {
                let overrides = document
                    .overrides()
                    .get(id)
                    .into_iter()
                    .chain(document.gap_overrides().get(id))
                    .flat_map(|entries| entries.iter().map(|(_, root)| root));
                for root in overrides {
                    let Some(&before) = previous.by_id.get(root) else {
                        return Ok(None);
                    };
                    if self.unchanged_child(before).is_none() {
                        return Ok(None);
                    }
                }
                let Some(child) = self.unchanged_child(*default_child) else {
                    return Ok(None);
                };
                (child != *default_child).then(|| {
                    let mut kind = fragment.kind.clone();
                    if let CompiledKind::Repeat { default_child, .. } = &mut kind {
                        *default_child = child;
                    }
                    kind
                })
            }
            CompiledKind::Retime { child: before, .. } => {
                let Some(child) = self.unchanged_child(*before) else {
                    return Ok(None);
                };
                (child != *before).then(|| {
                    let mut kind = fragment.kind.clone();
                    if let CompiledKind::Retime {
                        child: renumbered, ..
                    } = &mut kind
                    {
                        *renumbered = child;
                    }
                    kind
                })
            }
        };
        self.reused += usize::from(!rederived);
        Ok(Some(match renumbered {
            None => Arc::clone(fragment),
            Some(kind) => Arc::new(fragment.with_kind(kind, self.sizes[new])),
        }))
    }
}

/// A diagnostic count that is not part of the compiled plan's value, so a
/// reused and a full compilation print identically.
#[derive(Clone, Copy, Default)]
pub(super) struct ReuseCount(pub(super) usize);

impl std::fmt::Debug for ReuseCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("_")
    }
}

thread_local! {
    static ORACLE: Cell<bool> = const { Cell::new(false) };
}

/// Run `operation` with every [`RenderPlan::compile_from`] also compiling
/// the document from scratch and panicking unless both plans are identical
/// in every compiled value. Tests use it as the reference for fragment reuse.
pub fn with_full_compile_oracle<R>(operation: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            ORACLE.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(ORACLE.with(|flag| flag.replace(true)));
    operation()
}

pub(super) fn oracle() -> bool {
    ORACLE.with(Cell::get)
}
