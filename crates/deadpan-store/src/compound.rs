//! One authored history entry with permanently reserved leaf allocations and
//! sparse immutable capture checkpoints. Step identities are never live heads.

use std::collections::BTreeSet;

use deadpan_core::{
    Command, CommandRequest, EditTransaction, ProjectDocument, RegisterValue, ResolvedStep,
    RevisionId,
};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{CommandPlan, CommitOutcome, ProjectStore, StoreError, registers::RegisterBank};

const MAX_STEPS: usize = 1_024;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_PROCESSED_BYTES: usize = 512 * 1024 * 1024;

pub(crate) struct StepRecord {
    revision: RevisionId,
    document: Option<String>,
}

pub(crate) struct Prepared {
    pub steps: Vec<StepRecord>,
    pub registers: crate::registers::PreparedBank,
    pub writes: bool,
}

/// Only tiny derived mappings live for this one preview, commit or replay.
/// The existing Compound step bound also bounds the number of cache entries.
#[derive(Default)]
struct OriginalPasteMappings {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    entries: Vec<OriginalPasteMapping>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct OriginalPasteMapping {
    value: RegisterValue,
    asset: Option<deadpan_core::AssetRecord>,
    frame_rate: deadpan_core::FrameRate,
    source: deadpan_core::SourceNode,
}

#[derive(Debug, Serialize)]
pub struct CompoundCommitOutcome {
    pub committed: Option<CommitOutcome>,
    pub register_bank: RegisterBank,
}

#[derive(Debug, Serialize)]
pub struct CompoundPreview {
    pub edit: Option<EditTransaction>,
    pub register_bank: RegisterBank,
}

impl ProjectStore {
    /// Prepare authored edits and/or history-neutral register writes together.
    pub fn preview_compound(
        &self,
        request: &CommandRequest,
    ) -> Result<CompoundPreview, StoreError> {
        require_compound(request)?;
        let transaction = self.connection.unchecked_transaction()?;
        let plan = crate::prepare_command(&transaction, request)?;
        let prepared = plan.compound.as_ref().expect("compound plan");
        Ok(CompoundPreview {
            edit: (!prepared.steps.is_empty()).then_some(plan.edit),
            register_bank: prepared.registers.bank.clone(),
        })
    }

    /// A register-only transaction preserves the timeline, redo and relevance.
    pub fn commit_compound(
        &mut self,
        request: &CommandRequest,
        relevance: Option<&crate::generation::RelevancePlan>,
    ) -> Result<CompoundCommitOutcome, StoreError> {
        self.require_writer()?;
        require_compound(request)?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let plan = crate::prepare_command(&transaction, request)?;
        let prepared = plan.compound.as_ref().expect("compound plan");
        let register_bank = prepared.registers.bank.clone();
        let committed = if prepared.steps.is_empty() {
            if prepared.writes {
                crate::registers::write_prepared_bank(
                    &transaction,
                    &prepared.registers,
                    plan.current.project_id(),
                )?;
            }
            None
        } else {
            Some(crate::write_command_plan(&transaction, plan, relevance)?)
        };
        transaction.commit()?;
        Ok(CompoundCommitOutcome {
            committed,
            register_bank,
        })
    }
}

fn require_compound(request: &CommandRequest) -> Result<(), StoreError> {
    if matches!(request.command, Command::Compound { .. }) {
        Ok(())
    } else {
        Err(invalid("compound entrypoint requires a Compound command"))
    }
}

pub(crate) fn require_authored(plan: &CommandPlan) -> Result<(), StoreError> {
    if plan
        .compound
        .as_ref()
        .is_some_and(|prepared| prepared.steps.is_empty())
    {
        return Err(invalid(
            "register-only compound requires preview_compound or commit_compound",
        ));
    }
    Ok(())
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE transaction_steps (
            owner_revision TEXT NOT NULL REFERENCES revisions(id),
            ordinal INTEGER NOT NULL CHECK(ordinal>=0 AND ordinal<1024),
            step_revision TEXT NOT NULL UNIQUE,
            document TEXT CHECK(document IS NULL OR json_valid(document)),
            PRIMARY KEY(owner_revision,ordinal)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let malformed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM transaction_steps WHERE
            typeof(owner_revision)!='text' OR length(CAST(owner_revision AS BLOB)) NOT BETWEEN 1 AND ?1
            OR typeof(step_revision)!='text' OR length(CAST(step_revision AS BLOB)) NOT BETWEEN 1 AND ?1
            OR typeof(ordinal)!='integer' OR ordinal<0 OR ordinal>=?2
            OR (document IS NOT NULL AND (typeof(document)!='text' OR length(CAST(document AS BLOB))>?3)))",
        params![deadpan_core::MAX_IDENTITY_BYTES as i64, MAX_STEPS as i64, MAX_CHECKPOINT_BYTES as i64], |row| row.get(0),
    )?;
    if malformed {
        return Err(invalid("invalid or oversized transaction step"));
    }
    let inconsistent: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM transaction_steps GROUP BY owner_revision
            HAVING count(*)>?1 OR min(ordinal)!=0 OR max(ordinal)!=count(*)-1
            OR coalesce(sum(length(CAST(document AS BLOB))),0)>?2)
        OR EXISTS(SELECT 1 FROM transaction_steps GROUP BY step_revision HAVING count(*)>1)
        OR EXISTS(SELECT 1 FROM transaction_steps GROUP BY owner_revision,ordinal HAVING count(*)>1)
        OR EXISTS(SELECT 1 FROM transaction_steps s WHERE NOT EXISTS(SELECT 1 FROM revisions r WHERE r.id=s.owner_revision AND r.kind='edit'))
        OR EXISTS(SELECT 1 FROM transaction_steps s JOIN revisions r ON r.id=s.step_revision)
