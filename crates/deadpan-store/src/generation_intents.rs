//! Current automatic AI intent is independent of conditioning/attempt state.
//! Immutable preparation births authorize activations; one-shot terminals close
//! them. Retiring fulfilled or capacity-displaced work never closes its intent.

use std::collections::BTreeMap;

use deadpan_core::{
    FrameDuration, HoldFallback, HoldVideo, NodeKind, ProjectDocument, ProjectId, RevisionId,
    ScopedNodeTarget, ValidatedScopedIsolation,
};
use deadpan_jobs::RequestId;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::generation_origins::GenerationInputBinding;
use crate::generation_preparations::{PreparationId, PreparationOrigin};
use crate::{ProjectStore, StoreError};

mod capture;
#[cfg(test)]
mod extension_tests;
pub(crate) use capture::{capture_heads, capture_inputs};

const MAX_HEAD_BYTES: usize = 64 * 1024 * 1024;
const MAX_TERMINAL_BYTES: usize = 256 * 1024;
const MAX_DETAIL_BYTES: usize = 2048;
pub const MAX_INTENT_PAGE: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentCause {
    InsertedPause,
    DurationExtension,
    DurationChanged,
    SourceBoundaryChanged,
    /// SHA-256 of the complete canonical sorted scoped member set. The host
    /// rederives that set and the SCC policy from immutable transition inputs.
    CyclicBoundaryDependencies {
        group_sha256: String,
    },
}

