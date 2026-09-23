//! Optional native V1 workflow. Generic documents and legacy projects retain
//! their broader editing vocabulary. SQLite pins one measured original and the
//! first full-source edit as an undo floor, independently of later deletions.

use std::{collections::BTreeSet, path::Path, sync::atomic::AtomicBool};

use deadpan_core::{
    AssetId, BasisState, HoldVideo, NodeId, NodeKind, ProjectDocument, RevisionId,
    SourceQualificationId,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::source_registration::{
    PreparedSourceRegistration, SourceInsertionPurpose, SourceInsertionRequest,
    SourceQualificationReceipt, SourceRegistration, SourceRegistrationOutcome,
};
use crate::{ProjectStore, StoreError};

const MAX_PROFILE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SingleSourceState {
    AwaitingSource {
        initial_revision: RevisionId,
    },
    Ready {
        initial_revision: RevisionId,
        asset: AssetId,
        qualification: SourceQualificationId,
        /// Historical initial Source node; edits may wrap or delete it.
        node: NodeId,
        baseline_revision: RevisionId,
    },
}

/// Identity allocation only. Source range, insertion target and project basis
/// are derived from the empty project and the live measured source token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleSourceInitialization {
    pub expected_revision: RevisionId,
    pub new_revision: RevisionId,
    pub new_asset_id: AssetId,
    pub node: NodeId,
    pub label: String,
}

impl ProjectStore {
    pub fn create_single_source(
        path: &Path,
        document: &ProjectDocument,
    ) -> Result<Self, StoreError> {
        check_empty(document)?;
        Self::create_inner(path, document, true)
    }

    pub fn single_source_state(&self) -> Result<Option<SingleSourceState>, StoreError> {
        Ok(read(&self.connection)?.map(|(state, _)| state))
    }