",
        params![MAX_STEPS as i64, MAX_CHECKPOINT_BYTES as i64], |row| row.get(0),
    )?;
    if inconsistent {
        return Err(invalid(
            "transaction step reservations or checkpoint bounds disagree",
        ));
    }
    Ok(())
}

pub(crate) fn validate_namespace(
    connection: &Connection,
    initial: &ProjectDocument,
) -> Result<(), StoreError> {
    check_stored_sizes(connection)?;
    let allocations: BTreeSet<_> = initial
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            deadpan_core::NodeKind::Repeat { iterations, .. } => Some(iterations),
            _ => None,
        })
        .flat_map(|iterations| iterations.segments().map(|(id, _, _)| id.clone()))
        .chain(
            initial
                .audio_lineage()
                .values()
                .map(|lineage| lineage.allocation.clone()),
        )
        .chain(
            initial
                .audio_bindings()
                .allocation_ids()
                .into_iter()
                .cloned(),
        )
        .collect();
    for allocation in allocations {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM transaction_steps WHERE step_revision=?1)",
            [allocation.as_str()],
            |row| row.get(0),
        )?;
        if exists {
            return Err(invalid("transaction step reuses an initial allocation"));
        }
    }
    Ok(())
}

pub(crate) fn capture_columns(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<(Option<String>, Option<String>), StoreError> {
    let timeline: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM revisions WHERE id=?1)",
        [revision.as_str()],
        |row| row.get(0),
    )?;
    let step: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM transaction_steps WHERE step_revision=?1 AND document IS NOT NULL)", [revision.as_str()], |row| row.get(0))?;
    match (timeline, step) {
        (true, false) => Ok((Some(revision.as_str().into()), None)),
        (false, true) => Ok((None, Some(revision.as_str().into()))),
        _ => Err(invalid("capture identity is missing or ambiguous")),
    }
}

pub(crate) fn read_capture(
    connection: &Connection,
    revision: &RevisionId,
) -> Result<ProjectDocument, StoreError> {
    read_capture_before(connection, revision, None)
}