impl IntentCause {
    pub fn cyclic_group(members: &[ScopedNodeTarget]) -> Result<Self, StoreError> {
        if members.is_empty()
            || members.len() > deadpan_core::MAX_DOCUMENT_NODES
            || members.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "cyclic members must be nonempty, bounded and strictly sorted",
            ));
        }
        let mut hash = Sha256::new();
        hash.update(b"deadpan-intent-cyclic-group-1\0");
        for member in members {
            let bytes = crate::generation_scope::target_json(member)?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes.as_bytes());
        }
        Ok(Self::CyclicBoundaryDependencies {
            group_sha256: hex(&hash.finalize()),
        })
    }

    fn validate(&self) -> Result<(), StoreError> {
        if let Self::CyclicBoundaryDependencies { group_sha256 } = self
            && (group_sha256.len() != 64
                || !group_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(invalid("invalid cyclic group identity"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentAuthorization {
    AuthoredOrigin,
    Renewal { predecessor: PreparationId },
    Redo { original_activation: PreparationId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputUnavailableCause {
    UnsupportedPicture,
    MissingQualification,
    QueryLimit,
    InvalidRetainedEvidence,
    MissingContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentInputBinding {
    Measured {
        binding: Box<GenerationInputBinding>,
    },
    Unavailable {
        cause: InputUnavailableCause,
        detail: String,
    },
}

impl IntentInputBinding {
    fn supports_renewal(&self) -> bool {
        matches!(
            self,
            Self::Measured { .. }
                | Self::Unavailable {
                    cause: InputUnavailableCause::MissingContext,
                    ..
                }
        )
    }
    pub(crate) fn same_authority(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Measured { binding: a }, Self::Measured { binding: b }) => a == b,
            (Self::Unavailable { cause: a, .. }, Self::Unavailable { cause: b, .. }) => a == b,
            _ => false,
        }
    }

    fn validate(&self, duration: FrameDuration) -> Result<(), StoreError> {
        match self {
            Self::Measured { binding } if binding.duration == duration => Ok(()),
            Self::Unavailable { detail, .. }
                if !detail.is_empty() && detail.len() <= MAX_DETAIL_BYTES =>
            {
                Ok(())
            }
            _ => Err(invalid(
                "birth input binding has invalid duration or diagnostic bounds",
            )),
        }
    }
}

/// The remaining immutable birth fields live once in the enclosing preparation:
/// id, project, origin revision/target, duration and origin. A retirement keeps
/// the identical projection, so no mutable state can become birth authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentBirthReceipt {
    pub schema_version: u32,
    pub history_id: i64,
    pub cause: IntentCause,
    pub authorization: IntentAuthorization,
    pub fallback: HoldFallback,
    /// Resolved once at birth, even when its temporal context is unavailable.
    pub capture: Option<crate::generation_inputs::GenerationCaptureSpec>,
    pub input_binding: IntentInputBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentBirth {
    pub activation_id: PreparationId,
    pub project_id: ProjectId,
    pub activation_revision: RevisionId,
    pub origin_target: ScopedNodeTarget,
    pub duration: FrameDuration,
    pub origin: PreparationOrigin,
    pub receipt: IntentBirthReceipt,
}

impl IntentBirth {
    fn permits_renewal_to(&self, next: &Self) -> bool {
        if !self.receipt.input_binding.supports_renewal()
            || !next.receipt.input_binding.supports_renewal()
            || next.receipt.capture.is_none()
        {
            return false;
        }
        if self.receipt.capture != next.receipt.capture
            && !(self.receipt.capture.is_none()
                && self.origin.options().is_some_and(|options| {
                    options.mode == deadpan_jobs::GenerationModePreference::Automatic
                })
                && matches!(
                    self.receipt.input_binding,
                    IntentInputBinding::Unavailable {
                        cause: InputUnavailableCause::MissingContext,
                        ..
                    }
                ))
        {
            return false;
        }
        self.duration != next.duration
            || self.receipt.capture != next.receipt.capture
            || !self
                .receipt
                .input_binding
                .same_authority(&next.receipt.input_binding)
    }

    pub(crate) fn validate(&self) -> Result<(), StoreError> {
        if self.receipt.schema_version != 1
            || self.receipt.history_id < 1
            || self.duration == FrameDuration::ZERO
        {
            return Err(invalid("invalid immutable birth receipt"));
        }
        crate::generation_scope::target_json(&self.origin_target)?;
        self.receipt.cause.validate()?;
        self.receipt.input_binding.validate(self.duration)?;
        if let IntentInputBinding::Measured { binding } = &self.receipt.input_binding
            && self.receipt.capture != Some(binding.capture_spec())
        {
            return Err(invalid("birth operation differs from its measured inputs"));
        }
        Ok(())
    }

    pub(crate) fn matches_hold(
        &self,
        document: &ProjectDocument,
        target: &ScopedNodeTarget,
    ) -> bool {
        let Some(NodeKind::Hold { recipe }) =
            document.nodes().get(&target.node).map(|node| &node.kind)
        else {
            return false;
        };
        recipe.duration == self.duration && recipe.video == fallback_video(&self.receipt.fallback)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentHead {
    pub activation_id: PreparationId,
    pub target: ScopedNodeTarget,
    pub request_id: Option<RequestId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentGenerationIntent {
    pub revision: RevisionId,
    pub head: IntentHead,
    pub birth: IntentBirth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPhase {
    Transition,
    AfterRevision,
}

impl TerminalPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Transition => "transition",
            Self::AfterRevision => "after_revision",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentTerminalReason {
    Renewed {
        successor: PreparationId,
    },
    ProviderChoice,
    Accepted {
        request_id: RequestId,
        attempt_id: deadpan_jobs::AttemptId,
    },
    ExplicitGenerationRequest {
        request_id: RequestId,
    },
    UserCancelled,
    Deleted,
    HistoryDetached,
    Capacity,
}

impl IntentTerminalReason {
    fn successor(&self) -> Option<&PreparationId> {
        if let Self::Renewed { successor } = self {
            Some(successor)
        } else {
            None
        }
    }

    pub(crate) fn detail(&self) -> &'static str {
        match self {
            Self::Renewed { .. } => "Superseded by a fresh automatic generation intent.",
            Self::ProviderChoice => "Superseded by an explicit picture provider choice.",
            Self::Accepted { .. } => "The generated candidate was explicitly accepted.",
            Self::ExplicitGenerationRequest { .. } => {
                "Superseded by an explicit generation request."
            }
            Self::UserCancelled => "Cancelled by the user; the committed pause is unchanged.",
            Self::Deleted => "The intended Hold was removed from this edit.",
            Self::HistoryDetached => {
                "The intended fallback or conditioning inputs changed in history."
            }
            Self::Capacity => {
                "The bounded automatic-intent index filled; the saved fallback is unchanged."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentTerminal {
    pub activation_id: PreparationId,
    pub at_revision: RevisionId,
    pub phase: TerminalPhase,
    pub target: ScopedNodeTarget,
    pub reason: IntentTerminalReason,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch("CREATE TABLE generation_intent_heads (
        activation_id TEXT PRIMARY KEY,
        current_target TEXT NOT NULL UNIQUE CHECK(json_valid(current_target)),
        hold_id TEXT NOT NULL UNIQUE,
        request_id TEXT UNIQUE REFERENCES generation_requests(request_id)
    ) STRICT;
    CREATE TABLE generation_intent_terminals (
        activation_id TEXT PRIMARY KEY,
        at_revision TEXT NOT NULL REFERENCES revisions(id) DEFERRABLE INITIALLY DEFERRED,
        phase TEXT NOT NULL CHECK(phase IN ('transition','after_revision')),
        successor_id TEXT UNIQUE,
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;
    CREATE INDEX generation_intent_terminals_revision ON generation_intent_terminals(at_revision,phase,activation_id);")?;
    Ok(())
}

pub(crate) fn check_sizes(connection: &Connection) -> Result<(), StoreError> {
    let (count, bytes, bad): (i64, i64, bool) = connection.query_row("SELECT count(*),coalesce(sum(
        length(CAST(activation_id AS BLOB))+length(CAST(current_target AS BLOB))+length(CAST(hold_id AS BLOB))+coalesce(length(CAST(request_id AS BLOB)),0)),0),
        coalesce(max(length(CAST(activation_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR length(CAST(hold_id AS BLOB)) NOT BETWEEN 1 AND ?1
            OR length(CAST(current_target AS BLOB))>?2 OR (request_id IS NOT NULL AND length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1)),0)
        FROM generation_intent_heads", params![deadpan_core::MAX_IDENTITY_BYTES as i64, crate::generation_scope::MAX_TARGET_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    if count > deadpan_core::MAX_DOCUMENT_NODES as i64 || bytes > MAX_HEAD_BYTES as i64 || bad {
        return Err(invalid(
            "current head index exceeds its count or byte bounds",
        ));
    }
    let bad: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM generation_intent_terminals WHERE
        length(CAST(record AS BLOB))>?1 OR length(CAST(activation_id AS BLOB)) NOT BETWEEN 1 AND ?2
        OR length(CAST(at_revision AS BLOB)) NOT BETWEEN 1 AND ?2
        OR (successor_id IS NOT NULL AND length(CAST(successor_id AS BLOB)) NOT BETWEEN 1 AND ?2))",
        params![
            MAX_TERMINAL_BYTES as i64,
            deadpan_core::MAX_IDENTITY_BYTES as i64
        ],
        |row| row.get(0),
    )?;
    if bad {
        return Err(invalid("terminal receipt exceeds its bounds"));
    }
    Ok(())
}

fn parse_head(row: &rusqlite::Row<'_>) -> Result<IntentHead, StoreError> {
    let target = crate::generation_scope::parse_target(&row.get::<_, String>(1)?)?;
    if target.node.as_str() != row.get::<_, String>(2)? {
        return Err(invalid("head Hold ID differs from its target"));
    }
    Ok(IntentHead {
        activation_id: PreparationId::new(row.get::<_, String>(0)?)?,
        target,
        request_id: row
            .get::<_, Option<String>>(3)?
            .map(RequestId::new)
            .transpose()
            .map_err(|_| invalid("invalid intent request identity"))?,
    })
}

pub(crate) fn heads(connection: &Connection) -> Result<Vec<IntentHead>, StoreError> {
    check_sizes(connection)?;
    let mut statement = connection.prepare("SELECT activation_id,current_target,hold_id,request_id FROM generation_intent_heads ORDER BY activation_id")?;
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(parse_head(row)?);
    }
    Ok(result)
}

pub(crate) fn read_head(
    connection: &Connection,
    activation: &PreparationId,
) -> Result<Option<IntentHead>, StoreError> {
    let mut statement = connection.prepare("SELECT activation_id,current_target,hold_id,request_id FROM generation_intent_heads WHERE activation_id=?1")?;
    let mut rows = statement.query([activation.as_str()])?;
    rows.next()?.map(parse_head).transpose()
}

pub(crate) fn load_current(
    connection: &Connection,
    target: &ScopedNodeTarget,
) -> Result<Option<IntentHead>, StoreError> {
    let mut statement = connection.prepare("SELECT activation_id,current_target,hold_id,request_id FROM generation_intent_heads WHERE current_target=?1")?;
    let mut rows = statement.query([crate::generation_scope::target_json(target)?])?;
    rows.next()?.map(parse_head).transpose()
}

pub(crate) fn read_birth(
    connection: &Connection,
    activation: &PreparationId,
) -> Result<IntentBirth, StoreError> {
    crate::generation_preparations::read_intent_birth(connection, activation)?
        .ok_or_else(|| invalid("activation has no live or retired immutable birth"))
}

fn parse_terminal(row: &rusqlite::Row<'_>) -> Result<IntentTerminal, StoreError> {
    let text: String = row.get(0)?;
    if text.len() > MAX_TERMINAL_BYTES {
        return Err(invalid("terminal receipt exceeds its byte limit"));
    }
    let terminal: IntentTerminal = serde_json::from_str(&text)?;
    if terminal.activation_id.as_str() != row.get::<_, String>(1)?
        || terminal.at_revision.as_str() != row.get::<_, String>(2)?
        || terminal.phase.name() != row.get::<_, String>(3)?
        || terminal.reason.successor().map(PreparationId::as_str)
            != row.get::<_, Option<String>>(4)?.as_deref()
        || serde_json::to_string(&terminal)? != text
    {
        return Err(invalid("terminal receipt differs from its indexed columns"));
    }
    crate::generation_scope::target_json(&terminal.target)?;
    Ok(terminal)
}

pub(crate) fn read_terminal(
    connection: &Connection,
    activation: &PreparationId,
) -> Result<Option<IntentTerminal>, StoreError> {
    let mut statement = connection.prepare("SELECT record,activation_id,at_revision,phase,successor_id FROM generation_intent_terminals WHERE activation_id=?1")?;
    let mut rows = statement.query([activation.as_str()])?;
    rows.next()?.map(parse_terminal).transpose()
}

/// One-shot CAS. Work revocation and request staleness share the caller's writer
/// transaction with the terminal and head deletion. A later close never rewrites
/// the first receipt or mutates a successor's authorization.
pub(crate) fn close(
    connection: &Connection,
    expected: &IntentHead,
    terminal: IntentTerminal,
) -> Result<bool, StoreError> {
    if terminal.activation_id != expected.activation_id || terminal.target != expected.target {
        return Err(invalid("terminal does not name its captured head"));
    }
    if terminal.phase == TerminalPhase::AfterRevision
        && crate::validation::read_head(connection)? != terminal.at_revision.as_str()
    {
        return Ok(false);
    }
    if read_head(connection, &expected.activation_id)?.as_ref() != Some(expected) {
        return Ok(false);
    }
    let text = serde_json::to_string(&terminal)?;
    if text.len() > MAX_TERMINAL_BYTES {
        return Err(invalid("terminal receipt exceeds its byte limit"));
    }
    connection.execute("INSERT INTO generation_intent_terminals(activation_id,at_revision,phase,successor_id,record) VALUES (?1,?2,?3,?4,?5)",
        params![terminal.activation_id.as_str(), terminal.at_revision.as_str(), terminal.phase.name(), terminal.reason.successor().map(PreparationId::as_str), text])?;
    connection.execute(
        "DELETE FROM generation_intent_heads WHERE activation_id=?1",
        [expected.activation_id.as_str()],
    )?;
    crate::generation_preparations::close_intent_work(
        connection,
        &expected.activation_id,
        terminal.reason.detail(),
    )?;
    if let Some(request) = &expected.request_id
        && !matches!(terminal.reason, IntentTerminalReason::Accepted { .. })
    {
        connection.execute("UPDATE generation_requests SET relevance='stale' WHERE request_id=?1 AND relevance='current'", [request.as_str()])?;
    }
    Ok(true)
}

pub(crate) fn link_request(
    connection: &Connection,
    activation: &PreparationId,
    target: &ScopedNodeTarget,
    request: &RequestId,
) -> Result<(), StoreError> {
    let changed = connection.execute("UPDATE generation_intent_heads SET request_id=?1 WHERE activation_id=?2 AND current_target=?3 AND request_id IS NULL",
        params![request.as_str(), activation.as_str(), crate::generation_scope::target_json(target)?])?;
    if changed != 1 {
        return Err(invalid(
            "fulfilled preparation is no longer the unfulfilled current intent",
        ));
    }
    Ok(())
}

pub(crate) fn map(
    connection: &Connection,
    proof: &ValidatedScopedIsolation<'_>,
    forward: bool,
) -> Result<(), StoreError> {
    for mut head in heads(connection)? {
        head.target = if forward {
            proof.map_retained_forward(&head.target)
        } else {
            proof.map_retained_backward(&head.target)
        };
        connection.execute("UPDATE generation_intent_heads SET current_target=?1,hold_id=?2 WHERE activation_id=?3",
            params![crate::generation_scope::target_json(&head.target)?, head.target.node.as_str(), head.activation_id.as_str()])?;
    }
    check_sizes(connection)
}

/// Charge future scoped-isolation identifier growth once, while preserving
/// sparse Repeat choices. Work fulfilment's request link has a fixed reserve.
fn head_charge(head: &IntentHead) -> Result<usize, StoreError> {
    let identity = deadpan_core::MAX_IDENTITY_BYTES;
    let target = crate::generation_scope::target_json(&head.target)?;
    Ok(head.activation_id.as_str().len()
        + target.len()
        + identity
        + deadpan_jobs::MAX_PROTOCOL_ID_BYTES
        + identity
        - head.target.node.as_str().len()
        + head
            .target
            .repeats
            .iter()
            .map(|step| identity - step.repeat.as_str().len())
            .sum::<usize>())
}

#[derive(Default)]
pub(crate) struct ActivationCapacity {
    order: std::collections::BTreeSet<(i64, PreparationId)>,
    charges: BTreeMap<PreparationId, (i64, usize)>,
    total: usize,
    revision_order: i64,
}

impl ActivationCapacity {
    pub(crate) fn new(connection: &Connection, revision: &RevisionId) -> Result<Self, StoreError> {
        let revision_order = connection.query_row(
            "SELECT rowid FROM revisions WHERE id=?1",
            [revision.as_str()],
            |row| row.get(0),
        )?;
        let mut result = Self {
            order: Default::default(),
            charges: BTreeMap::new(),
            total: 0,
            revision_order,
        };
        let mut statement = connection.prepare("SELECT h.activation_id,h.current_target,h.hold_id,h.request_id,r.rowid FROM generation_intent_heads h JOIN
            (SELECT id,origin_revision FROM generation_preparations UNION ALL SELECT id,origin_revision FROM generation_preparation_retirements) b ON b.id=h.activation_id
            JOIN revisions r ON r.id=b.origin_revision ORDER BY h.activation_id")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let head = parse_head(row)?;
            result.insert(head.activation_id.clone(), row.get(4)?, head_charge(&head)?)?;
            if result.charges.len() > deadpan_core::MAX_DOCUMENT_NODES {
                return Err(invalid("current head count exceeds its bound"));
            }
        }
        Ok(result)
    }
    fn insert(&mut self, id: PreparationId, order: i64, charge: usize) -> Result<(), StoreError> {
        if self.charges.insert(id.clone(), (order, charge)).is_some() {
            return Err(invalid("head birth index has duplicate identities"));
        }
        self.total = self
            .total
            .checked_add(charge)
            .ok_or_else(|| invalid("head capacity arithmetic overflow"))?;
        self.order.insert((order, id));
        Ok(())
    }
    fn remove(&mut self, id: &PreparationId) {
        if let Some((order, charge)) = self.charges.remove(id) {
            self.total -= charge;
            self.order.remove(&(order, id.clone()));
        }
    }
    fn oldest(&self) -> Option<&PreparationId> {
        self.order.first().map(|(_, id)| id)
    }
}

/// Admit a new activation after its immutable preparation was inserted. A
/// renewal consumes one exact live head; cancellation can never supply it.
pub(crate) fn activate(
    connection: &Connection,
    birth: &IntentBirth,
    capacity: &mut ActivationCapacity,
) -> Result<Vec<PreparationId>, StoreError> {
    birth.validate()?;
    if read_terminal(connection, &birth.activation_id)?.is_some()
        || read_head(connection, &birth.activation_id)?.is_some()
    {
        return Err(invalid("activation identity was already used"));
    }
    if let IntentAuthorization::Renewal { predecessor } = &birth.receipt.authorization {
        let previous = read_head(connection, predecessor)?
            .ok_or_else(|| invalid("renewal predecessor is not current"))?;
        let original = read_birth(connection, predecessor)?;
        if previous.target.node != birth.origin_target.node
            || original.project_id != birth.project_id
            || original.origin != birth.origin
            || original.receipt.fallback != birth.receipt.fallback
            || !original.permits_renewal_to(birth)
        {
            return Err(invalid(
                "renewal differs from its measured current predecessor",
            ));
        }
        capacity.remove(predecessor);
        close(
            connection,
            &previous,
            IntentTerminal {
                activation_id: predecessor.clone(),
                at_revision: birth.activation_revision.clone(),
                phase: TerminalPhase::Transition,
                target: previous.target.clone(),
                reason: IntentTerminalReason::Renewed {
                    successor: birth.activation_id.clone(),
                },
            },
        )?;
    } else if load_current(connection, &birth.origin_target)?.is_some() {
        return Err(invalid(
            "new authored activation would replace an unclosed intent",
        ));
    }
    let target = crate::generation_scope::target_json(&birth.origin_target)?;
    let head = IntentHead {
        activation_id: birth.activation_id.clone(),
        target: birth.origin_target.clone(),
        request_id: None,
    };
    let incoming = head_charge(&head)?;
    let mut displaced = Vec::new();
    while capacity.charges.len() >= deadpan_core::MAX_DOCUMENT_NODES
        || capacity.total + incoming > MAX_HEAD_BYTES
    {
        let id = capacity
            .oldest()
            .ok_or_else(|| invalid("one head cannot fit the intent byte budget"))?
            .clone();
        let head =
            read_head(connection, &id)?.ok_or_else(|| invalid("capacity head disappeared"))?;
        close(
            connection,
            &head,
            IntentTerminal {
                activation_id: id.clone(),
                at_revision: birth.activation_revision.clone(),
                phase: TerminalPhase::Transition,
                target: head.target.clone(),
                reason: IntentTerminalReason::Capacity,
            },
        )?;
        capacity.remove(&id);
        displaced.push(id);
    }
    capacity.insert(
        birth.activation_id.clone(),
        capacity.revision_order,
        incoming,
    )?;
    connection.execute("INSERT INTO generation_intent_heads(activation_id,current_target,hold_id,request_id) VALUES (?1,?2,?3,NULL)",
        params![birth.activation_id.as_str(),target,birth.origin_target.node.as_str()])?;
    Ok(displaced)
}

/// Keep current intent across completed work, while closing an actually
/// removed or changed target. Canonical addresses are recovered by exact node
/// identity after ordinary wrapping; a copied Hold never receives this head.
pub(crate) fn reconcile(
    connection: &Connection,
    document: &ProjectDocument,
) -> Result<Vec<crate::generation_preparations::PreparationNotice>, StoreError> {
    let mut current = heads(connection)?;
    if current.is_empty() {
        return Ok(Vec::new());
    }
    let plan =
        deadpan_plan::RenderPlan::compile(document).map_err(|error| invalid(&error.to_string()))?;
    let targets = plan
        .authored_hold_targets(deadpan_core::BoundaryQueryLimits {
            max_scopes: deadpan_core::MAX_DOCUMENT_NODES * 64,
            max_comparisons: deadpan_core::MAX_DOCUMENT_NODES * 64,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    let by_node: BTreeMap<_, _> = targets
        .into_iter()
        .map(|target| (target.node.clone(), target))
        .collect();
    let mut surviving = Vec::new();
    for mut head in current.drain(..) {
        let Some(target) = by_node.get(&head.target.node) else {
            close(
                connection,
                &head,
                IntentTerminal {
                    activation_id: head.activation_id.clone(),
                    at_revision: document.revision_id().clone(),
                    phase: TerminalPhase::Transition,
                    target: head.target.clone(),
                    reason: IntentTerminalReason::Deleted,
                },
            )?;
            continue;
        };
        if &head.target != target {
            head.target = target.clone();
            connection.execute(
                "UPDATE generation_intent_heads SET current_target=?1 WHERE activation_id=?2",
                params![
                    crate::generation_scope::target_json(target)?,
                    head.activation_id.as_str()
                ],
            )?;
        }
        surviving.push(head);
    }
    let bindings = capture_heads(connection, document, &surviving)?;
    for (head, binding) in surviving.into_iter().zip(bindings) {
        let birth = read_birth(connection, &head.activation_id)?;
        if !birth.matches_hold(document, &head.target) || !binding.matches(&birth.receipt) {
            close(
                connection,
                &head,
                IntentTerminal {
                    activation_id: head.activation_id.clone(),
                    at_revision: document.revision_id().clone(),
                    phase: TerminalPhase::Transition,
                    target: head.target.clone(),
                    reason: IntentTerminalReason::HistoryDetached,
                },
            )?;
        }
    }
    let mut capacity = ActivationCapacity::new(connection, document.revision_id())?;
    let mut displaced = Vec::new();
    let mut total = 0;
    while capacity.total > MAX_HEAD_BYTES {
        let id = capacity
            .oldest()
            .ok_or_else(|| invalid("oversized head index is empty"))?
            .clone();
        let head =
            read_head(connection, &id)?.ok_or_else(|| invalid("capacity head disappeared"))?;
        close(
            connection,
            &head,
            IntentTerminal {
                activation_id: id.clone(),
                at_revision: document.revision_id().clone(),
                phase: TerminalPhase::Transition,
                target: head.target.clone(),
                reason: IntentTerminalReason::Capacity,
            },
        )?;
        capacity.remove(&id);
        total += 1;
        if displaced.len() < MAX_INTENT_PAGE {
            displaced.push(id);
        }
    }
    if total == 0 {
        Ok(Vec::new())
    } else {
        Ok(vec![
            crate::generation_preparations::PreparationNotice::IntentCapacity { displaced, total },
        ])
    }
}

impl ProjectStore {
    pub fn generation_intents(
        &self,
        after: Option<&PreparationId>,
        limit: usize,
    ) -> Result<Vec<CurrentGenerationIntent>, StoreError> {
        if !(1..=MAX_INTENT_PAGE).contains(&limit) {
            return Err(invalid("intent page must contain 1–256 rows"));
        }
        crate::generation_preparations::verify(&self.connection)?;
        let revision = RevisionId::new(crate::validation::read_head(&self.connection)?)?;
        let mut statement = self.connection.prepare("SELECT activation_id,current_target,hold_id,request_id FROM generation_intent_heads WHERE (?1 IS NULL OR activation_id>?1) ORDER BY activation_id LIMIT ?2")?;
        let mut rows = statement.query(params![after.map(PreparationId::as_str), limit as i64])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let head = parse_head(row)?;
            let birth = read_birth(&self.connection, &head.activation_id)?;
            result.push(CurrentGenerationIntent {
                revision: revision.clone(),
                head,
                birth,
            });
        }
        Ok(result)
    }

    /// Close the whole automatic intent, including Fulfilled/Ready or retired
    /// work. Discarding one candidate variant does not call this operation.
    pub fn cancel_generation_intent(
        &mut self,
        activation: &PreparationId,
        expected_revision: &RevisionId,
    ) -> Result<bool, StoreError> {
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::generation_preparations::verify(&transaction)?;
        if crate::validation::read_head(&transaction)? != expected_revision.as_str() {
            return Ok(false);
        }
        let Some(head) = read_head(&transaction, activation)? else {
            return Ok(false);
        };
        let result = close(
            &transaction,
            &head,
            IntentTerminal {
                activation_id: activation.clone(),
                at_revision: expected_revision.clone(),
                phase: TerminalPhase::AfterRevision,
                target: head.target.clone(),
                reason: IntentTerminalReason::UserCancelled,
            },
        )?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }
}

pub(crate) fn fallback_video(fallback: &HoldFallback) -> HoldVideo {
    match fallback {
        HoldFallback::Background => HoldVideo::Background,
        HoldFallback::Freeze { asset, timestamp } => HoldVideo::Freeze {
            asset: asset.clone(),
            timestamp: *timestamp,
        },
    }
}

fn terminals_at(
    connection: &Connection,
    revision: &RevisionId,
    phase: TerminalPhase,
) -> Result<Vec<IntentTerminal>, StoreError> {
    let mut statement = connection.prepare("SELECT record,activation_id,at_revision,phase,successor_id FROM generation_intent_terminals WHERE at_revision=?1 AND phase=?2 ORDER BY activation_id")?;
    let mut rows = statement.query(params![revision.as_str(), phase.name()])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        if result.len() >= deadpan_core::MAX_DOCUMENT_NODES * 2 {
            return Err(invalid("revision terminal batch exceeds its bound"));
        }
        result.push(parse_terminal(row)?);
    }
    Ok(result)
}

pub(crate) fn digest(connection: &Connection) -> Result<crate::audit::Chain, StoreError> {
    check_sizes(connection)?;
    let mut hash = Sha256::new();
    hash.update(b"deadpan-generation-intents-1\0");
    for head in heads(connection)? {
        let bytes = serde_json::to_vec(&head)?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.update([0]);
    let mut statement = connection.prepare("SELECT record,activation_id,at_revision,phase,successor_id FROM generation_intent_terminals ORDER BY activation_id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let terminal = parse_terminal(row)?;
        let bytes = serde_json::to_vec(&terminal)?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(hash.finalize().into())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    let current = heads(connection)?;
    if current.is_empty() {
        return Ok(());
    }
    let document = crate::read_snapshot(connection)?;
    if current.iter().try_fold(0usize, |sum, head| {
        head_charge(head).map(|charge| sum + charge)
    })? > MAX_HEAD_BYTES
    {
        return Err(invalid("current heads exceed their reserved byte budget"));
    }
    let bindings = capture_heads(connection, &document, &current)?;
    for (head, binding) in current.into_iter().zip(bindings) {
        let birth = read_birth(connection, &head.activation_id)?;
        if birth.project_id != *document.project_id()
            || !birth.matches_hold(&document, &head.target)
            || !binding.matches(&birth.receipt)
            || read_terminal(connection, &head.activation_id)?.is_some()
            || crate::generation_preparations::intent_request(connection, &head.activation_id)?
                != head.request_id
        {
            return Err(invalid(
                "current intent differs from its immutable birth or fallback",
            ));
        }
        validate_request_interval(
            connection,
            &birth,
            head.request_id.as_ref(),
            document.revision_id(),
        )?;
    }
    Ok(())
}

fn validate_request_interval(
    connection: &Connection,
    birth: &IntentBirth,
    request: Option<&RequestId>,
    through: &RevisionId,
) -> Result<(), StoreError> {
    if let Some(request) = request {
        let valid: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM generation_requests q JOIN revisions r ON r.id=q.origin_revision JOIN revisions b ON b.id=?2 JOIN revisions t ON t.id=?3 WHERE q.request_id=?1 AND r.rowid BETWEEN b.rowid AND t.rowid AND q.project_id=?4)",
            params![request.as_str(),birth.activation_revision.as_str(),through.as_str(),birth.project_id.as_str()], |row| row.get(0))?;
        if !valid {
            return Err(invalid(
                "intent fulfilment lies outside its activation lifetime",
            ));
        }
    }
    Ok(())
}

/// Chronological proof keeps only the bounded current-head set. Immutable birth
/// and terminal receipts remain indexed on disk; a later cancellation cannot
/// retroactively revoke an earlier renewal's authority.
#[derive(Default)]
pub(crate) struct Replay {
    current: BTreeMap<PreparationId, IntentHead>,
    terminals: u64,
    capacity: ActivationCapacity,
}

impl Replay {
    pub(crate) fn heads(&self) -> Vec<IntentHead> {
        self.current.values().cloned().collect()
    }

    pub(crate) fn command(
        &mut self,
        connection: &Connection,
        before: &ProjectDocument,
        request: &deadpan_core::CommandRequest,
        after: &ProjectDocument,
        history: i64,
        births: &[crate::generation_preparations::Birth],
    ) -> Result<(), StoreError> {
        let choices = crate::generation_preparations::provider_choices(
            before,
            request,
            &self.current.values().cloned().collect::<Vec<_>>(),
        )?;
        crate::generation_scope::with_command_proof(before, request, after, |proof| {
            self.map(proof, true);
            Ok(())
        })?;
        self.arrive(connection, after, history, births, &choices)
    }

    pub(crate) fn history(
        &mut self,
        connection: &Connection,
        _before: &ProjectDocument,
        entry: i64,
        redo: bool,
        after: &ProjectDocument,
        births: &[crate::generation_preparations::Birth],
    ) -> Result<(), StoreError> {
        let mut choices = std::collections::BTreeSet::new();
        if redo {
            let historical = crate::validation::read_history(connection, entry)?;
            let original = crate::validation::read_revision(
                connection,
                historical.request.expected_revision.as_str(),
            )?
            .document;
            choices = crate::generation_preparations::provider_choices(
                &original,
                &historical.request,
                &self.current.values().cloned().collect::<Vec<_>>(),
            )?;
        }
        crate::generation_scope::with_history_proof(connection, entry, |proof| {
            self.map(proof, redo);
            Ok(())
        })?;
        self.arrive(connection, after, entry, births, &choices)
    }

    fn map(&mut self, proof: &ValidatedScopedIsolation<'_>, forward: bool) {
        for head in self.current.values_mut() {
            head.target = if forward {
                proof.map_retained_forward(&head.target)
            } else {
                proof.map_retained_backward(&head.target)
            };
        }
    }

    fn arrive(
        &mut self,
        connection: &Connection,
        document: &ProjectDocument,
        history: i64,
        births: &[crate::generation_preparations::Birth],
        choices: &std::collections::BTreeSet<PreparationId>,
    ) -> Result<(), StoreError> {
        self.refresh_capacity()?;
        self.capacity.revision_order = connection.query_row(
            "SELECT rowid FROM revisions WHERE id=?1",
            [document.revision_id().as_str()],
            |row| row.get(0),
        )?;
        let mut transition = terminals_at(
            connection,
            document.revision_id(),
            TerminalPhase::Transition,
        )?;
        if self.current.is_empty() && births.is_empty() {
            if !transition.is_empty()
                || !terminals_at(
                    connection,
                    document.revision_id(),
                    TerminalPhase::AfterRevision,
                )?
                .is_empty()
            {
                return Err(invalid(
                    "terminal closes an activation which was not current",
                ));
            }
            return Ok(());
        }
        // Provider choices happen before new activations. In particular, a
        // same-fallback choice must close a head although its pixels match.
        for terminal in transition.iter().filter(|terminal| {
            matches!(
                terminal.reason,
                IntentTerminalReason::ProviderChoice | IntentTerminalReason::Accepted { .. }
            )
        }) {
            if !choices.contains(&terminal.activation_id) {
                return Err(invalid("provider terminal has no explicit authored choice"));
            }
            self.consume(connection, terminal)?;
        }
        if choices.iter().any(|id| self.current.contains_key(id)) {
            return Err(invalid(
                "explicit provider choice retained automatic intent",
            ));
        }
        transition.retain(|terminal| {
            !matches!(
                terminal.reason,
                IntentTerminalReason::ProviderChoice | IntentTerminalReason::Accepted { .. }
            )
        });
        let mut capacity = BTreeMap::new();
        for terminal in &transition {
            if matches!(terminal.reason, IntentTerminalReason::Capacity) {
                capacity.insert(terminal.activation_id.clone(), terminal.clone());
            }
        }
        let mut occupied: std::collections::BTreeSet<_> = self
            .current
            .values()
            .map(|head| head.target.clone())
            .collect();
        let birth_ids: std::collections::BTreeSet<_> = births
            .iter()
            .map(|birth| {
                crate::generation_preparations::id_for(document.revision_id(), &birth.target)
            })
            .collect::<Result<_, _>>()?;
        // Birth order is the same typed batch order used by writer admission.
        for derived in births {
            let id =
                crate::generation_preparations::id_for(document.revision_id(), &derived.target)?;
            let birth = read_birth(connection, &id)?;
            if birth.receipt.history_id != history
                || birth.receipt.authorization != derived.authorization
                || birth.receipt.cause != derived.cause
            {
                return Err(invalid(
                    "birth authorization differs from the derived transition",
                ));
            }
            match &birth.receipt.authorization {
                IntentAuthorization::AuthoredOrigin => {}
                IntentAuthorization::Redo {
                    original_activation,
                } => {
                    let original = read_birth(connection, original_activation)?;
                    if original.receipt.history_id != history
                        || original.origin != birth.origin
                        || original.origin_target != birth.origin_target
                        || original.duration != birth.duration
                        || original.receipt.fallback != birth.receipt.fallback
                        || original.receipt.capture != birth.receipt.capture
                        || !original
                            .receipt
                            .input_binding
                            .same_authority(&birth.receipt.input_binding)
                        || original.activation_id == birth.activation_id
                    {
                        return Err(invalid(
                            "redo differs from its original immutable authorization",
                        ));
                    }
                }
                IntentAuthorization::Renewal { predecessor } => {
                    let terminal = transition
                        .iter()
                        .find(|terminal| &terminal.activation_id == predecessor)
                        .ok_or_else(|| invalid("renewal has no predecessor terminal"))?;
                    if terminal.reason
                        != (IntentTerminalReason::Renewed {
                            successor: id.clone(),
                        })
                    {
                        return Err(invalid("renewal authorization is not reciprocal"));
                    }
                    let previous = self.current.get(predecessor).ok_or_else(|| {
                        invalid("renewal predecessor was not current at that revision")
                    })?;
                    let original = read_birth(connection, predecessor)?;
                    if previous.target.node != birth.origin_target.node
                        || original.origin != birth.origin
                        || original.receipt.fallback != birth.receipt.fallback
                        || !original.permits_renewal_to(&birth)
                    {
                        return Err(invalid(
                            "renewal does not preserve its measured predecessor authority",
                        ));
                    }
                    occupied.remove(&previous.target);
                    self.consume(connection, terminal)?;
                }
            }
            if occupied.contains(&birth.origin_target) {
                return Err(invalid("multiple automatic heads own the same target"));
            }
            let head = IntentHead {
                activation_id: id.clone(),
                target: birth.origin_target,
                request_id: crate::generation_preparations::intent_request(connection, &id)?,
            };
            let charge = head_charge(&head)?;
            while self.capacity.charges.len() >= deadpan_core::MAX_DOCUMENT_NODES
                || self.capacity.total + charge > MAX_HEAD_BYTES
            {
                let oldest = self
                    .capacity
                    .oldest()
                    .ok_or_else(|| invalid("incoming intent exceeds capacity"))?
                    .clone();
                let terminal = capacity.remove(&oldest).ok_or_else(|| {
                    invalid("head admission omitted deterministic Capacity closure")
                })?;
                occupied.remove(&terminal.target);
                self.consume(connection, &terminal)?;
            }
            self.capacity
                .insert(id.clone(), self.capacity.revision_order, charge)?;
            occupied.insert(head.target.clone());
            if self.current.insert(id, head).is_some() {
                return Err(invalid("activation was reused during replay"));
            }
        }
        // Reconcile ordinary wrapping by exact retained Hold identity, then
        // verify every other closure against the actual final fallback/inputs.
        let plan = deadpan_plan::RenderPlan::compile(document)
            .map_err(|error| invalid(&error.to_string()))?;
        let targets = plan
            .authored_hold_targets(deadpan_core::BoundaryQueryLimits {
                max_scopes: deadpan_core::MAX_DOCUMENT_NODES * 64,
                max_comparisons: deadpan_core::MAX_DOCUMENT_NODES * 64,
            })
            .map_err(|error| invalid(&error.to_string()))?;
        let by_node: BTreeMap<_, _> = targets
            .into_iter()
            .map(|target| (target.node.clone(), target))
            .collect();
        for head in self.current.values_mut() {
            if let Some(target) = by_node.get(&head.target.node) {
                head.target = target.clone();
            }
        }
        let present: Vec<_> = self
            .current
            .values()
            .filter(|head| by_node.contains_key(&head.target.node))
            .cloned()
            .collect();
        let bindings = capture_heads(connection, document, &present)?;
        let bindings: BTreeMap<_, _> = present
            .into_iter()
            .zip(bindings)
            .map(|(head, binding)| (head.activation_id, binding))
            .collect();
        for terminal in &transition {
            match &terminal.reason {
                IntentTerminalReason::Renewed { successor } => {
                    if self.current.contains_key(&terminal.activation_id)
                        || !birth_ids.contains(successor)
                    {
                        return Err(invalid("renewed terminal has no derived successor"));
                    }
                }
                IntentTerminalReason::Deleted | IntentTerminalReason::HistoryDetached => {
                    let head = self
                        .current
                        .get(&terminal.activation_id)
                        .ok_or_else(|| invalid("detached activation was not current"))?;
                    let birth = read_birth(connection, &head.activation_id)?;
                    let absent = !by_node.contains_key(&head.target.node);
                    let detached = !birth.matches_hold(document, &head.target)
                        || bindings
                            .get(&head.activation_id)
                            .is_none_or(|binding| !binding.matches(&birth.receipt));
                    if matches!(terminal.reason, IntentTerminalReason::Deleted) != absent
                        || (!absent && !detached)
                    {
                        return Err(invalid(
                            "history closure still has its exact fallback and inputs",
                        ));
                    }
                    self.consume(connection, terminal)?;
                }
                IntentTerminalReason::Capacity => {}
                _ => {
                    return Err(invalid(
                        "operational closure is incorrectly placed in an authored transition",
                    ));
                }
            }
        }
        self.refresh_capacity()?;
        while self.capacity.total > MAX_HEAD_BYTES {
            let oldest = self
                .capacity
                .oldest()
                .ok_or_else(|| invalid("oversized replay index is empty"))?
                .clone();
            let terminal = capacity
                .remove(&oldest)
                .ok_or_else(|| invalid("scope growth omitted deterministic Capacity closure"))?;
            self.consume(connection, &terminal)?;
        }
        if !capacity.is_empty() {
            return Err(invalid(
                "Capacity receipt did not correspond to exhausted head capacity",
            ));
        }
        if self.current.len() > deadpan_core::MAX_DOCUMENT_NODES {
            return Err(invalid("replayed head index exceeds its count bound"));
        }
        for head in self.current.values() {
            let birth = read_birth(connection, &head.activation_id)?;
            if !birth.matches_hold(document, &head.target)
                || bindings
                    .get(&head.activation_id)
                    .is_none_or(|binding| !binding.matches(&birth.receipt))
            {
                return Err(invalid(
                    "changed current intent lacks a proven closure or renewal",
                ));
            }
        }
        for terminal in terminals_at(
            connection,
            document.revision_id(),
            TerminalPhase::AfterRevision,
        )? {
            if !matches!(
                terminal.reason,
                IntentTerminalReason::UserCancelled
                    | IntentTerminalReason::ExplicitGenerationRequest { .. }
            ) {
                return Err(invalid(
                    "authored closure is incorrectly placed after a revision",
                ));
            }
            self.consume(connection, &terminal)?;
        }
        Ok(())
    }

    fn consume(
        &mut self,
        connection: &Connection,
        terminal: &IntentTerminal,
    ) -> Result<(), StoreError> {
        let head = self
            .current
            .get(&terminal.activation_id)
            .ok_or_else(|| invalid("terminal closes an activation which was not current"))?;
        if head.target != terminal.target {
            return Err(invalid(
                "terminal target differs from its chronological current head",
            ));
        }
        let birth = read_birth(connection, &head.activation_id)?;
        validate_request_interval(
            connection,
            &birth,
            head.request_id.as_ref(),
            &terminal.at_revision,
        )?;
        match &terminal.reason {
            IntentTerminalReason::Accepted {
                request_id,
                attempt_id,
            } => {
                let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM generation_accepted_origins WHERE request_id=?1 AND attempt_id=?2 AND accepted_revision=?3)",
                    params![request_id.as_str(),attempt_id.as_str(),terminal.at_revision.as_str()], |row| row.get(0))?;
                if head.request_id.as_ref() != Some(request_id) || !exists {
                    return Err(invalid("accepted intent has no matching fulfilled attempt"));
                }
            }
            IntentTerminalReason::ExplicitGenerationRequest { request_id } => {
                let target: Option<String> = connection.query_row("SELECT origin_target FROM generation_requests WHERE request_id=?1 AND origin_revision=?2",
                    params![request_id.as_str(),terminal.at_revision.as_str()], |row| row.get(0)).optional()?;
                if target
                    .as_deref()
                    .map(crate::generation_scope::parse_target)
                    .transpose()?
                    .as_ref()
                    != Some(&head.target)
                {
                    return Err(invalid(
                        "explicit request closure belongs to another target or revision",
                    ));
                }
            }
            _ => {}
        }
        self.current.remove(&terminal.activation_id);
        self.capacity.remove(&terminal.activation_id);
        self.terminals = self
            .terminals
            .checked_add(1)
            .ok_or_else(|| invalid("terminal count overflow"))?;
        Ok(())
    }

    fn refresh_capacity(&mut self) -> Result<(), StoreError> {
        let mut total = 0usize;
        for (id, head) in &self.current {
            let charge = head_charge(head)?;
            let (_, recorded) = self
                .capacity
                .charges
                .get_mut(id)
                .ok_or_else(|| invalid("replay capacity index is missing a head"))?;
            *recorded = charge;
            total = total
                .checked_add(charge)
                .ok_or_else(|| invalid("replay byte budget overflow"))?;
        }
        self.capacity.total = total;
        Ok(())
    }

    pub(crate) fn finish(self, connection: &Connection) -> Result<(), StoreError> {
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM generation_intent_terminals",
            [],
            |row| row.get(0),
        )?;
        let actual: BTreeMap<_, _> = heads(connection)?
            .into_iter()
            .map(|head| (head.activation_id.clone(), head))
            .collect();
        if u64::try_from(count).ok() != Some(self.terminals) || self.current != actual {
            return Err(invalid(
                "current heads or one-shot terminals differ from chronological history",
            ));
        }
        validate_store(connection)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid(reason: &str) -> StoreError {
    StoreError::Integrity(format!("generation intent: {reason}"))
}

#[cfg(test)]
mod tests;
