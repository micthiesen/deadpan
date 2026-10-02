//! Resolved transactions are flat programs over immutable staged documents.
//! This is an execution boundary, not a semantic macro parser or input recorder.

mod identities;
pub(crate) mod wire;

use crate::{
    CapturedEditSlice, Command, CommandRequest, EditError, EditErrorCode, EditTransaction,
    ProjectDocument, RegisterName, RegisterValue, RevisionId,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const MAX_COMPOUND_STEPS: usize = 1024;
pub const MAX_COMPOUND_WIRE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_COMPOUND_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_COMPOUND_DOCUMENT_BYTES: usize = 512 * 1024 * 1024;

/// A command that cannot recursively contain another resolved transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct AtomicCommand(Box<Command>);
impl AtomicCommand {
    pub fn new(command: Command) -> Result<Self, EditError> {
        if matches!(command, Command::Compound { .. }) {
            return Err(invalid("nested compound commands are forbidden"));
        }
        if matches!(command, Command::Delete { .. }) {
            return Err(invalid(
                "resolved transactions require DeleteRipple, not historical Delete",
            ));
        }
        wire::size(&command, MAX_COMPOUND_WIRE_BYTES)?;
        Ok(Self(Box::new(command)))
    }
    pub fn as_command(&self) -> &Command {
        &self.0
    }
}
impl<'de> Deserialize<'de> for AtomicCommand {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = wire::read(deserializer)?;
        if value.get("command").and_then(serde_json::Value::as_str) == Some("compound") {
            return Err(serde::de::Error::custom(
                "nested compound commands are forbidden",
            ));
        }
        let command = Command::deserialize(value).map_err(serde::de::Error::custom)?;
        Self::new(command).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafEdit {
    pub new_revision: RevisionId,
    pub command: AtomicCommand,
}
impl LeafEdit {
    pub fn new(new_revision: RevisionId, command: Command) -> Result<Self, EditError> {
        Ok(Self {
            new_revision,
            command: AtomicCommand::new(command)?,
        })
    }
    /// Derive the only valid ordinary request for this leaf's staged input.
    pub fn request(&self, document: &ProjectDocument) -> CommandRequest {
        CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: self.new_revision.clone(),
            command: self.command.as_command().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedStep {
    Edit {
        edit: LeafEdit,
    },
    Yank {
        name: RegisterName,
        value: Arc<RegisterValue>,
    },
    Cut {
        name: RegisterName,
        slice: Arc<CapturedEditSlice>,
        delete: LeafEdit,
    },
    Paste {
        name: RegisterName,
        edit: LeafEdit,
    },
}
impl ResolvedStep {
    pub fn edit(&self) -> Option<&LeafEdit> {
        match self {
            Self::Edit { edit } | Self::Paste { edit, .. } => Some(edit),
            Self::Cut { delete, .. } => Some(delete),
            Self::Yank { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedTransaction {
    expected_bank_version: u64,
    inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
    steps: Vec<ResolvedStep>,
}
impl ResolvedTransaction {
    pub fn new(
        expected_bank_version: u64,
        inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
        steps: Vec<ResolvedStep>,
    ) -> Result<Self, EditError> {
        let result = Self {
            expected_bank_version,
            inputs,
            steps,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn expected_bank_version(&self) -> u64 {
        self.expected_bank_version
    }
    pub fn inputs(&self) -> &BTreeMap<RegisterName, Option<Arc<RegisterValue>>> {
        &self.inputs
    }
    pub fn steps(&self) -> &[ResolvedStep] {
        &self.steps
    }
    fn validate(&self) -> Result<(), EditError> {
        if self.steps.is_empty() || self.steps.len() > MAX_COMPOUND_STEPS {
            return Err(limit("resolved transaction needs 1..=1024 expanded steps"));
        }
        // Include the tagged command envelope without cloning the staged
        // payload. A constructible transaction must fit its persisted command.
        #[derive(Serialize)]
        #[serde(tag = "command", rename_all = "snake_case")]
        enum Envelope<'a> {
            Compound {
                transaction: &'a ResolvedTransaction,
            },
        }
        wire::size(
            &Envelope::Compound { transaction: self },
            MAX_COMPOUND_WIRE_BYTES,
        )?;
        let mut revisions = BTreeSet::new();
        for step in &self.steps {
            if let Some(edit) = step.edit()
                && !revisions.insert(&edit.new_revision)
            {
                return Err(identity("leaf revision identities must be distinct"));
            }
        }
        Ok(())
    }
}
impl<'de> Deserialize<'de> for ResolvedTransaction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            expected_bank_version: u64,
            #[serde(deserialize_with = "crate::document::unique_map")]
            inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
            steps: Vec<ResolvedStep>,
        }
        let value = wire::read_transaction(deserializer)?;
        let steps = value
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| serde::de::Error::custom("resolved transaction needs a steps array"))?;
        if steps.is_empty() || steps.len() > MAX_COMPOUND_STEPS {
            return Err(serde::de::Error::custom(
                "resolved transaction needs 1..=1024 expanded steps",
            ));
        }
        let decoded = Wire::deserialize(value).map_err(serde::de::Error::custom)?;
        Self::new(decoded.expected_bank_version, decoded.inputs, decoded.steps)
            .map_err(serde::de::Error::custom)
    }
}

/// A successful pure step. Host validation may reject it; no writes are implied.
/// `value` is the precise selected register payload for Yank, Cut and Paste.
/// A Yank has no ordinary request and its before/after references are identical.
pub struct CompoundVisit<'a> {
    pub step: &'a ResolvedStep,
    pub before: &'a ProjectDocument,
    pub after: &'a ProjectDocument,
    pub request: Option<&'a CommandRequest>,
    pub value: Option<&'a RegisterValue>,
}

pub struct CompoundOutcome {
    pub edit: EditTransaction,
    pub document: ProjectDocument,
    /// Includes the unnamed alias of every named write. Empty for edit-only runs.
    pub register_writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
}

/// Validate and stage all steps, visiting the exact ordinary leaf boundaries.
/// The visitor must stage its own side effects until this returns successfully.
/// Frozen input admission and measured Original ordinal mapping remain host work.
/// Copy-only programs are valid; the host may use a history-neutral bank write
/// instead of persisting the returned revision-only aggregate patch.
pub fn replay_compound<E: From<EditError>>(
    document: &ProjectDocument,
    request: &CommandRequest,
    mut visit: impl FnMut(CompoundVisit<'_>) -> Result<(), E>,
) -> Result<CompoundOutcome, E> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::Compound { transaction } = &request.command else {
        return Err(invalid("sequential replay requires a compound command").into());
    };
    transaction.validate()?;
    for edit in transaction.steps.iter().filter_map(ResolvedStep::edit) {
        if &edit.new_revision == document.revision_id() || edit.new_revision == request.new_revision
        {
            return Err(identity("leaf revision must differ from base and outer revisions").into());
        }
    }
    let mut bytes = wire::size(document, crate::MAX_DOCUMENT_JSON_BYTES)?;
    let mut captured_bytes = 0usize;
    let mut captures = BTreeSet::new();
    let mut identities = identities::Ledger::new(document);
    let mut current = document.clone();
    let mut writes = BTreeMap::<RegisterName, Arc<RegisterValue>>::new();
    for step in &transaction.steps {
        let value = match step {
            ResolvedStep::Edit { .. } => None,
            ResolvedStep::Yank { value, .. } => Some(value.clone()),
            ResolvedStep::Cut { slice, .. } => Some(Arc::new(RegisterValue::Edited {
                slice: slice.clone(),
            })),
            ResolvedStep::Paste { name, .. } => Some(
                writes
                    .get(name)
                    .cloned()
                    .or_else(|| transaction.inputs.get(name).and_then(Clone::clone))
                    .ok_or_else(|| {
                        invalid("paste register is absent from frozen inputs and staged writes")
                    })?,
            ),
        };
        match step {
            ResolvedStep::Yank { .. } | ResolvedStep::Cut { .. } => {
                let value = value.as_deref().expect("capture step has a value");
                if captures.insert(current.revision_id().clone()) {
                    let size = wire::size(&current, MAX_COMPOUND_CAPTURE_BYTES)?;
                    charge(
                        &mut captured_bytes,
                        size,
                        MAX_COMPOUND_CAPTURE_BYTES,
                        "aggregate captured document byte limit",
                    )?;
                }
                validate_capture(&current, value)?;
                if let ResolvedStep::Cut { slice, delete, .. } = step {
                    validate_cut(slice, delete.command.as_command())?;
                }
            }
            ResolvedStep::Paste { edit, .. } => validate_paste(
                &current,
                value.as_deref().expect("paste step has a value"),
                edit.command.as_command(),
            )?,
            ResolvedStep::Edit { .. } => {}
        }
        let leaf = step.edit().map(|edit| edit.request(&current));
        let next = if let Some(leaf) = &leaf {
            identities.reserve(&current, &leaf.command)?;
            let size = wire::size(&current, crate::MAX_DOCUMENT_JSON_BYTES)?;
            charge(
                &mut bytes,
                size,
                MAX_COMPOUND_DOCUMENT_BYTES,
                "staged document work byte limit",
            )?;
            let applied = crate::apply(&current, leaf)?;
            let next = applied.forward.apply(&current)?;
            let size = wire::size(&next, crate::MAX_DOCUMENT_JSON_BYTES)?;
            charge(
                &mut bytes,
                size,
                MAX_COMPOUND_DOCUMENT_BYTES,
                "staged document work byte limit",
            )?;
            identities.observe(&current, &next)?;
            Some(next)
        } else {
            None
        };
        visit(CompoundVisit {
            step,
            before: &current,
            after: next.as_ref().unwrap_or(&current),
            request: leaf.as_ref(),
            value: value.as_deref(),
        })?;
        if let ResolvedStep::Yank { name, .. } | ResolvedStep::Cut { name, .. } = step {
            let value = value.expect("capture step has a value");
            writes.insert(*name, value.clone());
            writes.insert(RegisterName::unnamed(), value);
        }
        if let Some(next) = next {
            current = next;
        }
    }
    current.revision_id = request.new_revision.clone();
    let final_size = wire::size(&current, crate::MAX_DOCUMENT_JSON_BYTES)?;
    charge(
        &mut bytes,
        final_size,
        MAX_COMPOUND_DOCUMENT_BYTES,
        "staged document work byte limit",
    )?;
    let edit = crate::command::net_transaction(document, &current, "Apply resolved transaction")?;
    Ok(CompoundOutcome {
        edit,
        document: current,
        register_writes: writes,
    })
}

fn validate_capture(document: &ProjectDocument, value: &RegisterValue) -> Result<(), EditError> {
    match value {
        RegisterValue::Edited { slice } => slice.validate_capture(document),
        RegisterValue::Original {
            revision,
            asset,
            qualification,
            ordinals,
        } => {
            if revision != document.revision_id() {
                return Err(invalid(
                    "Original capture must name the current staged revision",
                ));
            }
            validate_original(document, asset, qualification, ordinals)
        }
    }
}
fn validate_original(
    document: &ProjectDocument,
    asset: &crate::AssetId,
    qualification: &crate::SourceQualificationId,
    ordinals: &std::ops::Range<u64>,
) -> Result<(), EditError> {
    if ordinals.start >= ordinals.end
        || document.assets().get(asset).is_none_or(|record| {
            record.source_qualification.as_ref() != Some(qualification)
                || record.video.is_none()
                || record.still_image
                || record.frame_count.is_some_and(|frames| {
                    u64::try_from(frames.frames()).map_or(true, |count| ordinals.end > count)
                })
        })
    {
        return Err(invalid(
            "Original register needs a nonempty selection and its qualified asset",
        ));
    }
    Ok(())
}
fn validate_cut(slice: &CapturedEditSlice, command: &Command) -> Result<(), EditError> {
    let matches = match (command, slice.selection()) {
        (
            Command::DeleteRange { parent, range, .. },
            crate::SliceCaptureSelection::Range { range: captured },
        ) => parent == slice.parent() && range == captured,
        (
            Command::DeleteRipple { node, .. },
            crate::SliceCaptureSelection::Child { node: captured },
        ) => node == captured,
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(invalid("cut must delete exactly its captured selection"))
    }
}
fn validate_paste(
    document: &ProjectDocument,
    value: &RegisterValue,
    command: &Command,
) -> Result<(), EditError> {
    match (value, command) {
        (
            RegisterValue::Edited { slice: selected },
            Command::SpliceSlice { slice, .. }
            | Command::SpliceSliceAt { slice, .. }
            | Command::ReplaceSlice { slice, .. },
        ) if selected.as_ref() == slice => Ok(()),
        (
            RegisterValue::Original {
                asset,
                qualification,
                ordinals,
                ..
            },
            Command::SpliceSource { source, .. }
            | Command::SpliceSourceAt { source, .. }
            | Command::ReplaceSource { source, .. },
        ) => {
            validate_original(document, asset, qualification, ordinals)?;
            let record = &document.assets()[asset];
            let expected_link = if record.audio.is_some() {
                crate::LinkRelation::Linked
            } else {
                crate::LinkRelation::Independent
            };
            if !matches!(&source.video, crate::SourceVideo::Stream { asset: actual, .. } if actual == asset)
                || source
                    .audio
                    .as_ref()
                    .is_some_and(|audio| &audio.asset != asset)
                || source.audio.is_some() != record.audio.is_some()
                || source.link != expected_link
            {
                return Err(invalid(
                    "Original paste must bind linked source material to its selected asset",
                ));
            }
            Ok(())
        }
        _ => Err(invalid(
            "paste command differs from its selected register value",
        )),
    }
}
fn charge(total: &mut usize, amount: usize, limit: usize, message: &str) -> Result<(), EditError> {
    *total = total
        .checked_add(amount)
        .filter(|value| *value <= limit)
        .ok_or_else(|| self::limit(message))?;
    Ok(())
}
fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn identity(message: &str) -> EditError {
    EditError::new(EditErrorCode::IdentityConflict, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