fn read_capture_before(
    connection: &Connection,
    revision: &RevisionId,
    admitted: Option<&BTreeSet<RevisionId>>,
) -> Result<ProjectDocument, StoreError> {
    let (timeline, step) = capture_columns(connection, revision)?;
    let owner = match &step {
        Some(step) => connection.query_row(
            "SELECT owner_revision FROM transaction_steps WHERE step_revision=?1",
            [step],
            |row| row.get::<_, String>(0),
        )?,
        None => timeline.as_ref().expect("capture source").clone(),
    };
    let owner = RevisionId::new(owner)?;
    if admitted.is_some_and(|admitted| !admitted.contains(&owner)) {
        return Err(invalid(
            "capture must name an earlier committed state admitted by history replay",
        ));
    }

    if timeline.is_some() {
        return Ok(crate::validation::read_revision(connection, revision.as_str())?.document);
    }
    let json: Option<String> = connection.query_row(
        "SELECT CASE WHEN typeof(s.document)='text' AND length(CAST(s.document AS BLOB))<=?2 THEN s.document END
        FROM transaction_steps s JOIN revisions r ON r.id=s.owner_revision AND r.kind='edit' WHERE s.step_revision=?1",
        params![revision.as_str(), MAX_CHECKPOINT_BYTES as i64], |row| row.get(0),
    )?;
    let document = ProjectDocument::from_json(
        &json.ok_or_else(|| invalid("capture checkpoint is missing or oversized"))?,
    )?;
    if document.revision_id() != revision {
        return Err(invalid("capture checkpoint revision disagrees"));
    }
    Ok(document)
}

pub(crate) fn prepare(
    connection: &Connection,
    current: ProjectDocument,
    request: &CommandRequest,
) -> Result<CommandPlan, StoreError> {
    let Command::Compound { transaction } = &request.command else {
        return Err(invalid("expected compound"));
    };
    let bank = crate::registers::prepare_bank(connection)?;
    if transaction.expected_bank_version() != bank.bank.version {
        return Err(invalid("register bank version changed"));
    }
    for (name, expected) in transaction.inputs() {
        if bank.bank.entries.get(name) != expected.as_ref() {
            return Err(invalid("frozen register input differs from current bank"));
        }
    }
    let allocations: Vec<_> = std::iter::once(&request.new_revision)
        .chain(
            transaction
                .steps()
                .iter()
                .filter_map(ResolvedStep::edit)
                .map(|edit| &edit.new_revision),
        )
        .collect();
    crate::ensure_unused_revisions(connection, &allocations)?;
    let (outcome, steps) = execute(connection, &current, request, None)?;
    let writes = !outcome.register_writes.is_empty();
    let registers = crate::registers::prepare_writes_from(bank, &outcome.register_writes)?;
    crate::command_plan(
        current,
        outcome.document,
        outcome.edit,
        request,
        Some(Prepared {
            steps,
            registers,
            writes,
        }),
    )
}

