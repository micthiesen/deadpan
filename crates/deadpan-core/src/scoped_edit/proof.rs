//! Durable descriptions of isolation, admitted only by replaying the command
//! that actually produced the before/after documents.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, bounded_steps, invalid, limit};
use crate::{
    CommandRequest, EditError, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_JSON_BYTES, MAX_DOCUMENT_NODES,
    NodeId, ProjectDocument, ProjectId, RevisionId,
};

/// One clone in execution order. Prefixes name the scope at this step, after
/// any earlier clones have already been applied. Fields are readable but only
/// a command replay can turn a serialized record into a mapping capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StepWire")]
pub struct ScopedIsolationStep {
    before_prefix: Vec<RepeatEditStep>,
    after_prefix: Vec<RepeatEditStep>,
    nodes: BTreeMap<NodeId, NodeId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepWire {
    #[serde(deserialize_with = "bounded_steps")]
    before_prefix: Vec<RepeatEditStep>,
    #[serde(deserialize_with = "bounded_steps")]
    after_prefix: Vec<RepeatEditStep>,
    #[serde(deserialize_with = "bounded_nodes")]
    nodes: BTreeMap<NodeId, NodeId>,
}

impl TryFrom<StepWire> for ScopedIsolationStep {
    type Error = EditError;
    fn try_from(wire: StepWire) -> Result<Self, Self::Error> {
        let value = Self {
            before_prefix: wire.before_prefix,
            after_prefix: wire.after_prefix,
            nodes: wire.nodes,
        };
        value.validate_shape()?;
        Ok(value)
    }
}

impl ScopedIsolationStep {
    pub fn before_prefix(&self) -> &[RepeatEditStep] {
        &self.before_prefix
    }
    pub fn after_prefix(&self) -> &[RepeatEditStep] {
        &self.after_prefix
    }
    pub fn nodes(&self) -> &BTreeMap<NodeId, NodeId> {
        &self.nodes
    }

    pub(crate) fn from_execution(
        before_prefix: Vec<RepeatEditStep>,
        nodes: BTreeMap<NodeId, NodeId>,
    ) -> Self {
        let mut after_prefix = before_prefix.clone();
        for step in &mut after_prefix {
            if let Some(repeat) = nodes.get(&step.repeat) {
                step.repeat = repeat.clone();
            }
        }
        Self {
            before_prefix,
            after_prefix,
            nodes,
        }
    }

    pub(crate) fn for_definition_branch(
        document: &ProjectDocument,
        repeat: &NodeId,
        iteration: &crate::IterationId,
        nodes: BTreeMap<NodeId, NodeId>,
    ) -> Result<Self, EditError> {
        let mut current = repeat.clone();
        let mut prefix = Vec::new();
        let mut depth = 0;
        while let Some(parent) = document.parent_of(&current) {
            depth += 1;
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(limit("isolation ancestry exceeds depth bound"));
            }
            if let crate::NodeKind::Repeat { child, .. } = &document.nodes()[&parent].kind {
                let branch = if child == &current {
                    RepeatEditBranch::Default
                } else {
                    let selected = document
                        .overrides()
                        .get(&parent)
                        .into_iter()
                        .chain(document.gap_overrides().get(&parent))
                        .flat_map(|entries| entries.iter())
                        .find_map(|(iteration, root)| (root == &current).then_some(iteration))
                        .ok_or_else(|| invalid("isolation branch is not owned by its Repeat"))?;
                    RepeatEditBranch::Play {
                        iteration: selected.clone(),
                    }
                };
                prefix.push(RepeatEditStep {
                    repeat: parent.clone(),
                    branch,
                });
            }
            current = parent;
        }
        prefix.reverse();
        prefix.push(RepeatEditStep {
            repeat: repeat.clone(),
            branch: RepeatEditBranch::Play {
                iteration: iteration.clone(),
            },
        });
        Ok(Self::from_execution(prefix, nodes))
    }

    fn validate_shape(&self) -> Result<(), EditError> {
        if self.before_prefix.is_empty()
            || self.before_prefix.len() > MAX_DOCUMENT_DEPTH
            || self.after_prefix.len() != self.before_prefix.len()
            || self.nodes.is_empty()
            || self.nodes.len() > MAX_DOCUMENT_NODES
        {
            return Err(limit(
                "scoped isolation step exceeds its depth or node bound",
            ));
        }
        let mut destinations = BTreeSet::new();
        if self
            .nodes
            .iter()
            .any(|(old, fresh)| old == fresh || !destinations.insert(fresh))
        {
            return Err(invalid(
                "scoped isolation destinations must be distinct fresh names",
            ));
        }
        for prefix in [&self.before_prefix, &self.after_prefix] {
            let mut repeats = BTreeSet::new();
            if prefix.iter().any(|step| !repeats.insert(&step.repeat))
                || !matches!(
                    prefix.last().map(|step| &step.branch),
                    Some(RepeatEditBranch::Play { .. })
                )
            {
                return Err(invalid(
                    "scoped isolation prefix must end in one concrete play",
                ));
            }
        }
        Ok(())
    }
}

