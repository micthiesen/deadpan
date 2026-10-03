//! Explicit, bounded semantic macro requests shared by the CLI and native writer.

use std::{fs::File, io::Read, num::NonZeroU32, path::Path, sync::Arc};

use deadpan_core::{
    EditError, NodeId, ProjectDocument, ProjectFrame, ProjectId, RegisterName, RegisterValue,
    RevisionId, SemanticAllocation, SemanticContext, SemanticInstruction, SemanticPlan,
    SemanticProgram, SplitIdentities, plan_semantic,
};
use deadpan_store::{AccessMode, CompoundPreview, ProjectStore, registers::RegisterBank};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::live_project::{LiveError, ShortOperation};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = deadpan_core::MAX_SEMANTIC_PROGRAM_BYTES + 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u32,
    pub project_id: ProjectId,
    pub expected_revision: RevisionId,
    pub expected_bank_version: u64,
    #[serde(default)]
    pub dry_run: bool,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Save {
        register: RegisterName,
        program: Arc<SemanticProgram>,
    },
    Run {
        register: RegisterName,
        parent: NodeId,
        cursor: ProjectFrame,
        count: NonZeroU32,
        #[serde(default)]
        new_revision: Option<RevisionId>,
    },
}

/// A bank-only save has no new authored revision. Its exact durable context is
/// retained independently of the potentially large command output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterReceipt {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub bank_version: u64,
}

#[derive(Debug)]
pub struct Execution {
    pub output: Value,
    pub committed_revision: Option<RevisionId>,
    pub committed_registers: Option<RegisterReceipt>,
}

/// Hosts may prepare runtime register copies from this immutable result before
/// calling `commit`. Preparation never writes the store or reserves identities.
#[derive(Debug)]
pub struct Prepared {
    pub request: Request,
    pub document: ProjectDocument,
    pub bank: RegisterBank,
    pub final_bank: RegisterBank,
    pub plan: Option<SemanticPlan>,
    pub preview: Option<CompoundPreview>,
}

impl Request {
    pub fn from_json(bytes: &[u8]) -> Result<Self, LiveError> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(limit());
        }
        let request: Self = serde_json::from_slice(bytes).map_err(LiveError::json)?;
        request.validate()?;
        Ok(request)
    }

    /// Also called for typed requests and IPC, which cannot bypass public bounds.
    pub fn validate(&self) -> Result<(), LiveError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(LiveError::new(
                "ProtocolUnsupported",
                "Macro protocol must be 1",
            ));
        }
        let register = match &self.operation {
            Operation::Save { register, program } => {
                program.validate().map_err(edit_error)?;
                register
            }
            Operation::Run { register, .. } => register,
        };
        if *register == RegisterName::unnamed() {
            return Err(LiveError::new(
                "InvalidCommand",
                "Macros require a named register a-z",
            ));
        }
        if serde_json::to_vec(self).map_err(LiveError::json)?.len() > MAX_REQUEST_BYTES {
            return Err(limit());
        }
        Ok(())
    }
}

fn limit() -> LiveError {
    LiveError::new(
        "LimitExceeded",
        "Macro request exceeds the bounded program and envelope size",
    )
}

fn edit_error(error: EditError) -> LiveError {
    LiveError {
        current_revision: error.current_revision.clone(),
        ..LiveError::new(error.code.as_str(), error)
    }
}

fn check_context(
    request: &Request,
    document: &ProjectDocument,
    bank: &RegisterBank,
) -> Result<(), LiveError> {
    if document.project_id() != &request.project_id {
        return Err(LiveError::new(
            "HostProjectChanged",
            "The macro names a different project",
        ));
    }
    if document.revision_id() != &request.expected_revision {
        return Err(LiveError {
            current_revision: Some(document.revision_id().clone()),
            ..LiveError::new(
                "RevisionConflict",
                "The macro's expected revision is no longer current",
            )
        });
    }
    if bank.version != request.expected_bank_version {
        return Err(LiveError::new(
            "RegisterInvalid",
            "Register bank version changed",
        ));
    }
    Ok(())
}