fn execute(
    connection: &Connection,
    current: &ProjectDocument,
    request: &CommandRequest,
    admitted: Option<&BTreeSet<RevisionId>>,
) -> Result<(deadpan_core::CompoundOutcome, Vec<StepRecord>), StoreError> {
    let Command::Compound { transaction } = &request.command else {
        return Err(invalid("expected compound"));
    };
    for value in transaction.inputs().values().flatten() {
        if let Some(revision) = value.capture_revision() {
            let captured = read_capture_before(connection, revision, admitted)?;
            crate::registers::validate_value_at(
                connection,
                value,
                current.project_id(),
                &captured,
            )?;
        } else {
            crate::registers::validate_value(connection, value, current.project_id())?;
        }
    }
    let mut capture_states = BTreeSet::new();
    for step in transaction.steps() {
        match step {
            ResolvedStep::Yank { value, .. } => {
                let revision = value
                    .capture_revision()
                    .ok_or_else(|| invalid("macros cannot be captured by Yank"))?;
                capture_states.insert(revision.clone());
            }
            ResolvedStep::Cut { slice, .. } => {
                capture_states.insert(slice.revision_id().clone());
            }
            _ => {}
        }
        if let Some(value) = step
            .edit()
            .and_then(|edit| slice(edit.command.as_command()))
        {
            capture_states.insert(value.revision_id().clone());
        }
    }
    let mut steps: Vec<StepRecord> = Vec::new();
    let mut checkpoint_bytes = 0_usize;
    let mut processed_bytes = current.to_json()?.len();
    let mut original_pastes = OriginalPasteMappings::default();
    let outcome =
        deadpan_core::replay_compound(current, request, |visit| -> Result<(), StoreError> {
            if matches!(
                visit.step,
                ResolvedStep::Yank { .. } | ResolvedStep::Cut { .. }
            ) {
                let value = visit
                    .value
                    .ok_or_else(|| invalid("capture step has no value"))?;
                crate::registers::validate_value_at(
                    connection,
                    value,
                    current.project_id(),
                    visit.before,
                )?;
                if visit.before.revision_id() != current.revision_id()
                    && !steps.iter().any(|step| {
                        &step.revision == visit.before.revision_id() && step.document.is_some()
                    })
                {
                    return Err(invalid("capture does not name an already reached state"));
                }
            }

            if let Some(leaf) = visit.request {
                if steps.iter().any(|step| step.revision == leaf.new_revision)
                    || leaf.new_revision == request.new_revision
                {
                    return Err(invalid("compound leaf allocation is reused"));
                }
                let captured = match slice(&leaf.command) {
                    Some(slice) => {
                        let document =
                            if slice.revision_id() == visit.before.revision_id() {
                                visit.before.clone()
                            } else if let Some(step) = steps
                                .iter()
                                .find(|step| &step.revision == slice.revision_id())
                            {
                                ProjectDocument::from_json(step.document.as_deref().ok_or_else(
                                    || invalid("local slice has no preceding capture"),
                                )?)?
                            } else {
                                read_capture_before(connection, slice.revision_id(), admitted)?
                            };
                        slice.validate_capture(&document)?;
                        Some(document)
                    }
                    None => None,
                };
                crate::validate_transition(
                    connection,
                    visit.before,
                    visit.after,
                    leaf,
                    crate::Admission::default(),
                    captured.as_ref(),
                )?;
                if let ResolvedStep::Paste { .. } = visit.step {
                    validate_original_paste(
                        connection,
                        visit.before,
                        leaf,
                        visit
                            .value
                            .ok_or_else(|| invalid("paste has no register value"))?,
                        &mut original_pastes,
                    )?;
                }
                let json = visit.after.to_json()?;
                processed_bytes = processed_bytes
                    .checked_add(json.len())
                    .filter(|bytes| *bytes <= MAX_PROCESSED_BYTES)
                    .ok_or_else(|| invalid("compound staged document work exceeds 512 MiB"))?;
                if steps.len() >= MAX_STEPS {
                    return Err(invalid("compound step limit"));
                }
                let document = if capture_states.contains(&leaf.new_revision) {
                    checkpoint_bytes = checkpoint_bytes
                        .checked_add(json.len())
                        .filter(|bytes| *bytes <= MAX_CHECKPOINT_BYTES)
                        .ok_or_else(|| invalid("compound capture checkpoints exceed 64 MiB"))?;
                    Some(json)
                } else {
                    None
                };
                steps.push(StepRecord {
                    revision: leaf.new_revision.clone(),
                    document,
                });
            }
            Ok(())
        })?;
    crate::check_document_size(&outcome.document.to_json()?)?;
    Ok((outcome, steps))
}

