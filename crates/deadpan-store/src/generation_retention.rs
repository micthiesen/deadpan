//! Retention policy for offered, unaccepted AI pause variants
//! (specification Section 19.2: "Unaccepted candidates: evictable, subject to
//! a visible retention policy").
//!
//! Every Ready bridge bundle has one operational retention row, written in
//! the same transaction as its receipt: when it became Ready, whether the
//! person kept (pinned) or explicitly picked it, whether it was accepted or
//! found named by another retained row, and, once its receipt is evicted, why
//! (`discarded` by the person or `expired` by this policy). Nothing here is
//! authored history; Undo never changes it.
//!
//! An *offered* variant is a present Ready bundle of a current request. It
//! expires [`DEFAULT_VARIANT_RETENTION`] after it became Ready unless it is
//! kept, explicitly picked, its request's selection, accepted, or named by
//! another retained row. Expiry is planned on any store (read-only included,
//! so the whole-database scan never runs inside a write transaction) and
//! applied on the writer after a cheap per-row recheck. It marks the receipt
//! evicted exactly like Discard, so the variant is no longer offered and its
//! receipt no longer pins its objects. Files are removed later, only by the
//! ordinary reference-tracked storage cleanup and its grace period.
//!
//! The policy runs on the wall clock, so an automatic pass first checks it
//! against a persisted watermark: a clock behind the newest recorded time, or
//! more than one retention period past the last pass, defers automatic expiry
//! until an explicit cleanup confirms the clock.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deadpan_jobs::{AttemptId, MessageIdentity, RequestId};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::{ProjectStore, StoreError};

/// How long an offered, unprotected variant stays offered after it became
/// Ready.
pub const DEFAULT_VARIANT_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The most variants one automatic pass expires; the rest wait for the next.
pub const MAX_AUTOMATIC_EXPIRY: usize = 32;

/// How far behind the newest recorded time the clock may be (ordinary clock
/// adjustments) before an automatic pass treats it as wrong.
pub const CLOCK_BEHIND_TOLERANCE: Duration = Duration::from_secs(60 * 60);

const CREATE_TABLES: &str = "
CREATE TABLE generation_variant_retention (
    request_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    ready_at_ms INTEGER NOT NULL CHECK (ready_at_ms BETWEEN 0 AND 9223372036854775807),
    kept INTEGER NOT NULL DEFAULT 0 CHECK (kept IN (0,1)),
    picked INTEGER NOT NULL DEFAULT 0 CHECK (picked IN (0,1)),
    accepted INTEGER NOT NULL DEFAULT 0 CHECK (accepted IN (0,1)),
    named_elsewhere INTEGER NOT NULL DEFAULT 0 CHECK (named_elsewhere IN (0,1)),
    eviction TEXT CHECK (eviction IS NULL OR eviction IN ('discarded','expired')),
    evicted_at_ms INTEGER CHECK (evicted_at_ms IS NULL OR evicted_at_ms>=0),
    PRIMARY KEY (request_id,attempt_id),
    FOREIGN KEY (request_id,attempt_id)
        REFERENCES generation_bundle_receipts(request_id,attempt_id),
    CHECK ((eviction IS NULL) = (evicted_at_ms IS NULL))
) STRICT;
CREATE TABLE generation_retention_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton=1),
    last_pass_ms INTEGER NOT NULL CHECK (last_pass_ms>=0)
) STRICT;";

/// Why a variant's receipt was evicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantEviction {
    /// The person discarded it (or the host evicted it explicitly).
    Discarded,
    /// The retention policy expired it.
    Expired,
}

impl VariantEviction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Discarded => "discarded",
            Self::Expired => "expired",
        }
    }
}

/// Who runs an expiry: the app's automatic pass checks the clock and caps
/// its work; an explicit cleanup confirms the current clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryMode {
    Automatic,
    Explicit,
}

/// Why an automatic pass did not trust the wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClockAnomaly {
    /// The clock is before the newest Ready, eviction or pass time recorded
    /// (by more than [`CLOCK_BEHIND_TOLERANCE`]), or before 1970.
    Behind {
        now_unix_ms: i64,
        newest_unix_ms: i64,
    },
    /// The clock is more than one retention period past the last pass and
    /// every recorded time: possibly set ahead.
    Ahead {
        now_unix_ms: i64,
        last_seen_unix_ms: i64,
    },
}

