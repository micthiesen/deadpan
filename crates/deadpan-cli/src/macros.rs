//! Explicit, bounded semantic macro requests shared by the CLI and native writer.

use std::{fs::File, io::Read, num::NonZeroU32, path::Path, sync::Arc};

use deadpan_core::{
    AssetRecord, EditError, EditErrorCode, FrameRate, MarkId, NodeId, OccurrenceIdentities,
    ProjectDocument, ProjectFrame, ProjectId, RegisterName, RegisterValue, RevisionId,
    SemanticAllocation, SemanticAllocationRequest, SemanticContext, SemanticInstruction,
    SemanticPlan, SemanticProgram, SemanticRegisterBank, SemanticVisualSelection,
    SlicePasteIdentities, SourceNode, SplitIdentities,
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
        #[serde(default)]
        selected_child: Option<NodeId>,
        #[serde(default)]
        visual_selection: Option<SemanticVisualSelection>,
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
            selected_child,
            visual_selection,
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
            let plan = plan_program(
                store,
                &document,
                &bank,
                &SemanticContext {
                    parent: parent.clone(),
                    cursor: *cursor,
                    selected_child: selected_child.clone(),
                    visual_selection: visual_selection.clone(),
                },
                &program,
                revision,
            )?;
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

/// Shared host preparation for named calls and single recorded actions. The
/// core resolves each step against its staged document; this host supplies
/// fresh identities and measured Original mappings, never authored writes.
pub fn plan_program(
    store: &ProjectStore,
    document: &ProjectDocument,
    bank: &RegisterBank,
    context: &SemanticContext,
    program: &SemanticProgram,
    new_revision: RevisionId,
) -> Result<SemanticPlan, LiveError> {
    let mut source_error = None;
    // Keep only derived mappings, not large qualification indexes. Repeated
    // pastes of one frozen Original value need one receipt read in this plan;
    // store preview and commit each perform their own independent admission.
    let mut originals: Vec<(RegisterValue, AssetRecord, FrameRate, SourceNode)> = Vec::new();
    // Loaded only when an instruction needs words.
    let mut speech: Option<Result<crate::speech::StoredSpeech, EditError>> = None;
    // Measured picture indexes for pause pictures, loaded once per asset.
    let mut indexes: Vec<(deadpan_core::AssetId, Arc<deadpan_core::SourceFrameIndex>)> = Vec::new();
    let result = deadpan_core::plan_semantic_with_speech(
        document,
        context,
        program,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        new_revision,
        allocate,
        |staged, value| {
            let record = match value {
                RegisterValue::Original { asset, .. } => staged.assets().get(asset),
                _ => None,
            };
            let rate = staged.presentation_basis().frame_rate;
            if let Some((_, _, _, source)) = originals.iter().find(|(previous, asset, fps, _)| {
                previous == value && Some(asset) == record && *fps == rate
            }) {
                return Ok(source.clone());
            }
            let source = original_source(store, staged, value).map_err(|error| {
                let message = error.to_string();
                source_error = Some(error);
                EditError {
                    code: EditErrorCode::InvalidCommand,
                    message,
                    current_revision: None,
                }
            })?;
            if let Some(record) = record {
                originals.push((value.clone(), record.clone(), rate, source.clone()));
            }
            Ok(source)
        },
        |staged| {
            speech
                .get_or_insert_with(|| crate::speech::StoredSpeech::load(store))
                .as_ref()
                .map_err(Clone::clone)?
                .project(staged)
        },
        |staged, site| {
            let plan = deadpan_plan::RenderPlan::compile(staged)
                .map_err(|error| pause_error(&error.to_string()))?;
            crate::pause::site_provider(staged, &plan, &site, &mut |asset| {
                if let Some((_, index)) = indexes.iter().find(|(known, _)| known == asset) {
                    return Ok(Arc::clone(index));
                }
                let index = Arc::new(
                    store
                        .source_video_index(document.revision_id(), asset)
                        .map_err(|error| error.to_string())?,
                );
                indexes.push((asset.clone(), Arc::clone(&index)));
                Ok(index)
            })
            .map_err(|error| pause_error(&error))
        },
    );
    result.map_err(|error| source_error.unwrap_or_else(|| edit_error(error)))
}

fn pause_error(message: &str) -> EditError {
    EditError {
        code: EditErrorCode::SelectionUnavailable,
        message: format!("the pause picture could not be resolved: {message}"),
        current_revision: None,
    }
}

fn allocate(request: SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    let nodes = |count| {
        (0..count)
            .map(|_| NodeId::new(uuid::Uuid::new_v4().to_string()))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(match request {
        SemanticAllocationRequest::Group {
            required_split_ids, ..
        } => SemanticAllocation::Group {
            new_revision: crate::new_revision()?,
            identities: deadpan_core::GroupSelectionIdentities {
                group: NodeId::new(uuid::Uuid::new_v4().to_string())?,
                split: SplitIdentities {
                    nodes: nodes(required_split_ids)?,
                },
            },
        },
        SemanticAllocationRequest::Ungroup { .. } => SemanticAllocation::Ungroup {
            new_revision: crate::new_revision()?,
        },
        SemanticAllocationRequest::SetRepeatPlays { .. } => SemanticAllocation::SetRepeatPlays {
            new_revision: crate::new_revision()?,
        },
        SemanticAllocationRequest::SetRepeatGaps { branches, .. } => {
            SemanticAllocation::SetRepeatGaps {
                new_revision: crate::new_revision()?,
                nodes: nodes(branches)?,
            }
        }
        SemanticAllocationRequest::ParameterEdit { .. } => SemanticAllocation::ParameterEdit {
            new_revision: crate::new_revision()?,
        },
        SemanticAllocationRequest::Sound { .. } => SemanticAllocation::Sound {
            new_revision: crate::new_revision()?,
            id: deadpan_core::SoundId::new(uuid::Uuid::new_v4().to_string())?,
        },
        SemanticAllocationRequest::Roll { needs_wrapper, .. } => SemanticAllocation::Roll {
            new_revision: crate::new_revision()?,
            wrapper: needs_wrapper
                .then(|| NodeId::new(uuid::Uuid::new_v4().to_string()))
                .transpose()?,
        },
        SemanticAllocationRequest::InsertPause {
            required_split_ids, ..
        } => SemanticAllocation::InsertPause {
            new_revision: crate::new_revision()?,
            id: NodeId::new(uuid::Uuid::new_v4().to_string())?,
            split: SplitIdentities {
                nodes: nodes(required_split_ids)?,
            },
        },
        SemanticAllocationRequest::Repeat {
            required_split_ids,
            needs_group,
            ..
        } => SemanticAllocation::Repeat {
            new_revision: crate::new_revision()?,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: NodeId::new(uuid::Uuid::new_v4().to_string())?,
                group: needs_group
                    .then(|| NodeId::new(uuid::Uuid::new_v4().to_string()))
                    .transpose()?,
                split: SplitIdentities {
                    nodes: nodes(required_split_ids)?,
                },
            },
        },
        SemanticAllocationRequest::Cut {
            required_split_ids, ..
        } => SemanticAllocation::Cut {
            new_revision: crate::new_revision()?,
            capture_revision: crate::new_revision()?,
            split_identities: SplitIdentities {
                nodes: nodes(required_split_ids)?,
            },
        },
        SemanticAllocationRequest::Yank { .. } => SemanticAllocation::Yank {
            capture_revision: crate::new_revision()?,
        },
        SemanticAllocationRequest::PasteEdited {
            requirements,
            required_split_ids,
            ..
        } => SemanticAllocation::PasteEdited {
            new_revision: crate::new_revision()?,
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: nodes(requirements.nodes)?,
                    marks: (0..requirements.marks)
                        .map(|_| MarkId::new(uuid::Uuid::new_v4().to_string()))
                        .collect::<Result<_, _>>()?,
                },
                aliases: nodes(requirements.aliases)?,
            },
            split_identities: SplitIdentities {
                nodes: nodes(required_split_ids)?,
            },
        },
        SemanticAllocationRequest::PasteOriginal {
            required_split_ids, ..
        } => SemanticAllocation::PasteOriginal {
            new_revision: crate::new_revision()?,
            node: NodeId::new(uuid::Uuid::new_v4().to_string())?,
            split_identities: SplitIdentities {
                nodes: nodes(required_split_ids)?,
            },
        },
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn original_source(
    store: &ProjectStore,
    document: &ProjectDocument,
    value: &RegisterValue,
) -> Result<SourceNode, LiveError> {
    let RegisterValue::Original {
        asset,
        qualification,
        ordinals,
        ..
    } = value
    else {
        return Err(LiveError::new(
            "RegisterInvalid",
            "Expected an Original register",
        ));
    };
    let record = document.assets().get(asset).ok_or_else(|| {
        LiveError::new(
            "RegisterInvalid",
            "Original asset is absent from the staged edit",
        )
    })?;
    let receipt = store
        .source_qualification(qualification)
        .map_err(LiveError::store)?;
    if receipt
        .asset_record(record.label.clone())
        .map_err(LiveError::store)?
        != *record
    {
        return Err(LiveError::new(
            "RegisterInvalid",
            "Original differs from its saved qualification",
        ));
    }
    let video = receipt.snapshot().video().ok_or_else(|| {
        LiveError::new("RegisterInvalid", "Original has no qualified picture index")
    })?;
    deadpan_media::source_import_timing::derive_source_moment(
        video.index(),
        receipt.snapshot().audio(),
        ordinals.clone(),
        document.presentation_basis().frame_rate,
    )
    .map(|moment| moment.source_node(asset.clone()))
    .map_err(|error| LiveError::new("SourceRegistration", error))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn original_source(
    _: &ProjectStore,
    _: &ProjectDocument,
    _: &RegisterValue,
) -> Result<SourceNode, LiveError> {
    Err(LiveError::new(
        "SourceAdmissionUnavailable",
        "Original paste is unavailable on this platform",
    ))
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
                let registers =
                    (result.register_bank.version != prepared.bank.version).then(|| {
                        RegisterReceipt {
                            project_id: prepared.request.project_id.clone(),
                            revision_id: revision
                                .clone()
                                .unwrap_or_else(|| prepared.request.expected_revision.clone()),
                            bank_version: result.register_bank.version,
                        }
                    });
                (revision, registers)
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
                    // Final plan differences, including retained fragments and
                    // unresolved reasons, not per-instruction loss claims.
                    let mark_ids: std::collections::BTreeSet<_> = self
                        .document
                        .marks()
                        .keys()
                        .chain(plan.document.marks().keys())
                        .collect();
                    output["mark_changes"] = Value::Array(
                        mark_ids
                            .into_iter()
                            .filter_map(|id| {
                                let before = self.document.marks().get(id);
                                let after = plan.document.marks().get(id);
                                if before == after {
                                    None
                                } else {
                                    Some(json!({"id":id,"before":before,"after":after}))
                                }
                            })
                            .collect(),
                    );
                    output["context"] = json!({"parent":plan.context.parent,"cursor":plan.context.cursor,
                        "selected_child":plan.selected_child,
                        "visual_selection":plan.context.visual_selection});
                    output["trace"] = Value::Array(plan.trace.iter().map(|row| json!({
                        "instruction":row.instruction,"before_revision":row.before_revision,
                        "before_scope":row.before_scope,"parent":row.before.parent,
                        "after_parent":row.after.parent,
                        "resolved_parent":row.resolved_parent,"capture":row.capture,
                        "before_cursor":row.before.cursor,"after_cursor":row.after.cursor,
                        "before_selected_child":row.before.selected_child,
                        "after_selected_child":row.after.selected_child,
                        "before_visual_selection":row.before.visual_selection,
                        "after_visual_selection":row.after.visual_selection,
                        "captured_child_label":row.captured_child_label,
                        "resolved_selection":row.resolved_selection,
                        "resolved_range":row.resolved_range,"removed_range":row.removed_range,"depth":row.depth,
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