    /// Atomically establishes the complete original and protected baseline.
    /// Failed or cancelled preparation leaves the project awaiting its source.
    pub fn initialize_prepared_source(
        &mut self,
        input: &SingleSourceInitialization,
        source: &PreparedSourceRegistration,
        cancelled: &AtomicBool,
    ) -> Result<SourceRegistrationOutcome, StoreError> {
        let current = self.snapshot()?;
        let registration = SourceRegistration {
            expected_revision: input.expected_revision.clone(),
            new_revision: input.new_revision.clone(),
            original: source.receipt().original().content().clone(),
            new_asset_id: input.new_asset_id.clone(),
            label: input.label.clone(),
            insertion: Some(SourceInsertionRequest {
                parent: current.root().clone(),
                index: 0,
                node: input.node.clone(),
                label: input.label.clone(),
                purpose: SourceInsertionPurpose::Primary,
            }),
        };
        self.register_prepared_source_inner(&registration, source, None, cancelled, true)
    }
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE single_source (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            profile TEXT NOT NULL CHECK(json_valid(profile)),
            baseline_history INTEGER REFERENCES history(id)
        ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn create_profile(
    connection: &Connection,
    document: &ProjectDocument,
) -> Result<(), StoreError> {
    check_empty(document)?;
    let profile = SingleSourceState::AwaitingSource {
        initial_revision: document.revision_id().clone(),
    };
    connection.execute(
        "UPDATE state SET workflow='single_source_v1' WHERE singleton=1",
        [],
    )?;
    connection.execute(
        "INSERT INTO single_source(singleton,profile,baseline_history) VALUES(1,?1,NULL)",
        [serde_json::to_string(&profile)?],
    )?;
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::SingleSource(message.into())
}

fn check_empty(document: &ProjectDocument) -> Result<(), StoreError> {
    if document.nodes().len() != 1
        || !document.assets().is_empty()
        || !document.marks().is_empty()
        || document.basis_state() != &BasisState::provisional()
        || !matches!(&document.nodes()[document.root()].kind, NodeKind::Sequence { children } if children.is_empty())
    {
        return Err(invalid(
            "initial project must contain only an empty provisional root Sequence",
        ));
    }
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid_rows: bool = connection.query_row(
        "SELECT (SELECT count(*) FROM single_source)>1
         OR EXISTS(SELECT 1 FROM single_source WHERE singleton!=1
             OR typeof(profile)!='text' OR length(CAST(profile AS BLOB)) NOT BETWEEN 1 AND ?1
             OR (baseline_history IS NOT NULL AND (typeof(baseline_history)!='integer' OR baseline_history<=0)))
         OR EXISTS(SELECT 1 FROM state WHERE typeof(workflow)!='text' OR workflow NOT IN ('generic','single_source_v1'))",
        [MAX_PROFILE_BYTES as i64], |row| row.get(0),
    )?;
    if invalid_rows {
        return Err(invalid(
            "stored profile exceeds its bound or has invalid metadata",
        ));
    }
    Ok(())
}

fn read(connection: &Connection) -> Result<Option<(SingleSourceState, Option<i64>)>, StoreError> {
    check_stored_sizes(connection)?;
    let workflow: String =
        connection.query_row("SELECT workflow FROM state WHERE singleton=1", [], |row| {
            row.get(0)
        })?;
    let row: Option<(String, Option<i64>)> = connection
        .query_row(
            "SELECT profile,baseline_history FROM single_source WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match (workflow.as_str(), row) {
        ("generic", None) => Ok(None),
        ("single_source_v1", Some((json, floor))) => {
            let state: SingleSourceState = serde_json::from_str(&json)?;
            if matches!(&state, SingleSourceState::AwaitingSource { .. }) != floor.is_none() {
                return Err(invalid("profile state and baseline floor disagree"));
            }
            Ok(Some((state, floor)))
        }
        _ => Err(invalid("workflow marker and profile disagree")),
    }
}

pub(crate) fn at_baseline(connection: &Connection) -> Result<bool, StoreError> {
    let Some((_, Some(floor))) = read(connection)? else {
        return Ok(false);
    };
    let cursor: Option<i64> =
        connection.query_row("SELECT cursor FROM state WHERE singleton=1", [], |row| {
            row.get(0)
        })?;
    Ok(cursor == Some(floor))
}

pub(crate) fn check_registration(
    connection: &Connection,
    receipt: &SourceQualificationReceipt,
    initialize: bool,
) -> Result<(), StoreError> {
    match (read(connection)?, initialize) {
        (Some((SingleSourceState::AwaitingSource { .. }, _)), true) => {
            if receipt.snapshot().video().is_none() {
                return Err(invalid("the original must contain qualified picture"));
            }
        }
        (Some((SingleSourceState::AwaitingSource { .. }, _)), false) => {
            return Err(invalid("choose the original before adding edits or sounds"));
        }
        (Some((SingleSourceState::Ready { qualification, .. }, _)), false) => {
            if receipt.snapshot().video().is_some() && receipt.id() != &qualification {
                return Err(invalid(
                    "this project already has its original; other imports must be audio-only",
                ));
            }
        }
        (None, false) => {}
        _ => {
            return Err(invalid(
                "only a project awaiting its original can be initialized",
            ));
        }
    }
    Ok(())
}

pub(crate) fn finish_initialization(
    connection: &Connection,
    input: &SourceRegistration,
    receipt: &SourceQualificationReceipt,
) -> Result<(), StoreError> {
    let Some((SingleSourceState::AwaitingSource { initial_revision }, None)) = read(connection)?
    else {
        return Err(invalid("project is no longer awaiting its original"));
    };
    let node = input
        .insertion
        .as_ref()
        .ok_or_else(|| invalid("initial source insertion is missing"))?
        .node
        .clone();
    let state = SingleSourceState::Ready {
        initial_revision,
        asset: input.new_asset_id.clone(),
        qualification: receipt.id().clone(),
        node,
        baseline_revision: input.new_revision.clone(),
    };
    connection.execute(
        "UPDATE single_source SET profile=?1,baseline_history=(SELECT cursor FROM state WHERE singleton=1) WHERE singleton=1",
        [serde_json::to_string(&state)?],
    )?;
    // The dedicated constructor and decoded registration derive the entire
    // baseline above. Do not deserialize or rehash its potentially large receipt
    // again on the writer; complete stored chronology is checked on reopen.
    Ok(())
}

pub(crate) fn check_transition(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    decoded_admission: bool,
) -> Result<(), StoreError> {
    match read(connection)? {
        None => Ok(()),
        Some((SingleSourceState::AwaitingSource { initial_revision }, _)) => {
            check_empty(current)?;
            if current.revision_id() != &initial_revision || !decoded_admission {
                return Err(invalid("choose the original before making edits"));
            }
            Ok(())
        }
        Some((state @ SingleSourceState::Ready { .. }, _)) => {
            check_ready_transition(&state, current, next)
        }
    }
}

fn check_ready_transition(
    state: &SingleSourceState,
    current: &ProjectDocument,
    next: &ProjectDocument,
) -> Result<(), StoreError> {
    let SingleSourceState::Ready {
        asset,
        qualification,
        ..
    } = state
    else {
        return Err(invalid("ready state is missing"));
    };
    if !next
        .basis_state()
        .primary
        .as_ref()
        .is_some_and(|primary| &primary.asset == asset && &primary.qualification == qualification)
        || next
            .assets()
            .get(asset)
            .and_then(|record| record.source_qualification.as_ref())
            != Some(qualification)
    {
        return Err(invalid("edits cannot replace the pinned original"));
    }
    let mut generated = BTreeSet::new();
    for node in next.nodes().values() {
        let video = match &node.kind {
            NodeKind::Hold { recipe }
            | NodeKind::Repeat {
                gap: Some(recipe), ..
            } => &recipe.video,
            _ => continue,
        };
        if let HoldVideo::Generated { accepted } = video {
            generated.insert(&accepted.artifact.sampled_asset);
            generated.insert(&accepted.artifact.native_asset);
        }
    }
    for (id, record) in next.assets() {
        if (record.video.is_some() || record.still_image)
            && current.assets().get(id) != Some(record)
            && record.source_qualification.as_ref() != Some(qualification)
            && !generated.contains(id)
        {
            return Err(invalid(
                "additional picture assets are outside the single-original workflow",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let Some((state, floor)) = read(connection)? else {
        return Ok(());
    };
    let initial_id = match &state {
        SingleSourceState::AwaitingSource { initial_revision }
        | SingleSourceState::Ready {
            initial_revision, ..
        } => initial_revision,
    };
    let initial = crate::validation::read_revision(connection, initial_id.as_str())?.document;
    check_empty(&initial)?;
    let initial_valid: bool = connection.query_row(
        "SELECT parent_id IS NULL AND kind='initial' AND rowid=(SELECT MIN(rowid) FROM revisions) FROM revisions WHERE id=?1",
        [initial_id.as_str()], |row| row.get(0),
    )?;
    if !initial_valid {
        return Err(invalid("profile does not name the initial revision"));
    }
    let SingleSourceState::Ready {
        asset,
        qualification,
        node,
        baseline_revision,
        ..
    } = &state
    else {
        let unchanged: bool = connection.query_row(
            "SELECT (SELECT count(*) FROM revisions)=1 AND NOT EXISTS(SELECT 1 FROM history) AND NOT EXISTS(SELECT 1 FROM source_qualifications)", [], |row| row.get(0),
        )?;
        return if unchanged {
            Ok(())
        } else {
            Err(invalid(
                "awaiting project already has authored source or edit history",
            ))
        };
    };
    let baseline_valid: bool = connection
        .query_row(
            "SELECT h.parent_id IS NULL AND h.revision_id=?2 AND r.parent_id=?3 AND r.kind='edit'
         AND r.rowid=(SELECT MIN(rowid) FROM revisions WHERE id!=?3)
         FROM history h JOIN revisions r ON r.id=h.revision_id WHERE h.id=?1",
            params![floor, baseline_revision.as_str(), initial_id.as_str()],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false);
    if !baseline_valid {
        return Err(invalid("baseline is not the first source edit"));
    }
    let baseline =
        crate::validation::read_revision(connection, baseline_revision.as_str())?.document;
    let receipt = crate::source_registration::read_receipt(connection, qualification)?
        .ok_or_else(|| invalid("pinned qualification is missing"))?;
    let basis = receipt
        .snapshot()
        .basis_candidate()?
        .ok_or_else(|| invalid("pinned original has no measured picture basis"))?
        .basis;
    let expected_source = receipt
        .snapshot()
        .derive_timing(basis.frame_rate)?
        .source_node(asset.clone());
    let expected_record = baseline
        .assets()
        .get(asset)
        .ok_or_else(|| invalid("baseline original asset is missing"))?;
    if baseline.root() != initial.root()
        || baseline.nodes().len() != 2
        || baseline.assets().len() != 1
        || !baseline.marks().is_empty()
        || baseline.presentation_basis() != &basis
        || !matches!(&baseline.nodes()[baseline.root()].kind, NodeKind::Sequence { children } if children.as_slice() == [node.clone()])
        || !matches!(baseline.nodes().get(node).map(|node| &node.kind), Some(NodeKind::Source { source }) if *source == expected_source)
        || receipt.asset_record(expected_record.label.clone())? != *expected_record
    {
        return Err(invalid(
            "baseline must contain exactly the full measured original",
        ));
    }
    check_history_floor(
        connection,
        floor.ok_or_else(|| invalid("baseline floor is missing"))?,
    )?;
    // Check every chronology revision, including abandoned branches. An undo of
    // the baseline or later introduction of another picture cannot hide in history.
    let mut statement =
        connection.prepare("SELECT id,parent_id FROM revisions WHERE id!=?1 ORDER BY rowid")?;
    let mut rows = statement.query([initial_id.as_str()])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let parent: String = row.get(1)?;
        let before = crate::validation::read_revision(connection, &parent)?.document;
        let after = crate::validation::read_revision(connection, &id)?.document;
        check_ready_transition(&state, &before, &after)?;
    }
    Ok(())
}

fn check_history_floor(connection: &Connection, floor: i64) -> Result<(), StoreError> {
    // Complete replay has already proved acyclicity and that every historical
    // parent was the live cursor when committed. Requiring the baseline to be
    // the sole history root therefore covers abandoned branches too.
    let other_root: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM history WHERE parent_id IS NULL AND id!=?1)",
        [floor],
        |row| row.get(0),
    )?;
    if other_root {
        return Err(invalid(
            "history contains a branch below the original baseline",
        ));
    }
    let mut cursor: Option<i64> =
        connection.query_row("SELECT cursor FROM state WHERE singleton=1", [], |row| {
            row.get(0)
        })?;
    let mut remaining: i64 =
        connection.query_row("SELECT count(*) FROM history", [], |row| row.get(0))?;
    while let Some(entry) = cursor {
        if entry == floor {
            return Ok(());
        }
        if remaining <= 0 {
            break;
        }
        remaining -= 1;
        cursor = connection.query_row(
            "SELECT parent_id FROM history WHERE id=?1",
            [entry],
            |row| row.get(0),
        )?;
    }
    Err(invalid(
        "current cursor does not descend from the original baseline",
    ))
}