/// One variant's retention record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRetention {
    pub identity: MessageIdentity,
    pub ready_at: SystemTime,
    pub kept: bool,
    /// The person explicitly chose it (Select, Preview or `accept-hold
    /// --attempt`). It stays protected until they pick another variant of
    /// the request or discard it, even if a later variant becomes selected.
    pub picked: bool,
    /// An acceptance of this variant committed at some point. Operational:
    /// Undo does not clear it, because history keeps naming the variant.
    pub accepted: bool,
    /// An expiry found its objects named by another retained row.
    pub named_elsewhere: bool,
    pub eviction: Option<(VariantEviction, SystemTime)>,
}

impl VariantRetention {
    /// Protected by the record itself (not counting selection, which
    /// belongs to the request).
    pub fn protected(&self) -> bool {
        self.kept || self.picked || self.accepted || self.named_elsewhere
    }

    /// When the policy expires this variant if nothing protects it. `None`
    /// when it is protected or already evicted. The caller decides about
    /// selection.
    pub fn expires_at(&self, retention: Duration) -> Option<SystemTime> {
        (!self.protected() && self.eviction.is_none())
            .then(|| self.ready_at.checked_add(retention))
            .flatten()
    }
}

/// One variant expired (or, in a dry run, due to expire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpiredVariant {
    pub request_id: String,
    pub attempt_id: String,
    pub ready_at_unix_ms: u64,
    /// The byte lengths of its distinct masters and provenance: at most what
    /// cleanup can then remove.
    pub bytes: u64,
}

/// The result of applying an [`ExpiryPlan`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VariantExpiry {
    pub dry_run: bool,
    pub retention_seconds: u64,
    pub expired: Vec<ExpiredVariant>,
    pub expired_bytes: u64,
    /// Due by age but kept because another retained row names their
    /// objects. Recorded (`named_elsewhere`), so later passes skip them
    /// without scanning.
    pub skipped_accepted: u64,
    /// Due, but left for a later automatic pass by [`MAX_AUTOMATIC_EXPIRY`].
    pub deferred_by_cap: u64,
    /// Automatic expiry did nothing because the clock looks wrong.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_anomaly: Option<ClockAnomaly>,
}

#[derive(Debug, Clone)]
struct PlannedExpiry {
    identity: MessageIdentity,
    ready_at_ms: i64,
    named_elsewhere: bool,
    bytes: u64,
}

/// What an expiry would do, computed from one read snapshot (any store,
/// read-only included). Apply it with [`ProjectStore::apply_generation_expiry`]
/// on the writer of the same package, which rechecks every row.
#[derive(Debug, Clone)]
pub struct ExpiryPlan {
    package: PathBuf,
    now_ms: i64,
    retention: Duration,
    mode: ExpiryMode,
    clock_anomaly: Option<ClockAnomaly>,
    candidates: Vec<PlannedExpiry>,
    deferred_by_cap: u64,
}

impl ExpiryPlan {
    pub fn mode(&self) -> ExpiryMode {
        self.mode
    }

    pub fn clock_anomaly(&self) -> Option<ClockAnomaly> {
        self.clock_anomaly
    }

    /// Variants the plan would expire or record as named elsewhere.
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// What applying the plan would do if nothing changes meanwhile: the
    /// variants it would expire with their bytes, without rechecking rows
    /// (that is [`ProjectStore::apply_generation_expiry`]'s job).
    pub fn preview(&self) -> VariantExpiry {
        let mut outcome = VariantExpiry {
            dry_run: true,
            retention_seconds: self.retention.as_secs(),
            deferred_by_cap: self.deferred_by_cap,
            clock_anomaly: self.clock_anomaly,
            ..VariantExpiry::default()
        };
        for planned in &self.candidates {
            if planned.named_elsewhere {
                outcome.skipped_accepted += 1;
                continue;
            }
            outcome.expired_bytes += planned.bytes;
            outcome.expired.push(ExpiredVariant {
                request_id: planned.identity.request_id.as_str().to_owned(),
                attempt_id: planned.identity.attempt_id.as_str().to_owned(),
                ready_at_unix_ms: u64::try_from(planned.ready_at_ms).unwrap_or(0),
                bytes: planned.bytes,
            });
        }
        outcome
    }
}

