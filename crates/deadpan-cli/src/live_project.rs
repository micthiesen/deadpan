//! Semantic requests executed by the one live project writer. A socket reply is
//! an observation, never a media, worker, publication or writer capability.

use std::path::Path;

use deadpan_core::{CommandRequest, ProjectId, RevisionId};
use deadpan_store::source_registration::PrimaryGeometryAdoption;
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::host::{Client, HostError};
use crate::render::{RenderContext, RenderRequest, RenderStatus, WorkflowTarget};

pub mod generation;
pub mod preparation;
use preparation::{PreparationCommand, PreparationStatus, PreparationTarget};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    #[serde(deserialize_with = "deserialize_empty")]
    Inspect,
    Execute {
        project_id: ProjectId,
        command: Box<ShortOperation>,
    },
    Render {
        request: RenderRequest,
    },
    RenderStatus {
        project_id: ProjectId,
        target: WorkflowTarget,
    },
    ReleaseRenderStatus {
        project_id: ProjectId,
        target: WorkflowTarget,
    },
    Prepare {
        project_id: ProjectId,
        target: PreparationTarget,
        command: Box<PreparationCommand>,
    },
    PreparationStatus {
        project_id: ProjectId,
        target: PreparationTarget,
    },
    CancelPreparation {
        project_id: ProjectId,
        target: PreparationTarget,
    },
    ReleasePreparationStatus {
        project_id: ProjectId,
        target: PreparationTarget,
    },
    /// Start the owner's AI job for one pause.
    Generate {
        project_id: ProjectId,
        request: generation::GenerateRequest,
    },
    GenerationStatus {
        project_id: ProjectId,
        job: u64,
    },
    /// Cooperatively cancel exactly the observed job.
    CancelGeneration {
        project_id: ProjectId,
        job: u64,
    },
    /// The caller has the concluded job's result; the owner may forget it.
    ReleaseGenerationStatus {
        project_id: ProjectId,
        job: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryDirection {
    Undo,
    Redo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShortOperation {
    Macro {
        request: Box<crate::macros::Request>,
    },
    Edit {
        request: Box<CommandRequest>,
        dry_run: bool,
    },
    History {
        direction: HistoryDirection,
        expected_revision: RevisionId,
        new_revision: RevisionId,
        dry_run: bool,
    },
    AdoptPrimaryGeometry {
        adoption: PrimaryGeometryAdoption,
        dry_run: bool,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    Migrate,
    /// Accept a Ready AI pause variant as one undoable edit. With `attempt`,
    /// that variant of the request is selected first. With
    /// `expected_revision`, a different head is refused.
    AcceptHold {
        request: deadpan_jobs::RequestId,
        attempt: Option<deadpan_jobs::AttemptId>,
        expected_revision: Option<RevisionId>,
        new_revision: RevisionId,
    },
}

impl ShortOperation {
    pub fn is_preview(&self) -> bool {
        matches!(self, Self::Macro { request } if request.dry_run)
            || matches!(
                self,
                Self::Edit { dry_run: true, .. }
                    | Self::History { dry_run: true, .. }
                    | Self::AdoptPrimaryGeometry { dry_run: true, .. }
            )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Context {
        context: RenderContext,
        preview_active: bool,
    },
    Completed {
        output: Value,
        committed_revision: Option<RevisionId>,
        #[serde(default)]
        committed_registers: Option<crate::macros::RegisterReceipt>,
        refresh_error: Option<String>,
    },
    Render {
        status: Box<RenderStatus>,
        finished: bool,
    },
    Preparation {
        status: Box<PreparationStatus>,
    },
    Generation {
        status: Box<generation::GenerationStatus>,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    Released,
    Failed {
        error: LiveError,
    },
}

// Serde's internally tagged unit variants otherwise ignore unknown fields.
fn deserialize_empty<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}

    Empty::deserialize(deserializer).map(|_| ())
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
#[serde(deny_unknown_fields)]
pub struct LiveError {
    pub code: String,
    pub message: String,
    pub current_revision: Option<RevisionId>,
    pub committed_revision: Option<RevisionId>,
    #[serde(default)]
    pub committed_registers: Option<Box<crate::macros::RegisterReceipt>>,
}

impl LiveError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
            current_revision: None,
            committed_revision: None,
            committed_registers: None,
        }
    }

    pub fn store(error: StoreError) -> Self {
        let current_revision = match &error {
            StoreError::RevisionConflict { current, .. } => RevisionId::new(current).ok(),
            StoreError::Edit(error) => error.current_revision.clone(),
            _ => None,
        };
        Self {
            current_revision,
            ..Self::new(error.code(), error)
        }
    }

    pub fn json(error: serde_json::Error) -> Self {
        Self::new("HostProtocolInvalid", error)
    }
}

impl From<HostError> for LiveError {
    fn from(error: HostError) -> Self {
        Self::new(&error.code, error.message)
    }
}

impl Request {
    pub fn new(operation: Operation) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            operation,
        }
    }

    pub fn from_value(value: Value) -> Result<Self, LiveError> {
        let request: Self = serde_json::from_value(value).map_err(LiveError::json)?;
        if request.schema_version != SCHEMA_VERSION {
            return Err(LiveError::new(
                "HostProtocolUnsupported",
                "Live project schema_version must be 1",
            ));
        }
        if let Operation::Render { request } = &request.operation {
            // The smaller public Render request bound also applies through IPC.
            let bytes = serde_json::to_vec(request).map_err(LiveError::json)?;
            RenderRequest::from_json(&bytes)
                .map_err(|error| LiveError::new(&error.code, error.message))?;
        }
        match &request.operation {
            Operation::Execute { command, .. } => {
                if let ShortOperation::Macro { request } = command.as_ref() {
                    request.validate()?;
                }
            }
            Operation::Prepare {
                target, command, ..
            } => {
                target.validate()?;
                command.validate()?;
            }
            Operation::PreparationStatus { target, .. }
            | Operation::CancelPreparation { target, .. }
            | Operation::ReleasePreparationStatus { target, .. } => target.validate()?,
            Operation::Generate { request, .. } => request.validate()?,
            _ => {}
        }
        Ok(request)
    }
}