/// Untrusted until `validate` replays its command against both exact snapshots.
/// A valid empty record means that command cloned no scoped identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RecordWire")]
pub struct ScopedIsolationRecord {
    schema_version: u32,
    project: ProjectId,
    before: RevisionId,
    after: RevisionId,
    steps: Vec<ScopedIsolationStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordWire {
    schema_version: u32,
    project: ProjectId,
    before: RevisionId,
    after: RevisionId,
    #[serde(deserialize_with = "bounded_records")]
    steps: Vec<ScopedIsolationStep>,
}

impl TryFrom<RecordWire> for ScopedIsolationRecord {
    type Error = EditError;
    fn try_from(wire: RecordWire) -> Result<Self, Self::Error> {
        let value = Self {
            schema_version: wire.schema_version,
            project: wire.project,
            before: wire.before,
            after: wire.after,
            steps: wire.steps,
        };
        value.validate_shape()?;
        Ok(value)
    }
}

impl ScopedIsolationRecord {
    pub fn steps(&self) -> &[ScopedIsolationStep] {
        &self.steps
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn validate<'a>(
        &self,
        before: &'a ProjectDocument,
        request: &CommandRequest,
        after: &'a ProjectDocument,
    ) -> Result<ValidatedScopedIsolation<'a>, EditError> {
        self.validate_shape()?;
        let derived = derive_scoped_isolation(before, request, after)?;
        if self != derived.record() {
            return Err(invalid(
                "scoped isolation record differs from its actual command execution",
            ));
        }
        Ok(derived)
    }

    pub(super) fn from_execution(
        before: &ProjectDocument,
        request: &CommandRequest,
        after: &ProjectDocument,
        steps: Vec<ScopedIsolationStep>,
    ) -> Result<Self, EditError> {
        let record = Self {
            schema_version: 1,
            project: request.project_id.clone(),
            before: request.expected_revision.clone(),
            after: request.new_revision.clone(),
            steps,
        };
        record.check_documents(before, after)?;
        record.validate_shape()?;
        Ok(record)
    }

    fn validate_shape(&self) -> Result<(), EditError> {
        if self.schema_version != 1 || self.before == self.after {
            return Err(invalid(
                "invalid scoped isolation record schema or revisions",
            ));
        }
        let mut total = 0_usize;
        let mut destinations = BTreeSet::new();
        for step in &self.steps {
            step.validate_shape()?;
            total = total
                .checked_add(step.nodes.len())
                .filter(|total| *total <= MAX_DOCUMENT_NODES)
                .ok_or_else(|| limit("scoped isolation record exceeds the node bound"))?;
            if step.nodes.values().any(|node| !destinations.insert(node)) {
                return Err(invalid("scoped isolation reuses a destination identity"));
            }
        }
        crate::compound::wire::size(self, MAX_DOCUMENT_JSON_BYTES)?;
        Ok(())
    }

    fn check_documents(
        &self,
        before: &ProjectDocument,
        after: &ProjectDocument,
    ) -> Result<(), EditError> {
        if before.project_id() != &self.project
            || after.project_id() != &self.project
            || before.revision_id() != &self.before
            || after.revision_id() != &self.after
        {
            return Err(invalid(
                "scoped isolation documents differ from the recorded identities",
            ));
        }
        Ok(())
    }

    /// Only prepared execution may bypass replay; this is deliberately not a
    /// public capability on a deserializable record.
    pub(super) fn bind_prepared<'a>(
        &self,
        before: &'a ProjectDocument,
        after: &'a ProjectDocument,
    ) -> Result<ValidatedScopedIsolation<'a>, EditError> {
        self.check_documents(before, after)?;
        Ok(ValidatedScopedIsolation::new(self.clone(), before, after))
    }
}

/// Derived through the production command reducer. Direct commands without
/// isolation produce an empty record. Compound commands collect each retained
/// leaf's actual mappings in execution order through the same replay engine.
pub fn derive_scoped_isolation<'a>(
    before: &'a ProjectDocument,
    request: &CommandRequest,
    after: &'a ProjectDocument,
) -> Result<ValidatedScopedIsolation<'a>, EditError> {
    let (_, computed, steps) = crate::command::apply_with_isolation(before, request)?;
    if &computed != after {
        return Err(invalid(
            "scoped isolation result differs from the actual command result",
        ));
    }
    let record = ScopedIsolationRecord::from_execution(before, request, after, steps)?;
    Ok(ValidatedScopedIsolation::new(record, before, after))
}