/// The policy and its current state, for the Storage report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VariantRetentionReport {
    pub retention_seconds: u64,
    /// Present Ready variants of current requests.
    pub offered: u64,
    pub kept: u64,
    /// Offered variants the person explicitly chose.
    pub picked: u64,
    /// Offered variants that are their request's selection.
    pub selected: u64,
    /// Offered variants that were accepted or are named by another row.
    pub accepted: u64,
    /// Offered variants the policy will expire (none of the above).
    pub expiring: u64,
    /// Of those, already past their expiry and waiting for a pass, and the
    /// byte lengths of their objects (at most what cleanup then removes).
    pub due: u64,
    pub due_bytes: u64,
    /// The soonest expiry among `expiring`, as Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soonest_expiry_unix_seconds: Option<u64>,
    /// The last expiry pass that trusted the clock, as Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_pass_unix_seconds: Option<u64>,
    /// Set when an automatic pass would not trust the clock now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_anomaly: Option<ClockAnomaly>,
    /// Evicted variants by reason.
    pub discarded: u64,
    pub expired: u64,
    /// Evicted variants with at least one of their own objects still in
    /// `Media/Generated` and unreferenced, and those objects' bytes. Filled
    /// in by the storage report from its namespace listing.
    pub evicted_awaiting_cleanup: u64,
    pub evicted_awaiting_cleanup_bytes: u64,
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(CREATE_TABLES)?;
    Ok(())
}

/// 67 to 68: every existing bundle receipt gets a retention row. Present
/// variants count from the upgrade (`now`), so none expires sooner than one
/// full retention period after it. Evicted ones are recorded as discarded.
/// A variant whose own objects another row already names is flagged
/// accepted, so expiry can trust the flag without scanning.
pub(crate) fn migrate(connection: &Connection, now: SystemTime) -> Result<(), StoreError> {
    create_tables(connection)?;
    let now_ms = clamped_ms(now);
    let named = crate::storage::non_receipt_media_digests(connection)?;
    let rows = connection
        .prepare(
            "SELECT request_id, attempt_id, availability,
                    json_extract(bundle,'$.native_object.content.digest'),
                    json_extract(bundle,'$.sampled_object.content.digest'),
                    json_extract(bundle,'$.provenance_object.content.digest')
             FROM generation_bundle_receipts",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                [
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ],
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (request, attempt, availability, own) in rows {
        let accepted = own.iter().flatten().any(|digest| named.contains(digest));
        let evicted = availability != "present";
        connection.execute(
            "INSERT INTO generation_variant_retention(
                request_id,attempt_id,ready_at_ms,accepted,eviction,evicted_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                request,
                attempt,
                now_ms,
                accepted,
                evicted.then_some("discarded"),
                evicted.then_some(now_ms),
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let invalid: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_variant_retention WHERE
            typeof(request_id)!='text' OR length(CAST(request_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(attempt_id)!='text' OR length(CAST(attempt_id AS BLOB)) NOT BETWEEN 1 AND ?1 OR
            typeof(ready_at_ms)!='integer' OR ready_at_ms<0 OR
            typeof(kept)!='integer' OR kept NOT IN (0,1) OR
            typeof(picked)!='integer' OR picked NOT IN (0,1) OR
            typeof(accepted)!='integer' OR accepted NOT IN (0,1) OR
            typeof(named_elsewhere)!='integer' OR named_elsewhere NOT IN (0,1) OR
            (eviction IS NOT NULL AND (typeof(eviction)!='text' OR eviction NOT IN ('discarded','expired'))) OR
            (evicted_at_ms IS NOT NULL AND (typeof(evicted_at_ms)!='integer' OR evicted_at_ms<0))",
        [deadpan_jobs::MAX_PROTOCOL_ID_BYTES as i64],
        |row| row.get(0),
    )?;
    let invalid_state: i64 = connection.query_row(
        "SELECT COUNT(*) FROM generation_retention_state WHERE
            singleton!=1 OR typeof(last_pass_ms)!='integer' OR last_pass_ms<0",
        [],
        |row| row.get(0),
    )?;
    if invalid != 0 || invalid_state != 0 {
        return Err(StoreError::Integrity(
            "stored AI variant retention exceeds its bounds or has the wrong type".into(),
        ));
    }
    Ok(())
}

/// Exactly one retention row per bundle receipt, evicted exactly when the
/// receipt is.
pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    for (query, message) in [
        (
            "SELECT EXISTS(SELECT 1 FROM generation_bundle_receipts b WHERE NOT EXISTS(
                SELECT 1 FROM generation_variant_retention r
                WHERE r.request_id=b.request_id AND r.attempt_id=b.attempt_id))",
            "AI variant has no retention record",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_variant_retention r WHERE NOT EXISTS(
                SELECT 1 FROM generation_bundle_receipts b
                WHERE b.request_id=r.request_id AND b.attempt_id=r.attempt_id))",
            "AI variant retention record has no bundle receipt",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_variant_retention r
                JOIN generation_bundle_receipts b
                  ON b.request_id=r.request_id AND b.attempt_id=r.attempt_id
                WHERE (r.eviction IS NULL) != (b.availability='present'))",
            "AI variant retention disagrees with its receipt's availability",
        ),
        (
            "SELECT EXISTS(SELECT 1 FROM generation_variant_retention
                WHERE picked=1 GROUP BY request_id HAVING COUNT(*)>1)",
            "more than one explicitly picked AI variant for one request",
        ),
    ] {
        if connection.query_row(query, [], |row| row.get::<_, i64>(0))? != 0 {
            return Err(StoreError::Integrity(message.into()));
        }
    }
    Ok(())
}

