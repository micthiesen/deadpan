//! One compact authored-definition address per node, without Repeat expansion.

use std::{collections::BTreeMap, sync::Arc};

use deadpan_core::{
    ExactRatio, HoldFallback, HoldVideo, NodeId, NodeKind, ProjectDocument, RepeatEditBranch,
    RepeatEditStep, ScopedNodeTarget, SourcePoint,
};

use super::{CompiledHold, CompiledKind, PlanNode};
use crate::PlanError;

#[derive(Debug, Clone, Copy)]
pub(super) struct DefinitionAddress {
    pub(super) definition: usize,
    pub(super) start: i64,
    pub(super) scope: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct DefinitionScope {
    pub(super) parent: Option<usize>,
    pub(super) step: RepeatEditStep,
}

#[derive(Debug, Clone)]
pub(super) struct DefinitionIndex {
    // Cloned plans share this token. Equal project/revision names alone do not
    // prove that a mutable public sample came from this compiled snapshot.
    pub(super) identity: Arc<()>,
    pub(super) addresses: Vec<DefinitionAddress>,
    pub(super) scopes: Vec<DefinitionScope>,
    pub(super) holds: Vec<usize>,
    pub(super) fallbacks: BTreeMap<usize, CompiledHold>,
}

impl DefinitionIndex {
    pub(super) fn compile(
        document: &ProjectDocument,
        nodes: &[PlanNode],
        by_id: &BTreeMap<NodeId, usize>,
        parents: &[Option<usize>],
        root: usize,
    ) -> Result<Self, PlanError> {
        let mut offsets = vec![0_i64; nodes.len()];
        let mut branches = vec![None; nodes.len()];
        let mut holds = Vec::new();
        let mut fallbacks = BTreeMap::new();
        for (index, node) in nodes.iter().enumerate() {
            match &node.kind {
                CompiledKind::Sequence { entries } => {
                    for entry in entries {
                        offsets[entry.child] = entry.start;
                    }
                }
                CompiledKind::Repeat { default_child, .. } => {
                    branches[*default_child] = Some(RepeatEditStep {
                        repeat: node.inspection.id.clone(),
                        branch: RepeatEditBranch::Default,
                    });
                    for entries in document
                        .overrides()
                        .get(&node.inspection.id)
                        .into_iter()
                        .chain(document.gap_overrides().get(&node.inspection.id))
                    {
                        for (iteration, child) in entries.iter() {
                            branches[by_id[child]] = Some(RepeatEditStep {
                                repeat: node.inspection.id.clone(),
                                branch: RepeatEditBranch::Play {
                                    iteration: iteration.clone(),
                                },
                            });
                        }
                    }
                }
                CompiledKind::Hold { .. } => {
                    holds.push(index);
                    if let NodeKind::Hold { recipe } = &document.nodes()[&node.inspection.id].kind
                        && let HoldVideo::Generated { accepted } = &recipe.video
                    {
                        let fallback = match &accepted.fallback {
                            HoldFallback::Background => CompiledHold::Background,
                            HoldFallback::Freeze { asset, timestamp } => CompiledHold::Freeze {
                                asset: asset.clone(),
                                point: SourcePoint {
                                    ticks: ExactRatio::integer(timestamp.ticks),
                                    time_base: timestamp.time_base,
                                },
                            },
                        };
                        fallbacks.insert(index, fallback);
                    }
                }
                _ => {}
            }
        }
        let mut addresses: Vec<Option<DefinitionAddress>> = vec![None; nodes.len()];
        let mut scopes = Vec::new();
        let mut pending = Vec::new();
        // Memoizing each address visits each parent edge once across this entire
        // pass, even when node-ID order differs from structural preorder.
        for index in 0..nodes.len() {
            let mut current = index;
            while addresses[current].is_none() {
                pending.push(current);
                if pending.len() > deadpan_core::MAX_DOCUMENT_DEPTH + 1 {
                    return Err(PlanError::InvalidPlan(
                        "definition ancestry exceeds its bound",
                    ));
                }
                let Some(parent) = parents[current] else {
                    break;
                };
                current = parent;
            }
            while let Some(current) = pending.pop() {
                let mut address = DefinitionAddress {
                    definition: current,
                    start: 0,
                    scope: None,
                };
                if let Some(parent) = parents[current] {
                    let inherited = addresses[parent]
                        .ok_or(PlanError::InvalidPlan("definition parent was not indexed"))?;
                    address.scope = inherited.scope;
                    if matches!(&nodes[parent].kind, CompiledKind::Sequence { .. }) {
                        address.definition = inherited.definition;
                        address.start = inherited
                            .start
                            .checked_add(offsets[current])
                            .ok_or(deadpan_core::TimeError::Overflow)?;
                    }
                } else if current != root {
                    return Err(PlanError::InvalidPlan("definition node is unreachable"));
                }
                if let Some(step) = branches[current].take() {
                    let parent = address.scope;
                    address.scope = Some(scopes.len());
                    scopes.push(DefinitionScope { parent, step });
                }
                addresses[current] = Some(address);
            }
        }
        Ok(Self {
            identity: Arc::new(()),
            addresses: addresses
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or(PlanError::InvalidPlan("definition address is absent"))?,
            scopes,
            holds,
            fallbacks,
        })
    }

    pub(super) fn target(
        &self,
        node: usize,
        nodes: &[PlanNode],
        budget: &mut super::picture_definition::PictureBudget,
    ) -> Result<ScopedNodeTarget, PlanError> {
        budget.visit()?;
        let mut repeats = Vec::new();
        let mut scope = self.addresses[node].scope;
        while let Some(index) = scope {
            budget.repeat_comparisons(1)?;
            let entry = &self.scopes[index];
            repeats.push(entry.step.clone());
            scope = entry.parent;
        }
        repeats.reverse();
        Ok(ScopedNodeTarget {
            node: nodes[node].inspection.id.clone(),
            repeats,
        })
    }

    pub(super) fn validate(
        &self,
        node: usize,
        target: &ScopedNodeTarget,
        budget: &mut super::picture_definition::PictureBudget,
    ) -> Result<DefinitionAddress, PlanError> {
        budget.visit()?;
        let address = self.addresses[node];
        let mut scope = address.scope;
        for requested in target.repeats.iter().rev() {
            budget.repeat_comparisons(1)?;
            let index = scope.ok_or(PlanError::InvalidScopedHold(
                "the target has extra Repeat ancestors",
            ))?;
            let entry = &self.scopes[index];
            if requested != &entry.step {
                return Err(PlanError::InvalidScopedHold(
                    "the target does not exclusively own its authored branch",
                ));
            }
            scope = entry.parent;
        }
        if scope.is_some() {
            return Err(PlanError::InvalidScopedHold(
                "the target omits a Repeat ancestor",
            ));
        }
        Ok(address)
    }
}
