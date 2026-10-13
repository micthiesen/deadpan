//! Owned current-tree operands for a retained intrinsic Preserve stage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    DocumentError, DocumentErrorCode, ExactFrameRange, ExactRatio, InstancePath,
    MAX_DOCUMENT_DEPTH, NodeId, NodeKind, PitchPolicy, ProjectDocument,
};

#[cfg(test)]
mod tests;

pub const MAX_PRESERVE_INPUT_RUNS: usize = 4096;
pub const MAX_DOCUMENT_PRESERVE_INPUT_RUNS: usize = 65_536;

/// Paths are relative to the owning stage, never absolute project occurrences.
/// `Intrinsic` selects a nested stage's complete projected processing output,
/// before its separate splice allocation. Current recipes always remain live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreserveInputProvider {
    Current { target: InstancePath },
    Intrinsic { target: InstancePath },
}

impl PreserveInputProvider {
    pub fn target(&self) -> &InstancePath {
        match self {
            Self::Current { target } | Self::Intrinsic { target } => target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreserveInputRun {
    pub destination: ExactFrameRange,
    pub source: ExactFrameRange,
    pub provider: PreserveInputProvider,
}

/// A complete exact partition of the stage's original input duration.
/// This owns selectors and coordinates, never an old recipe snapshot or PCM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "InputWire")]
pub struct PreserveInputProjection {
    runs: Vec<PreserveInputRun>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputWire {
    #[serde(deserialize_with = "bounded_runs")]
    runs: Vec<PreserveInputRun>,
}

fn bounded_runs<'de, D: Deserializer<'de>>(decoder: D) -> Result<Vec<PreserveInputRun>, D::Error> {
    struct Runs;
    impl<'de> de::Visitor<'de> for Runs {
        type Value = Vec<PreserveInputRun>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded Preserve input partition")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut runs = Vec::new();
            while let Some(run) = seq.next_element()? {
                if runs.len() == MAX_PRESERVE_INPUT_RUNS {
                    return Err(de::Error::custom("Preserve input run limit"));
                }
                runs.push(run);
            }
            Ok(runs)
        }
    }
    decoder.deserialize_seq(Runs)
}

impl TryFrom<InputWire> for PreserveInputProjection {
    type Error = DocumentError;
    fn try_from(wire: InputWire) -> Result<Self, Self::Error> {
        Self::new(wire.runs)
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}

impl PreserveInputProjection {
    pub fn new(runs: Vec<PreserveInputRun>) -> Result<Self, DocumentError> {
        if runs.is_empty() || runs.len() > MAX_PRESERVE_INPUT_RUNS {
            return Err(invalid("Preserve input run count must be within 1..4096"));
        }
        let mut end = ExactRatio::ZERO;
        for run in &runs {
            if run.destination.start != end
                || !run.destination.end.compare(run.destination.start).is_gt()
                || run.source.start.compare_integer(0).is_lt()
                || !run.source.end.compare(run.source.start).is_gt()
                || run.provider.target().repeats.len() > MAX_DOCUMENT_DEPTH
            {
                return Err(invalid(
                    "Preserve input runs require contiguous positive destinations, positive source windows and bounded relative paths",
                ));
            }
            run.source
                .end
                .checked_sub(run.source.start)?
                .checked_div(run.destination.end.checked_sub(run.destination.start)?)?;
            end = run.destination.end;
        }
        Ok(Self { runs })
    }

    pub fn runs(&self) -> &[PreserveInputRun] {
        &self.runs
    }
    pub fn extent(&self) -> ExactRatio {
        self.runs
            .last()
            .expect("nonempty admitted input")
            .destination
            .end
    }
}

/// Called only after unique tree ownership and effective clocks were checked.
pub(crate) fn validate(
    document: &ProjectDocument,
    extents: &BTreeMap<NodeId, ExactRatio>,
) -> Result<(), DocumentError> {
    let mut parents = BTreeMap::new();
    let mut order = Vec::with_capacity(document.nodes().len());
    let mut pending = vec![document.root()];
    while let Some(id) = pending.pop() {
        order.push(id);
        for child in document.children(id) {
            parents.insert(child, id);
            pending.push(child);
        }
    }
    let mut contains_projection = BTreeSet::new();
    for id in order.into_iter().rev() {
        if document
            .derived_clocks()
            .get(id)
            .is_some_and(|clock| clock.preserve_input.is_some())
            || document
                .children(id)
                .any(|child| contains_projection.contains(child))
        {
            contains_projection.insert(id);
        }
    }
    let mut work = 1_000_000usize;
    let mut run_count = 0usize;
    for (owner, clock) in document.derived_clocks() {
        let Some(input) = &clock.preserve_input else {
            continue;
        };
        let NodeKind::Retime {
            child,
            duration,
            mapping,
            pitch,
            ..
        } = &document.nodes()[owner].kind
        else {
            unreachable!("clock ownership validated")
        };
        if *pitch != PitchPolicy::Preserve || *duration == mapping.duration() {
            return Err(invalid(
                "an owned Preserve input requires a nonunity Preserve recipe",
            ));
        }
        if input.extent() != ExactRatio::integer(mapping.duration().frames()) {
            return Err(invalid(
                "owned Preserve input must cover the complete intrinsic operand",
            ));
        }
        run_count = run_count
            .checked_add(input.runs.len())
            .ok_or_else(|| invalid("Preserve input count overflow"))?;
        if run_count > MAX_DOCUMENT_PRESERVE_INPUT_RUNS {
            return Err(invalid("document Preserve input run limit"));
        }
        for run in &input.runs {
            let target = run.provider.target();
            let Some(node) = document.nodes().get(&target.node) else {
                return Err(invalid("Preserve input provider is missing"));
            };
            let mut current = &target.node;
            let mut repeats = Vec::new();
            while current != child {
                work = work
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Preserve input ownership work limit"))?;
                let Some(&parent) = parents.get(current) else {
                    return Err(invalid(
                        "Preserve input provider is outside its owned subtree",
                    ));
                };
                if parent == owner {
                    return Err(invalid(
                        "Preserve input provider is outside its owned subtree",
                    ));
                }
                if matches!(document.nodes()[parent].kind, NodeKind::Repeat { .. }) {
                    repeats.push((parent, current));
                }
                current = parent;
            }
            repeats.reverse();
            if repeats.len() != target.repeats.len() {
                return Err(invalid("Preserve input relative Repeat path is incomplete"));
            }
            for ((repeat, branch), argument) in repeats.into_iter().zip(&target.repeats) {
                let NodeKind::Repeat {
                    child: default,
                    iterations,
                    ..
                } = &document.nodes()[repeat].kind
                else {
                    unreachable!()
                };
                let selected = document
                    .overrides()
                    .get(repeat)
                    .and_then(|entries| entries.get(&argument.iteration))
                    .unwrap_or(default);
                let gap = document
                    .gap_overrides()
                    .get(repeat)
                    .and_then(|entries| entries.get(&argument.iteration));
                if &argument.node != repeat
                    || iterations.position(&argument.iteration).is_none()
                    || (selected != branch && gap != Some(branch))
                {
                    return Err(invalid(
                        "Preserve input relative Repeat path selects another branch",
                    ));
                }
            }
            let available = match &run.provider {
                PreserveInputProvider::Current { .. } => {
                    if contains_projection.contains(&target.node) {
                        return Err(invalid(
                            "a nested owned Preserve input must select intrinsic output explicitly",
                        ));
                    }
                    extents[&target.node]
                }
                PreserveInputProvider::Intrinsic { .. } => {
                    if !document
                        .derived_clocks()
                        .get(&target.node)
                        .is_some_and(|clock| clock.preserve_input.is_some())
                    {
                        return Err(invalid(
                            "intrinsic Preserve input provider has no owned projection",
                        ));
                    }
                    let NodeKind::Retime { duration, .. } = node.kind else {
                        unreachable!()
                    };
                    ExactRatio::integer(duration.frames())
                }
            };
            if run.source.end.compare(available).is_gt() {
                return Err(invalid("Preserve input source window exceeds its provider"));
            }
        }
    }
    Ok(())
}