/// Record a new Ready variant, in its receipt's transaction. A clock before
/// 1970 records zero rather than failing the receipt.
pub(crate) fn record_ready(
    connection: &Connection,
    identity: &MessageIdentity,
    now: SystemTime,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO generation_variant_retention(request_id,attempt_id,ready_at_ms)
         VALUES (?1,?2,?3)",
        params![
            identity.request_id.as_str(),
            identity.attempt_id.as_str(),
            clamped_ms(now)
        ],
    )?;
    Ok(())
}

/// Record why a receipt was evicted, in its eviction's transaction.
pub(crate) fn record_eviction(
    connection: &Connection,
    identity: &MessageIdentity,
    reason: VariantEviction,
    now: SystemTime,
) -> Result<(), StoreError> {
    let updated = connection.execute(
        "UPDATE generation_variant_retention SET eviction=?1, evicted_at_ms=?2, picked=0
         WHERE request_id=?3 AND attempt_id=?4 AND eviction IS NULL",
        params![
            reason.as_str(),
            clamped_ms(now),
            identity.request_id.as_str(),
            identity.attempt_id.as_str()
        ],
    )?;
    if updated != 1 {
        return Err(StoreError::Integrity(
            "AI variant retention record is missing or already evicted".into(),
        ));
    }
    Ok(())
}

/// Flag the accepted variant, in the acceptance's transaction.
pub(crate) fn record_accepted(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<(), StoreError> {
    let updated = connection.execute(
        "UPDATE generation_variant_retention SET accepted=1
         WHERE request_id=?1 AND attempt_id=?2",
        params![identity.request_id.as_str(), identity.attempt_id.as_str()],
    )?;
    if updated != 1 {
        return Err(StoreError::Integrity(
            "the accepted AI variant has no retention record".into(),
        ));
    }
    Ok(())
}

/// Record the person's explicit choice of `identity`, in its selection's
/// transaction: it becomes the request's only picked variant.
pub(crate) fn record_picked(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<(), StoreError> {
    connection.execute(
        "UPDATE generation_variant_retention SET picked=0
         WHERE request_id=?1 AND attempt_id!=?2 AND picked=1",
        params![identity.request_id.as_str(), identity.attempt_id.as_str()],
    )?;
    let updated = connection.execute(
        "UPDATE generation_variant_retention SET picked=1
         WHERE request_id=?1 AND attempt_id=?2 AND eviction IS NULL",
        params![identity.request_id.as_str(), identity.attempt_id.as_str()],
    )?;
    if updated != 1 {
        return Err(StoreError::Integrity(
            "the chosen AI variant has no retention record".into(),
        ));
    }
    Ok(())
}

/// Milliseconds since 1970, clamped to `0..=i64::MAX`.
fn clamped_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// Milliseconds since 1970, negative before it.
fn signed_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |ms| -ms),
    }
}

fn duration_ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn from_unix_ms(ms: i64) -> Result<SystemTime, StoreError> {
    u64::try_from(ms)
        .ok()
        .and_then(|ms| UNIX_EPOCH.checked_add(Duration::from_millis(ms)))
        .ok_or_else(|| StoreError::Integrity("invalid AI variant retention time".into()))
}