pub fn prepare(store: &ProjectStore, request: &Request) -> Result<Prepared, LiveError> {
    request.validate()?;
    let (document, bank) = store.snapshot_with_registers().map_err(LiveError::store)?;
    check_context(request, &document, &bank)?;
    let (final_bank, plan, preview) = match &request.operation {
        Operation::Save { register, program } => {
            let bank = store
                .preview_macro(
                    &request.project_id,
                    &request.expected_revision,
                    request.expected_bank_version,
                    *register,
                    program.clone(),
                )
                .map_err(LiveError::store)?;
            (bank, None, None)
        }
        Operation::Run {
            register,
            parent,
            cursor,
            count,
            new_revision,
        } => {
            let program = SemanticProgram::new(vec![SemanticInstruction::Call {
                register: *register,
                count: *count,
            }])
            .map_err(edit_error)?;
            let revision = new_revision
                .clone()
                .map(Ok)
                .unwrap_or_else(crate::new_revision)
                .map_err(|error| edit_error(error.into()))?;
            let plan = plan_semantic(
                &document,
                &SemanticContext {
                    parent: parent.clone(),
                    cursor: *cursor,
                },
                &program,
                &bank.entries,
                bank.version,
                revision,
                |allocation| {
                    Ok(SemanticAllocation {
                        new_revision: crate::new_revision()?,
                        capture_revision: crate::new_revision()?,
                        split_identities: SplitIdentities {
                            nodes: (0..allocation.required_split_ids)
                                .map(|_| NodeId::new(uuid::Uuid::new_v4().to_string()))
                                .collect::<Result<_, _>>()?,
                        },
                    })
                },
            )
            .map_err(edit_error)?;
            let preview = plan
                .request
                .as_ref()
                .map(|command| store.preview_compound(command))
                .transpose()
                .map_err(LiveError::store)?;
            let final_bank = preview
                .as_ref()
                .map_or_else(|| bank.clone(), |preview| preview.register_bank.clone());
            (final_bank, Some(plan), preview)
        }
    };
    Ok(Prepared {
        request: request.clone(),
        document,
        bank,
        final_bank,
        plan,
        preview,
    })
}

pub fn commit(store: &mut ProjectStore, prepared: &Prepared) -> Result<Execution, LiveError> {
    // Construct the bounded result before any durable action. A later transport
    // or native refresh failure can still retain the independent receipts.
    let mut output = prepared.output();
    if prepared.request.dry_run {
        return Ok(Execution {
            output,
            committed_revision: None,
            committed_registers: None,
        });
    }
    let (committed_revision, committed_registers) = match &prepared.request.operation {
        Operation::Save { register, program } => {
            let bank = store
                .save_macro(
                    &prepared.request.project_id,
                    &prepared.request.expected_revision,
                    prepared.request.expected_bank_version,
                    *register,
                    program.clone(),
                )
                .map_err(LiveError::store)?;
            (
                None,
                Some(RegisterReceipt {
                    project_id: prepared.request.project_id.clone(),
                    revision_id: prepared.request.expected_revision.clone(),
                    bank_version: bank.version,
                }),
            )
        }
        Operation::Run { .. } => {
            if let Some(command) = prepared
                .plan
                .as_ref()
                .and_then(|plan| plan.request.as_ref())
            {
                let result = store
                    .commit_compound(command, None)
                    .map_err(LiveError::store)?;
                let revision = result.committed.map(|outcome| outcome.revision_id);
                let registers = RegisterReceipt {
                    project_id: prepared.request.project_id.clone(),
                    revision_id: revision
                        .clone()
                        .unwrap_or_else(|| prepared.request.expected_revision.clone()),
                    bank_version: result.register_bank.version,
                };
                (revision, Some(registers))
            } else {
                let (document, bank) = store.snapshot_with_registers().map_err(LiveError::store)?;
                check_context(&prepared.request, &document, &bank)?;
                (None, None)
            }
        }
    };
    output["committed"] = json!(committed_revision.is_some() || committed_registers.is_some());
    output["committed_revision"] = json!(committed_revision);
    output["committed_registers"] = json!(committed_registers);
    Ok(Execution {
        output,
        committed_revision,
        committed_registers,
    })
}

