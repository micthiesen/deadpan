//! One outer replay envelope for automatic accepted-provider fallback changes.
//! The host proves why replacement is required; core proves the exact edit and
//! that its final targets need no further occurrence isolation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AcceptedGeneration, Command, EditError, EditErrorCode, HoldFallback, HoldVideo,
    MAX_COMPOUND_WIRE_BYTES, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, NodeKind,
    ProjectDocument, RepeatEditBranch, ScopedNodeTarget,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryReplacement {
    pub target: ScopedNodeTarget,
    pub accepted: Box<AcceptedGeneration>,
}

/// The base command keeps its original allocation revision and Compound leaves.
/// Entries name its final, exclusive authored definitions. An already matching
/// fallback is a marker only: authorization to renew intent remains host work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryReplacementEdit {
    command: Box<Command>,
    replacements: Vec<BoundaryReplacement>,
}

impl BoundaryReplacementEdit {
    pub fn new(
        command: Command,
        replacements: Vec<BoundaryReplacement>,
    ) -> Result<Self, EditError> {
        if matches!(command, Command::WithBoundaryReplacements { .. }) {
            return Err(invalid("boundary replacement envelopes cannot be nested"));
        }
        if replacements.is_empty() || replacements.len() > MAX_DOCUMENT_NODES {
            return Err(limit(
                "boundary replacements need 1..=MAX_DOCUMENT_NODES entries",
            ));
        }
        let mut nodes = BTreeSet::new();
        let mut previous = None;
        for replacement in &replacements {
            let target = &replacement.target;
            if target.repeats.len() > MAX_DOCUMENT_DEPTH {
                return Err(limit("boundary replacement target exceeds the depth limit"));
            }
            if previous.is_some_and(|previous| previous >= target) || !nodes.insert(&target.node) {
                return Err(invalid(
                    "boundary replacements must be strictly target-sorted with unique nodes",
                ));
            }
            previous = Some(target);
        }
        let result = Self {
            command: Box::new(command),
            replacements,
        };
        // Apply the complete tagged-command byte, depth and scalar-count budget
        // to typed callers too. This also charges the base and entries together.
        #[derive(Serialize)]
        #[serde(tag = "command", rename_all = "snake_case")]
        enum Envelope<'a> {
            WithBoundaryReplacements { edit: &'a BoundaryReplacementEdit },
        }
        let envelope = Envelope::WithBoundaryReplacements { edit: &result };
        crate::compound::wire::size(&envelope, MAX_COMPOUND_WIRE_BYTES)?;
        let value = serde_json::to_value(envelope)
            .map_err(|_| limit("boundary replacement serialized command limit"))?;
        crate::compound::wire::read(value).map_err(|error| limit(&error.to_string()))?;
        Ok(result)
    }

    pub fn command(&self) -> &Command {
        &self.command
    }

    pub fn replacements(&self) -> &[BoundaryReplacement] {
        &self.replacements
    }

    pub(crate) fn restore(&self, document: &mut ProjectDocument) -> Result<(), EditError> {
        // The base was fully validated. Build one canonical parent/address
        // index, validate the entire batch, then mutate only provider values.
        {
            let parents = Parents::new(document)?;
            for replacement in &self.replacements {
                parents.validate(document, &replacement.target)?;
                let NodeKind::Hold { recipe } = &document.nodes()[&replacement.target.node].kind
                else {
                    return Err(invalid("boundary replacement requires an authored Hold"));
                };
                match &recipe.video {
                    HoldVideo::Generated { accepted } if accepted == &replacement.accepted => {}
                    video if *video == fallback(&replacement.accepted.fallback) => {}
                    _ => {
                        return Err(invalid(
                            "boundary replacement differs from the accepted provider or its saved fallback",
                        ));
                    }
                }
            }
        }
        for replacement in &self.replacements {
            let NodeKind::Hold { recipe } = &mut document
                .nodes
                .get_mut(&replacement.target.node)
                .expect("validated replacement node")
                .kind
            else {
                unreachable!("validated replacement Hold")
            };
            recipe.video = fallback(&replacement.accepted.fallback);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for BoundaryReplacementEdit {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            command: Command,
            replacements: Vec<BoundaryReplacement>,
        }
        let value = crate::compound::wire::read(deserializer)?;
        if value
            .get("command")
            .and_then(|command| command.get("command"))
            .and_then(serde_json::Value::as_str)
            == Some("with_boundary_replacements")
        {
            return Err(serde::de::Error::custom(
                "boundary replacement envelopes cannot be nested",
            ));
        }
        let wire = Wire::deserialize(value).map_err(serde::de::Error::custom)?;
        Self::new(wire.command, wire.replacements).map_err(serde::de::Error::custom)
    }
}

fn fallback(saved: &HoldFallback) -> HoldVideo {
    match saved {
        HoldFallback::Background => HoldVideo::Background,
        HoldFallback::Freeze { asset, timestamp } => HoldVideo::Freeze {
            asset: asset.clone(),
            timestamp: *timestamp,
        },
    }
}

/// One entry per owned tree edge. A Repeat edge names its literal canonical
/// branch, never an effective shared Play requiring fresh isolation.
struct Parents<'a>(BTreeMap<&'a NodeId, (&'a NodeId, Option<RepeatEditBranch>)>);

impl<'a> Parents<'a> {
    fn new(document: &'a ProjectDocument) -> Result<Self, EditError> {
        let mut result = BTreeMap::new();
        for (parent, beat) in document.nodes() {
            for child in beat.kind.children() {
                let branch = matches!(beat.kind, NodeKind::Repeat { .. })
                    .then_some(RepeatEditBranch::Default);
                if result.insert(child, (parent, branch)).is_some() {
                    return Err(invalid("boundary replacement found a multiply owned node"));
                }
            }
            for (iteration, child) in document
                .overrides()
                .get(parent)
                .into_iter()
                .chain(document.gap_overrides().get(parent))
                .flat_map(|entries| entries.iter())
            {
                let branch = RepeatEditBranch::Play {
                    iteration: iteration.clone(),
                };
                if result.insert(child, (parent, Some(branch))).is_some() {
                    return Err(invalid("boundary replacement found a multiply owned node"));
                }
            }
        }
        Ok(Self(result))
    }

    fn validate(
        &self,
        document: &ProjectDocument,
        target: &ScopedNodeTarget,
    ) -> Result<(), EditError> {
        if !document.nodes().contains_key(&target.node) {
            return Err(invalid("boundary replacement target is absent"));
        }
        let mut selected = &target.node;
        let mut index = target.repeats.len();
        let mut depth = 0;
        while let Some((parent, branch)) = self.0.get(selected) {
            depth += 1;
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(limit(
                    "boundary replacement ancestry exceeds the depth limit",
                ));
            }
            if let Some(branch) = branch {
                index = index
                    .checked_sub(1)
                    .ok_or_else(|| invalid("boundary replacement omits a Repeat ancestor"))?;
                let step = &target.repeats[index];
                if &step.repeat != *parent || &step.branch != branch {
                    return Err(invalid(
                        "boundary replacement must name an exclusive canonical Repeat branch",
                    ));
                }
            }
            selected = parent;
        }
        if index != 0 || selected != document.root() {
            return Err(invalid(
                "boundary replacement has extra ancestry or an unreachable target",
            ));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