fn eviction(
    name: Option<String>,
    at: Option<i64>,
) -> Result<Option<(VariantEviction, SystemTime)>, StoreError> {
    Ok(match (name.as_deref(), at) {
        (None, None) => None,
        (Some("discarded"), Some(at)) => Some((VariantEviction::Discarded, from_unix_ms(at)?)),
        (Some("expired"), Some(at)) => Some((VariantEviction::Expired, from_unix_ms(at)?)),
        _ => return Err(StoreError::Integrity("invalid AI variant eviction".into())),
    })
}

fn last_pass_ms(connection: &Connection) -> Result<Option<i64>, StoreError> {
    Ok(connection
        .query_row(
            "SELECT last_pass_ms FROM generation_retention_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

/// Whether an automatic pass may trust `now_ms`.
fn check_clock(
    connection: &Connection,
    now_ms: i64,
    retention: Duration,
) -> Result<Option<ClockAnomaly>, StoreError> {
    let last_pass = last_pass_ms(connection)?;
    let newest_record: Option<i64> = connection.query_row(
        "SELECT MAX(MAX(ready_at_ms, COALESCE(evicted_at_ms, 0)))
         FROM generation_variant_retention",
        [],
        |row| row.get(0),
    )?;
    let Some(newest) = [last_pass, newest_record].into_iter().flatten().max() else {
        return Ok((now_ms < 0).then_some(ClockAnomaly::Behind {
            now_unix_ms: now_ms,
            newest_unix_ms: 0,
        }));
    };
    if now_ms < 0 || now_ms < newest.saturating_sub(duration_ms(CLOCK_BEHIND_TOLERANCE)) {
        return Ok(Some(ClockAnomaly::Behind {
            now_unix_ms: now_ms,
            newest_unix_ms: newest,
        }));
    }
    if now_ms.saturating_sub(newest) > duration_ms(retention) {
        return Ok(Some(ClockAnomaly::Ahead {
            now_unix_ms: now_ms,
            last_seen_unix_ms: newest,
        }));
    }
    Ok(None)
}

/// An offered variant row: present Ready bundle of a current request.
struct Offered {
    request: String,
    attempt: String,
    ready_at_ms: i64,
    kept: bool,
    picked: bool,
    accepted: bool,
    named_elsewhere: bool,
    selected: bool,
    own: [Option<String>; 3],
    lengths: [Option<i64>; 3],
}

impl Offered {
    fn protected(&self) -> bool {
        self.kept || self.picked || self.accepted || self.named_elsewhere || self.selected
    }

    /// The byte lengths of its distinct own objects.
    fn bytes(&self) -> u64 {
        let mut seen = BTreeSet::new();
        self.own
            .iter()
            .zip(self.lengths)
            .filter_map(|(digest, length)| Some((digest.as_ref()?, length?)))
            .filter(|(digest, _)| seen.insert(*digest))
            .map(|(_, length)| u64::try_from(length).unwrap_or(0))
            .sum()
    }
}

const OFFERED_SELECT: &str = "
    SELECT r.request_id, r.attempt_id, r.ready_at_ms, r.kept, r.picked, r.accepted,
           r.named_elsewhere, coalesce(h.selected_ready_attempt_id=r.attempt_id, 0),
           json_extract(b.bundle,'$.native_object.content.digest'),
           json_extract(b.bundle,'$.sampled_object.content.digest'),
           json_extract(b.bundle,'$.provenance_object.content.digest'),
           json_extract(b.bundle,'$.native_object.byte_length'),
           json_extract(b.bundle,'$.sampled_object.byte_length'),
           json_extract(b.bundle,'$.provenance_object.byte_length')
    FROM generation_variant_retention r
    JOIN generation_bundle_receipts b
      ON b.request_id=r.request_id AND b.attempt_id=r.attempt_id
    JOIN generation_attempts a
      ON a.request_id=r.request_id AND a.attempt_id=r.attempt_id
    JOIN generation_requests q ON q.request_id=r.request_id
    LEFT JOIN generation_attempt_heads h ON h.request_id=r.request_id
    WHERE b.availability='present' AND a.state='ready' AND q.relevance='current'";

fn offered_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Offered> {
    Ok(Offered {
        request: row.get(0)?,
        attempt: row.get(1)?,
        ready_at_ms: row.get(2)?,
        kept: row.get(3)?,
        picked: row.get(4)?,
        accepted: row.get(5)?,
        named_elsewhere: row.get(6)?,
        selected: row.get::<_, i64>(7)? != 0,
        own: [row.get(8)?, row.get(9)?, row.get(10)?],
        lengths: [row.get(11)?, row.get(12)?, row.get(13)?],
    })
}

fn offered(connection: &Connection) -> Result<Vec<Offered>, StoreError> {
    Ok(connection
        .prepare(&format!(
            "{OFFERED_SELECT} ORDER BY r.ready_at_ms, r.request_id, a.ordinal"
        ))?
        .query_map([], offered_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

fn offered_one(
    connection: &Connection,
    identity: &MessageIdentity,
) -> Result<Option<Offered>, StoreError> {
    Ok(connection
        .query_row(
            &format!("{OFFERED_SELECT} AND r.request_id=?1 AND r.attempt_id=?2"),
            params![identity.request_id.as_str(), identity.attempt_id.as_str()],
            offered_row,
        )
        .optional()?)
}

fn identity_of(request: &str, attempt: &str) -> Result<MessageIdentity, StoreError> {
    Ok(MessageIdentity::new(
        RequestId::new(request.to_owned())
            .map_err(|_| StoreError::Integrity("invalid request ID".into()))?,
        AttemptId::new(attempt.to_owned())
            .map_err(|_| StoreError::Integrity("invalid attempt ID".into()))?,
    ))
}

impl ProjectStore {
    /// Keep (pin) or stop keeping one offered variant. A kept variant never
    /// expires. Operational, not undoable, durable across reopen. Refused
    /// unless the variant is a present Ready bundle of a current request.
    pub fn keep_generation_bundle_variant(
        &mut self,
        identity: &MessageIdentity,
        keep: bool,
    ) -> Result<crate::generation_attempts::AttemptMutationOutcome, StoreError> {
        use crate::generation_attempts::AttemptMutationOutcome;
        self.require_writer()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(variant) = offered_one(&transaction, identity)? else {
            return Err(StoreError::GenerationAttempt(
                "only an offered AI variant of a current request can be kept".into(),
            ));
        };
        if variant.kept == keep {
            return Ok(AttemptMutationOutcome::Duplicate);
        }
        transaction.execute(
            "UPDATE generation_variant_retention SET kept=?1 WHERE request_id=?2 AND attempt_id=?3",
            params![
                keep,
                identity.request_id.as_str(),
                identity.attempt_id.as_str()
            ],
        )?;
        transaction.commit()?;
        Ok(AttemptMutationOutcome::Applied)
    }

    /// Every variant's retention record of `request`, by attempt ordinal.
    pub fn generation_variant_retention(
        &self,
        request: &RequestId,
    ) -> Result<Vec<VariantRetention>, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let rows = transaction
            .prepare(
                "SELECT r.attempt_id, r.ready_at_ms, r.kept, r.picked, r.accepted,
                        r.named_elsewhere, r.eviction, r.evicted_at_ms
                 FROM generation_variant_retention r
                 JOIN generation_attempts a
                   ON a.request_id=r.request_id AND a.attempt_id=r.attempt_id
                 WHERE r.request_id=?1 ORDER BY a.ordinal",
            )?
            .query_map([request.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    [
                        row.get::<_, bool>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, bool>(4)?,
                        row.get::<_, bool>(5)?,
                    ],
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit()?;
        rows.into_iter()
            .map(
                |(attempt, ready, [kept, picked, accepted, named_elsewhere], name, at)| {
                    Ok(VariantRetention {
                        identity: MessageIdentity::new(
                            request.clone(),
                            AttemptId::new(attempt)
                                .map_err(|_| StoreError::Integrity("invalid attempt ID".into()))?,
                        ),
                        ready_at: from_unix_ms(ready)?,
                        kept,
                        picked,
                        accepted,
                        named_elsewhere,
                        eviction: eviction(name, at)?,
                    })
                },
            )
            .collect()
    }

    /// What an automatic check would find about the clock at `now`, from
    /// the recorded times alone (no scan).
    pub fn generation_retention_clock(
        &self,
        now: SystemTime,
        retention: Duration,
    ) -> Result<Option<ClockAnomaly>, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let anomaly = check_clock(&transaction, signed_ms(now), retention)?;
        transaction.commit()?;
        Ok(anomaly)
    }

    /// Plan an expiry at `now` from one read snapshot. Available on any
    /// store, read-only included, so the reference scan (run only when some
    /// unprotected variant is due) never holds the writer. An automatic plan
    /// whose clock check fails plans nothing, and one with more due variants
    /// than [`MAX_AUTOMATIC_EXPIRY`] plans the oldest.
    pub fn plan_generation_expiry(
        &self,
        now: SystemTime,
        retention: Duration,
        mode: ExpiryMode,
    ) -> Result<ExpiryPlan, StoreError> {
        let now_ms = signed_ms(now);
        let transaction = self.connection.unchecked_transaction()?;
        let mut plan = ExpiryPlan {
            package: self.package.clone(),
            now_ms,
            retention,
            mode,
            clock_anomaly: None,
            candidates: Vec::new(),
            deferred_by_cap: 0,
        };
        if mode == ExpiryMode::Automatic {
            plan.clock_anomaly = check_clock(&transaction, now_ms, retention)?;
            if plan.clock_anomaly.is_some() {
                return Ok(plan);
            }
        }
        let due: Vec<Offered> = offered(&transaction)?
            .into_iter()
            .filter(|variant| {
                !variant.protected()
                    && variant.ready_at_ms.saturating_add(duration_ms(retention)) <= now_ms
            })
            .collect();
        if !due.is_empty() {
            let named: BTreeSet<String> = crate::storage::non_receipt_media_digests(&transaction)?;
            let mut expiring = 0;
            for variant in due {
                let named_elsewhere = variant
                    .own
                    .iter()
                    .flatten()
                    .any(|digest| named.contains(digest));
                if !named_elsewhere && mode == ExpiryMode::Automatic {
                    if expiring == MAX_AUTOMATIC_EXPIRY {
                        plan.deferred_by_cap += 1;
                        continue;
                    }
                    expiring += 1;
                }
                plan.candidates.push(PlannedExpiry {
                    identity: identity_of(&variant.request, &variant.attempt)?,
                    ready_at_ms: variant.ready_at_ms,
                    named_elsewhere,
                    bytes: variant.bytes(),
                });
            }
        }
        transaction.commit()?;
        Ok(plan)
    }

    /// Apply `plan` on the writer in one short transaction. Each planned
    /// variant is rechecked (still offered, unprotected, unselected, same
    /// Ready time, due); one found named elsewhere is recorded so later
    /// passes skip it; the others are evicted exactly as Discard evicts,
    /// with reason `expired`. A pass that trusted the clock records it as
    /// the watermark. A dry run writes nothing.
    pub fn apply_generation_expiry(
        &mut self,
        plan: &ExpiryPlan,
        dry_run: bool,
    ) -> Result<VariantExpiry, StoreError> {
        self.require_writer()?;
        let same = |path: &PathBuf| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if same(&plan.package) != same(&self.package) {
            return Err(StoreError::GenerationAttempt(
                "the expiry plan belongs to another project".into(),
            ));
        }
        let mut outcome = VariantExpiry {
            dry_run,
            retention_seconds: plan.retention.as_secs(),
            deferred_by_cap: plan.deferred_by_cap,
            clock_anomaly: plan.clock_anomaly,
            ..VariantExpiry::default()
        };
        if plan.clock_anomaly.is_some() {
            return Ok(outcome);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = UNIX_EPOCH + Duration::from_millis(u64::try_from(plan.now_ms).unwrap_or(0));
        for planned in &plan.candidates {
            let Some(current) = offered_one(&transaction, &planned.identity)? else {
                continue;
            };
            if current.protected()
                || current.ready_at_ms != planned.ready_at_ms
                || current
                    .ready_at_ms
                    .saturating_add(duration_ms(plan.retention))
                    > plan.now_ms
            {
                continue;
            }
            if planned.named_elsewhere {
                outcome.skipped_accepted += 1;
                if !dry_run {
                    transaction.execute(
                        "UPDATE generation_variant_retention SET named_elsewhere=1
                         WHERE request_id=?1 AND attempt_id=?2",
                        params![
                            planned.identity.request_id.as_str(),
                            planned.identity.attempt_id.as_str()
                        ],
                    )?;
                }
                continue;
            }
            if !dry_run {
                crate::generation_attempts::evict_bundle(
                    &transaction,
                    &planned.identity,
                    VariantEviction::Expired,
                    now,
                )?;
            }
            outcome.expired_bytes += planned.bytes;
            outcome.expired.push(ExpiredVariant {
                request_id: planned.identity.request_id.as_str().to_owned(),
                attempt_id: planned.identity.attempt_id.as_str().to_owned(),
                ready_at_unix_ms: u64::try_from(planned.ready_at_ms).unwrap_or(0),
                bytes: planned.bytes,
            });
        }
        if !dry_run {
            if plan.now_ms >= 0 {
                transaction.execute(
                    "INSERT INTO generation_retention_state(singleton,last_pass_ms) VALUES (1,?1)
                     ON CONFLICT(singleton) DO UPDATE SET last_pass_ms=excluded.last_pass_ms",
                    [plan.now_ms],
                )?;
            }
            transaction.commit()?;
        }
        Ok(outcome)
    }

    /// Plan and apply on this writer: what `project storage --clean` uses.
    /// The app plans off the writer instead.
    pub fn expire_generation_variants(
        &mut self,
        now: SystemTime,
        retention: Duration,
        mode: ExpiryMode,
        dry_run: bool,
    ) -> Result<VariantExpiry, StoreError> {
        self.require_writer()?;
        let plan = self.plan_generation_expiry(now, retention, mode)?;
        self.apply_generation_expiry(&plan, dry_run)
    }
}

/// The policy and current counts. Object bytes are added by the storage
/// report.
pub(crate) fn retention_report(
    connection: &Connection,
    now: SystemTime,
    retention: Duration,
) -> Result<VariantRetentionReport, StoreError> {
    let now_ms = signed_ms(now);
    let retention_ms = duration_ms(retention);
    let mut report = VariantRetentionReport {
        retention_seconds: retention.as_secs(),
        last_pass_unix_seconds: last_pass_ms(connection)?
            .and_then(|ms| u64::try_from(ms / 1000).ok()),
        clock_anomaly: check_clock(connection, now_ms, retention)?,
        ..VariantRetentionReport::default()
    };
    let mut soonest: Option<i64> = None;
    for variant in offered(connection)? {
        report.offered += 1;
        report.kept += u64::from(variant.kept);
        report.picked += u64::from(variant.picked);
        report.selected += u64::from(variant.selected);
        report.accepted += u64::from(variant.accepted || variant.named_elsewhere);
        if variant.protected() {
            continue;
        }
        report.expiring += 1;
        let at = variant.ready_at_ms.saturating_add(retention_ms);
        if at <= now_ms {
            report.due += 1;
            report.due_bytes += variant.bytes();
        }
        soonest = Some(soonest.map_or(at, |soonest| soonest.min(at)));
    }
    report.soonest_expiry_unix_seconds = soonest.and_then(|ms| u64::try_from(ms / 1000).ok());
    let (discarded, expired): (i64, i64) = connection.query_row(
        "SELECT COALESCE(SUM(eviction='discarded'),0), COALESCE(SUM(eviction='expired'),0)
         FROM generation_variant_retention",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    report.discarded = u64::try_from(discarded).unwrap_or(0);
    report.expired = u64::try_from(expired).unwrap_or(0);
    Ok(report)
}

/// Own object digests of every evicted variant, for the storage report.
pub(crate) fn evicted_objects(
    connection: &Connection,
) -> Result<Vec<[Option<String>; 3]>, StoreError> {
    Ok(connection
        .prepare(
            "SELECT json_extract(bundle,'$.native_object.content.digest'),
                    json_extract(bundle,'$.sampled_object.content.digest'),
                    json_extract(bundle,'$.provenance_object.content.digest')
             FROM generation_bundle_receipts WHERE availability='evicted'",
        )?
        .query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))?
        .collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_before_1970_clamp_for_records_and_stay_negative_for_checks() {
        let before = UNIX_EPOCH - Duration::from_secs(5);
        assert_eq!(clamped_ms(before), 0);
        assert_eq!(signed_ms(before), -5_000);
        assert_eq!(clamped_ms(UNIX_EPOCH + Duration::from_millis(7)), 7);
    }
}