impl Prepared {
    fn output(&self) -> Value {
        let mut output = json!({
            "protocol": PROTOCOL_VERSION, "project_id": self.document.project_id(),
            "before_revision": self.document.revision_id(), "before_bank_version": self.bank.version,
            "bank_version": self.final_bank.version, "committed": false,
            "committed_revision": null, "committed_registers": null,
        });
        match &self.request.operation {
            Operation::Save { register, program } => {
                output["operation"] = json!("save");
                output["register"] = json!(register);
                output["instruction_count"] = json!(program.instructions().len());
            }
            Operation::Run { register, .. } => {
                output["operation"] = json!("run");
                output["register"] = json!(register);
                if let Some(plan) = &self.plan {
                    output["context"] = json!({"parent":plan.context.parent,"cursor":plan.context.cursor,
                        "selected_child":plan.selected_child});
                    output["trace"] = Value::Array(plan.trace.iter().map(|row| json!({
                        "instruction":row.instruction,"before_revision":row.before_revision,
                        "before_scope":row.before_scope,"parent":row.before.parent,
                        "before_cursor":row.before.cursor,"after_cursor":row.after.cursor,
                        "resolved_range":row.resolved_range,"depth":row.depth,
                    })).collect());
                    output["register_writes"] =
                        json!(plan.register_writes.keys().collect::<Vec<_>>());
                }
                output["edit"] = self.preview.as_ref().and_then(|preview| preview.edit.as_ref())
                    .map_or(Value::Null, |edit| json!({
                        "new_revision": self.plan.as_ref().and_then(|plan| plan.request.as_ref()).map(|request| &request.new_revision),
                        "duration_delta":edit.duration_delta,"changed_ids":edit.changed_ids,
                        "description":edit.description,
                    }));
            }
        }
        output
    }
}

/// Coherent inspection does not acquire a writer or infer a GUI selection.
pub fn inspect(store: &ProjectStore, register: Option<RegisterName>) -> Result<Value, LiveError> {
    let (document, bank) = store.snapshot_with_registers().map_err(LiveError::store)?;
    if register.is_some_and(|name| !bank.entries.contains_key(&name)) {
        return Err(LiveError::new(
            "SelectionUnavailable",
            "The requested register is empty",
        ));
    }
    let entries: Vec<_> = bank.entries.iter().filter(|(name, _)| register.is_none_or(|selected| selected == **name))
        .map(|(name, value)| match value.as_ref() {
            RegisterValue::Macro { program } => json!({"register":name,"type":"macro",
                "instruction_count":program.instructions().len(),"program":program}),
            RegisterValue::Original { revision, asset, qualification, ordinals } => json!({
                "register":name,"type":"original","capture_revision":revision,
                "asset":asset,"qualification":qualification,"ordinals":ordinals}),
            RegisterValue::Edited { slice } => json!({"register":name,"type":"edited",
                "capture_revision":slice.revision_id(),"parent":slice.parent(),"selection":slice.selection()}),
        }).collect();
    Ok(
        json!({"protocol":PROTOCOL_VERSION,"project_id":document.project_id(),
        "revision_id":document.revision_id(),"bank_version":bank.version,"registers":entries}),
    )
}

pub(super) fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    match arguments {
        ["inspect", package] => {
            let store = ProjectStore::open(Path::new(package), AccessMode::ReadOnly)?;
            crate::write_json(&inspect(&store, None)?)
        }
        ["inspect", package, "--register", name] => {
            let mut chars = name.chars();
            let name = chars.next().filter(|_| chars.next().is_none())
                .ok_or_else(|| LiveError::new("InvalidCommand", "Register must be one character"))?;
            let name = RegisterName::new(name).map_err(edit_error)?;
            let store = ProjectStore::open(Path::new(package), AccessMode::ReadOnly)?;
            crate::write_json(&inspect(&store, Some(name))?)
        }
        [package, "--json", path] => execute_file(Path::new(package), Path::new(path), false),
        [package, "--json", path, "--dry-run"] => execute_file(Path::new(package), Path::new(path), true),
        _ => Err(crate::CliError::Usage("Expected macro inspect <project> [--register a] or macro <project> --json <request> [--dry-run]".into())),
    }
}

fn execute_file(package: &Path, path: &Path, dry_run: bool) -> Result<(), crate::CliError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let mut request = Request::from_json(&bytes)?;
    request.dry_run |= dry_run;
    crate::write_json(&crate::live_project::dispatch_short(
        package,
        Some(request.project_id.clone()),
        ShortOperation::Macro {
            request: Box::new(request),
        },
    )?)
}