pub fn request(client: &mut Client, operation: Operation) -> Result<Reply, LiveError> {
    let payload = serde_json::to_value(Request::new(operation)).map_err(LiveError::json)?;
    let value = client.request(payload)?;
    let reply: Reply = serde_json::from_value(value).map_err(LiveError::json)?;
    match reply {
        Reply::Failed { error } => Err(error),
        reply => Ok(reply),
    }
}

pub fn inspect(client: &mut Client) -> Result<(RenderContext, bool), LiveError> {
    match request(client, Operation::Inspect)? {
        Reply::Context {
            context,
            preview_active,
        } => Ok((context, preview_active)),
        _ => Err(LiveError::new(
            "HostProtocolInvalid",
            "Host returned an unexpected context reply",
        )),
    }
}

/// The only fallback is obtaining the real writer lock. Once an owner is
/// discovered or a request is sent, connection errors never trigger local replay.
pub fn dispatch_short(
    package: &Path,
    project: Option<ProjectId>,
    operation: ShortOperation,
) -> Result<Value, LiveError> {
    let mode = if operation.is_preview() {
        AccessMode::ReadOnly
    } else {
        AccessMode::ReadWrite
    };
    match ProjectStore::open(package, mode) {
        Ok(mut store) => {
            store.set_generation_context_resolver(std::sync::Arc::new(
                crate::generation_context::BoundaryContextResolver::default(),
            ));
            let project = match project {
                Some(project) => project,
                None => store
                    .snapshot()
                    .map_err(LiveError::store)?
                    .project_id()
                    .clone(),
            };
            Ok(execute_short(&mut store, &project, &operation)?.output)
        }
        Err(StoreError::AlreadyOpen) => {
            let mut client = Client::discover(package)?.ok_or_else(|| {
                LiveError::new(
                    "HostOwnerUnavailable",
                    "The project writer has no available authenticated endpoint",
                )
            })?;
            let project_id = match project {
                Some(project) => project,
                None => inspect(&mut client)?.0.project_id,
            };
            match request(
                &mut client,
                Operation::Execute {
                    project_id,
                    command: Box::new(operation),
                },
            )? {
                Reply::Completed {
                    mut output,
                    committed_revision,
                    committed_registers,
                    refresh_error,
                } => {
                    if refresh_error.is_some() || committed_registers.is_some() {
                        let object = output.as_object_mut().ok_or_else(|| LiveError {
                            committed_revision: committed_revision.clone(),
                            committed_registers: committed_registers.clone().map(Box::new),
                            ..LiveError::new(
                                "HostProtocolInvalid",
                                "Committed command reply is not an object",
                            )
                        })?;
                        object.insert(
                            "committed_revision".into(),
                            serde_json::to_value(committed_revision).map_err(LiveError::json)?,
                        );
                        object.insert(
                            "committed_registers".into(),
                            serde_json::to_value(committed_registers).map_err(LiveError::json)?,
                        );
                        if let Some(error) = refresh_error {
                            object.insert("host_refresh_error".into(), Value::String(error));
                        }
                    }
                    Ok(output)
                }
                _ => Err(LiveError::new(
                    "HostProtocolInvalid",
                    "Host returned an unexpected command reply",
                )),
            }
        }
        Err(error) => Err(LiveError::store(error)),
    }
}