#[derive(Debug)]
pub struct ValidatedScopedIsolation<'a> {
    record: ScopedIsolationRecord,
    before: &'a ProjectDocument,
    after: &'a ProjectDocument,
    inverse: Vec<BTreeMap<NodeId, NodeId>>,
}

impl<'a> ValidatedScopedIsolation<'a> {
    fn new(
        record: ScopedIsolationRecord,
        before: &'a ProjectDocument,
        after: &'a ProjectDocument,
    ) -> Self {
        let inverse = record
            .steps
            .iter()
            .map(|step| {
                step.nodes
                    .iter()
                    .map(|(old, fresh)| (fresh.clone(), old.clone()))
                    .collect()
            })
            .collect();
        Self {
            record,
            before,
            after,
            inverse,
        }
    }

    pub fn record(&self) -> &ScopedIsolationRecord {
        &self.record
    }

    pub fn map_forward(&self, target: &ScopedNodeTarget) -> Result<ScopedNodeTarget, EditError> {
        target.validate(self.before)?;
        let result = self.map_retained_forward(target);
        result.validate(self.after)?;
        Ok(result)
    }

    pub fn map_backward(&self, target: &ScopedNodeTarget) -> Result<ScopedNodeTarget, EditError> {
        target.validate(self.after)?;
        let result = self.map_retained_backward(target);
        result.validate(self.before)?;
        Ok(result)
    }

    /// Carry an already captured historical address, including a detached
    /// address whose node was deleted by another edit. This proves only the
    /// identity transform; the caller must separately test current ownership
    /// before treating the result as an available editing target.
    pub fn map_retained_forward(&self, target: &ScopedNodeTarget) -> ScopedNodeTarget {
        let mut result = target.clone();
        for step in &self.record.steps {
            remap(&mut result, &step.before_prefix, &step.nodes);
        }
        result
    }

    /// Inverse of the retained identity transform. It never revives relevance
    /// or grants target availability merely because an old identity returns.
    pub fn map_retained_backward(&self, target: &ScopedNodeTarget) -> ScopedNodeTarget {
        let mut result = target.clone();
        for (step, nodes) in self.record.steps.iter().zip(&self.inverse).rev() {
            remap(&mut result, &step.after_prefix, nodes);
        }
        result
    }
}

pub(super) fn matches_scoped_prefix(target: &ScopedNodeTarget, prefix: &[RepeatEditStep]) -> bool {
    target.repeats.len() >= prefix.len() && target.repeats.iter().zip(prefix).all(|(actual, selected)| {
        actual.repeat == selected.repeat && match &selected.branch {
            RepeatEditBranch::Default => true,
            RepeatEditBranch::Play { iteration } => matches!(&actual.branch, RepeatEditBranch::Play { iteration: actual } if actual == iteration),
        }
    })
}

fn remap(
    target: &mut ScopedNodeTarget,
    prefix: &[RepeatEditStep],
    nodes: &BTreeMap<NodeId, NodeId>,
) {
    if !matches_scoped_prefix(target, prefix) || !nodes.contains_key(&target.node) {
        return;
    }
    target.node = nodes[&target.node].clone();
    for step in &mut target.repeats {
        if let Some(repeat) = nodes.get(&step.repeat) {
            step.repeat = repeat.clone();
        }
    }
}

fn bounded_nodes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<NodeId, NodeId>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<NodeId, NodeId>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded unique node isolation map")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut map = BTreeMap::new();
            while let Some((old, fresh)) = input.next_entry::<NodeId, NodeId>()? {
                if map.len() == MAX_DOCUMENT_NODES || map.insert(old, fresh).is_some() {
                    return Err(serde::de::Error::custom(
                        "isolation node map exceeds its bound or repeats a source",
                    ));
                }
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(Visitor)
}

fn bounded_records<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ScopedIsolationStep>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<ScopedIsolationStep>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded scoped isolation steps")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut steps = Vec::new();
            let mut nodes = 0_usize;
            let mut bytes = 0_usize;
            while let Some(step) = input.next_element::<ScopedIsolationStep>()? {
                nodes = nodes
                    .checked_add(step.nodes.len())
                    .filter(|n| *n <= MAX_DOCUMENT_NODES)
                    .ok_or_else(|| {
                        serde::de::Error::custom("isolation steps exceed the node bound")
                    })?;
                bytes += crate::compound::wire::size(&step, MAX_DOCUMENT_JSON_BYTES - bytes)
                    .map_err(serde::de::Error::custom)?;
                steps.push(step);
            }
            Ok(steps)
        }
    }
    deserializer.deserialize_seq(Visitor)
}