fn slice(command: &Command) -> Option<&deadpan_core::CapturedEditSlice> {
    match command {
        Command::SpliceSlice { slice, .. }
        | Command::SpliceSliceAt { slice, .. }
        | Command::ReplaceSlice { slice, .. } => Some(slice),
        Command::ReplaceSliceChildren { slice, .. } => Some(slice),
        _ => None,
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn validate_original_paste(
    connection: &Connection,
    current: &ProjectDocument,
    request: &CommandRequest,
    value: &RegisterValue,
    mappings: &mut OriginalPasteMappings,
) -> Result<(), StoreError> {
    let RegisterValue::Original {
        asset,
        qualification,
        ordinals,
        ..
    } = value
    else {
        return Ok(());
    };
    let record = current.assets().get(asset);
    let frame_rate = current.presentation_basis().frame_rate;
    let cached = mappings.entries.iter().position(|mapping| {
        &mapping.value == value
            && mapping.asset.as_ref() == record
            && mapping.frame_rate == frame_rate
    });
    let index = match cached {
        Some(index) => index,
        None => {
            let receipt = crate::source_registration::read_receipt(connection, qualification)?
                .ok_or_else(|| invalid("Original paste qualification is missing"))?;
            let video = receipt
                .snapshot()
                .video()
                .ok_or_else(|| invalid("Original paste has no picture index"))?;
            let source = deadpan_media::source_import_timing::derive_source_moment(
                video.index(),
                receipt.snapshot().audio(),
                ordinals.clone(),
                frame_rate,
            )
            .map_err(deadpan_media::source_qualification::SourceQualificationError::from)?
            .source_node(asset.clone());
            mappings.entries.push(OriginalPasteMapping {
                value: value.clone(),
                asset: record.cloned(),
                frame_rate,
                source,
            });
            mappings.entries.len() - 1
        }
    };
    let expected = &mappings.entries[index].source;
    let source = match &request.command {
        Command::SpliceSource { source, .. }
        | Command::SpliceSourceAt { source, .. }
        | Command::ReplaceSource { source, .. } => source,
        Command::ReplaceSourceChildren { source, .. } => source,
        _ => return Err(invalid("Original register requires an Original placement")),
    };
    if source != expected {
        return Err(invalid(
            "Original paste differs from its qualified register ordinals",
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn validate_original_paste(
    _: &Connection,
    _: &ProjectDocument,
    _: &CommandRequest,
    value: &RegisterValue,
    _: &mut OriginalPasteMappings,
) -> Result<(), StoreError> {
    if matches!(value, RegisterValue::Original { .. }) {
        Err(StoreError::SourceAdmissionUnavailable)
    } else {
        Ok(())
    }
}

pub(crate) fn write_steps(
    connection: &Connection,
    owner: &RevisionId,
    steps: &[StepRecord],
) -> Result<(), StoreError> {
    for (ordinal, step) in steps.iter().enumerate() {
        connection.execute("INSERT INTO transaction_steps(owner_revision,ordinal,step_revision,document) VALUES(?1,?2,?3,?4)",
            params![owner.as_str(), ordinal as i64, step.revision.as_str(), step.document])?;
    }
    Ok(())
}

pub(crate) fn replay(
    connection: &Connection,
    current: &ProjectDocument,
    request: &CommandRequest,
    admitted: &BTreeSet<RevisionId>,
) -> Result<EditTransaction, StoreError> {
    let (outcome, expected) = execute(connection, current, request, Some(admitted))?;
    if expected.is_empty() {
        return Err(invalid("history contains a register-only compound"));
    }
    let mut statement = connection.prepare("SELECT ordinal,step_revision,document FROM transaction_steps WHERE owner_revision=?1 ORDER BY ordinal")?;
    let mut rows = statement.query([request.new_revision.as_str()])?;
    for (ordinal, step) in expected.iter().enumerate() {
        let row = rows
            .next()?
            .ok_or_else(|| invalid("compound step reservation is missing"))?;
        let found: (i64, String, Option<String>) = (row.get(0)?, row.get(1)?, row.get(2)?);
        if found.0 != ordinal as i64
            || found.1 != step.revision.as_str()
            || found.2 != step.document
        {
            return Err(invalid(
                "compound checkpoint or reservation differs from replay",
            ));
        }
    }
    if rows.next()?.is_some() {
        return Err(invalid("compound has extra step reservations"));
    }
    Ok(outcome.edit)
}

pub(crate) fn validate_ordinary_history(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
    admitted: &BTreeSet<RevisionId>,
) -> Result<(), StoreError> {
    let extras: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM transaction_steps WHERE owner_revision=?1)",
        [request.new_revision.as_str()],
        |row| row.get(0),
    )?;
    if extras {
        return Err(invalid("ordinary edit owns compound step reservations"));
    }
    if let Some(slice) = slice(&request.command) {
        let captured = read_capture_before(connection, slice.revision_id(), Some(admitted))?;
        slice.validate_capture(&captured)?;
        crate::ensure_source_admission(Some(current), next, None, Some(&captured))?;
        crate::ensure_generated_admission_with(Some(current), next, None, Some(&captured))?;
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::Integrity(message.into())
}

#[cfg(test)]
mod tests;