/// Uses existing store transactions and never infers selection from GUI focus.
/// The commit receipt is returned separately so UI refresh cannot hide a commit.
pub fn execute_short(
    store: &mut ProjectStore,
    project: &ProjectId,
    operation: &ShortOperation,
) -> Result<crate::macros::Execution, LiveError> {
    if store.snapshot().map_err(LiveError::store)?.project_id() != project {
        return Err(LiveError::new(
            "HostProjectChanged",
            "The request names a different project",
        ));
    }
    let (output, committed) = match operation {
        ShortOperation::Macro { request } => {
            if &request.project_id != project {
                return Err(LiveError::new(
                    "HostProjectChanged",
                    "Macro and host project identities differ",
                ));
            }
            let prepared = crate::macros::prepare(store, request)?;
            return crate::macros::commit(store, &prepared);
        }
        ShortOperation::Edit { request, dry_run } => {
            if &request.project_id != project {
                return Err(LiveError::new(
                    "HostProjectChanged",
                    "Command and host project identities differ",
                ));
            }
            if *dry_run {
                let output = if matches!(&request.command, deadpan_core::Command::SlipSource { .. })
                {
                    let preview = store
                        .preview_source_slip(request)
                        .map_err(LiveError::store)?;
                    serde_json::json!({"protocol":1,"committed":false,
                        "edit":preview.edit,"source_slip":preview.resolution})
                } else if matches!(&request.command, deadpan_core::Command::TrimSource { .. }) {
                    let preview = store
                        .preview_source_trim(request)
                        .map_err(LiveError::store)?;
                    serde_json::json!({"protocol":1,"committed":false,
                        "edit":preview.edit,"source_trim":preview.resolution})
                } else if matches!(&request.command, deadpan_core::Command::RollSources { .. }) {
                    let preview = store
                        .preview_source_roll(request)
                        .map_err(LiveError::store)?;
                    serde_json::json!({"protocol":1,"committed":false,
                        "edit":preview.edit,"source_roll":preview.resolution})
                } else if matches!(
                    &request.command,
                    deadpan_core::Command::ApplySourceTrim { .. }
                ) {
                    let preview = store
                        .preview_source_trim_edit(request)
                        .map_err(LiveError::store)?;
                    serde_json::json!({"protocol":1,"committed":false,
                        "edit":preview.edit,"source_trim_edit":preview.resolution})
                } else {
                    let edit = store.preview(request).map_err(LiveError::store)?;
                    serde_json::json!({"protocol":1,"committed":false,"edit":edit})
                };
                (output, None)
            } else {
                let outcome = store.commit(request).map_err(LiveError::store)?;
                let revision = outcome.revision_id.clone();
                (
                    serde_json::json!({"protocol":1,"committed":true,"outcome":outcome}),
                    Some(revision),
                )
            }
        }
        ShortOperation::History {
            direction,
            expected_revision,
            new_revision,
            dry_run,
        } => {
            let outcome = match (direction, dry_run) {
                (HistoryDirection::Undo, true) => {
                    store.preview_undo(expected_revision, new_revision.clone())
                }
                (HistoryDirection::Undo, false) => {
                    store.undo(expected_revision, new_revision.clone())
                }
                (HistoryDirection::Redo, true) => {
                    store.preview_redo(expected_revision, new_revision.clone())
                }
                (HistoryDirection::Redo, false) => {
                    store.redo(expected_revision, new_revision.clone())
                }
            }
            .map_err(LiveError::store)?;
            let revision = (!dry_run).then(|| outcome.revision_id.clone());
            (
                serde_json::json!({"protocol":1,"committed":!dry_run,"outcome":outcome}),
                revision,
            )
        }
        ShortOperation::AdoptPrimaryGeometry { adoption, dry_run } => {
            if *dry_run {
                let edit = store
                    .preview_primary_geometry(adoption)
                    .map_err(LiveError::store)?;
                (
                    serde_json::json!({"protocol":1,"committed":false,"edit":edit}),
                    None,
                )
            } else {
                let outcome = store
                    .adopt_primary_geometry(adoption, None)
                    .map_err(LiveError::store)?;
                let revision = outcome.revision_id.clone();
                (
                    serde_json::json!({"protocol":1,"committed":true,"outcome":outcome}),
                    Some(revision),
                )
            }
        }
        ShortOperation::AcceptHold {
            request,
            attempt,
            expected_revision,
            new_revision,
        } => {
            if let Some(expected) = expected_revision {
                let current = store.head_revision().map_err(LiveError::store)?;
                if &current != expected {
                    return Err(LiveError::store(StoreError::RevisionConflict {
                        expected: expected.as_str().into(),
                        current: current.as_str().into(),
                    }));
                }
            }
            if let Some(attempt) = attempt {
                store
                    .select_generation_bundle_variant(&deadpan_jobs::MessageIdentity::new(
                        request.clone(),
                        attempt.clone(),
                    ))
                    .map_err(LiveError::store)?;
            }
            let outcome =
                crate::generation::acceptance::accept(store, request, new_revision.clone())
                    .map_err(|error| LiveError::new(error.code(), &error))?;
            let revision = outcome.revision_id.clone();
            (
                serde_json::json!({"protocol":1,"committed":true,"outcome":outcome}),
                Some(revision),
            )
        }
        ShortOperation::Migrate => {
            // An admitted store is already at the current schema. Never release
            // its writer merely to run the standalone migration entrypoint.
            let schema = deadpan_store::DATABASE_SCHEMA_VERSION;
            (
                serde_json::json!({"protocol":1,"migration":{"from_schema":schema,"to_schema":schema,"backup":null}}),
                None,
            )
        }
    };
    Ok(crate::macros::Execution {
        output,
        committed_revision: committed,
        committed_registers: None,
    })
}
