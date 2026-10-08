//! The Ready AI variants a pause offers, and the operational choices on them.
//!
//! [`offered`] is the one definition of "offered" that the native inspector
//! and the headless commands share: Ready, still present, not the pause's
//! accepted pictures, of a current bridge request whose Hold still exists.
//! [`apply`] makes the native `:pick-ai`/`:next-ai`, `:discard-ai` and
//! `:keep-ai` changes through the same store calls. These are durable
//! operational metadata, not edits: the pause and its history are unchanged
//! and nothing is undoable.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use deadpan_core::{HoldVideo, NodeId, NodeKind, ProjectDocument, RevisionId, ScopedNodeTarget};
use deadpan_jobs::{AttemptId, JobState, MessageIdentity, RequestId};
use deadpan_store::generation_attempts::{
    AttemptMutationOutcome, BundleValidationReceipt, CandidateAvailability,
};
use deadpan_store::generation_retention::DEFAULT_VARIANT_RETENTION;
use deadpan_store::{ProjectStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::live_project::LiveError;

/// One offered Ready variant.
#[derive(Debug, Clone)]
pub struct OfferedVariant {
    pub attempt: AttemptId,
    /// The attempt ordinal within the request (1-based, with gaps).
    pub ordinal: u64,
    pub seed: u64,
    pub receipt: Arc<BundleValidationReceipt>,
    /// When it became Ready (or when an older project was upgraded).
    pub ready_at: SystemTime,
    pub kept: bool,
    pub picked: bool,
    /// When retention stops offering it; `None` while kept, picked,
    /// selected or accepted.
    pub expires_at: Option<SystemTime>,
}

/// The offered variants of one pause's current request.
#[derive(Debug, Clone)]
pub struct OfferedRequest {
    pub request: RequestId,
    pub hold: NodeId,
    pub target: ScopedNodeTarget,
    pub origin_target: ScopedNodeTarget,
    /// The revision the request was conditioned from.
    pub origin: RevisionId,
    /// Project frames the sampled master covers.
    pub frames: i64,
    pub options: deadpan_jobs::GenerationOptions,
    /// Never empty, in attempt order.
    pub variants: Vec<OfferedVariant>,
    /// The store's selection when it is offered, otherwise the newest.
    pub selected: AttemptId,
}

/// Every pause's offered variants in `document`, keyed by authoring scope.
pub fn offered(
    store: &ProjectStore,
    document: &ProjectDocument,
) -> Result<BTreeMap<ScopedNodeTarget, OfferedRequest>, StoreError> {
    let mut found = BTreeMap::new();
    for request in store.current_generation_requests()? {
        if request.bridge_plan().is_none() {
            continue;
        }
        if request.target.validate(document).is_err() {
            continue;
        }
        let hold = request.target.node.clone();
        let Some(NodeKind::Hold { recipe }) = document.nodes().get(&hold).map(|node| &node.kind)
        else {
            continue;
        };
        let accepted = match &recipe.video {
            HoldVideo::Generated { accepted } => Some(&accepted.artifact.sampled_object),
            _ => None,
        };
        let retention = store.generation_variant_retention(&request.request_id)?;
        let store_selected = store
            .selected_generation_bundle(&request.request_id)?
            .map(|selected| selected.identity.attempt_id);
        let mut variants = Vec::new();
        let mut after = 0;
        loop {
            let page = store.generation_attempts(&request.request_id, after, 256)?;
            let Some(last) = page.last() else {
                break;
            };
            after = last.ordinal;
            for attempt in &page {
                let Some(receipt) = attempt.bundle_receipt.as_ref() else {
                    continue;
                };
                if attempt.checkpoint.state != JobState::Ready
                    || receipt.availability() != CandidateAvailability::Present
                    || accepted == Some(receipt.sampled_object())
                {
                    continue;
                }
                let attempt_id = &attempt.checkpoint.identity.attempt_id;
                let record = retention
                    .iter()
                    .find(|record| &record.identity.attempt_id == attempt_id);
                variants.push(OfferedVariant {
                    attempt: attempt_id.clone(),
                    ordinal: attempt.ordinal,
                    seed: receipt.provider().seed,
                    receipt: Arc::new(receipt.clone()),
                    ready_at: record.map_or(UNIX_EPOCH, |record| record.ready_at),
                    kept: record.is_some_and(|record| record.kept),
                    picked: record.is_some_and(|record| record.picked),
                    // The store's own selection never expires.
                    expires_at: record
                        .filter(|_| store_selected.as_ref() != Some(attempt_id))
                        .and_then(|record| record.expires_at(DEFAULT_VARIANT_RETENTION)),
                });
            }
            if page.len() < 256 {
                break;
            }
        }
        let Some(newest) = variants.last() else {
            continue;
        };
        let selected = store_selected
            .filter(|attempt| variants.iter().any(|variant| &variant.attempt == attempt))
            .unwrap_or_else(|| newest.attempt.clone());
        found.insert(
            request.target.clone(),
            OfferedRequest {
                request: request.request_id.clone(),
                hold,
                target: request.target,
                origin_target: request.origin_target,
                origin: request.origin_revision.clone(),
                frames: request.constraints.video.frames().frames(),
                options: deadpan_jobs::GenerationOptions::from_constraints(&request.constraints),
                variants,
                selected,
            },
        );
    }
    Ok(found)
}

/// One operational choice on an offered variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantAction {
    /// `:pick-ai` / `:next-ai`: the variant Preview, Audition and Accept use.
    Select,
    /// `:discard-ai`: never offered again; files go at a later cleanup.
    Discard,
    /// `:keep-ai`: never expires.
    Keep,
    /// `:keep-ai` again: may expire again under the retention policy.
    Release,
}

fn seconds(at: SystemTime) -> Option<u64> {
    at.duration_since(UNIX_EPOCH).ok().map(|at| at.as_secs())
}

/// The stable JSON form of one pause's offered variants. `joins` adds one
/// advisory reading per variant, in order, when measured.
pub fn report(offered: &OfferedRequest, joins: Option<&[Value]>) -> Value {
    let variants: Vec<Value> = offered
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let video = variant.receipt.sampled_video();
            let mut row = json!({
                "number": index + 1,
                "attempt_id": variant.attempt,
                "ordinal": variant.ordinal,
                "seed": variant.seed,
                "sampled_frames": video.frames().frames(),
                "sampled_size": [video.width(), video.height()],
                "ready_unix_seconds": seconds(variant.ready_at),
                "selected": variant.attempt == offered.selected,
                "kept": variant.kept,
                "picked": variant.picked,
                "expires_unix_seconds": variant.expires_at.and_then(seconds),
            });
            if let Some(joins) = joins.and_then(|joins| joins.get(index)) {
                row["joins"] = joins.clone();
            }
            row
        })
        .collect();
    json!({
        "hold": offered.hold,
        "scope": offered.target,
        "request_id": offered.request,
        "origin_revision": offered.origin,
        "frames": offered.frames,
        "options": offered.options,
        "selected_attempt": offered.selected,
        "variants": variants,
    })
}

fn not_offered() -> LiveError {
    LiveError::new(
        "GenerationVariantUnavailable",
        "This AI variant is not offered for its pause: it is not Ready, was discarded or expired, or is the pause's accepted picture",
    )
}

/// Apply `action` to `attempt` of `request` through the same store calls as
/// the native inspector, after checking that the variant is offered.
pub fn apply(
    store: &mut ProjectStore,
    request: &RequestId,
    attempt: &AttemptId,
    action: VariantAction,
) -> Result<Value, LiveError> {
    let document = store.snapshot().map_err(LiveError::store)?;
    let before = offered(store, &document)
        .map_err(LiveError::store)?
        .into_values()
        .find(|candidate| {
            &candidate.request == request
                && candidate
                    .variants
                    .iter()
                    .any(|variant| &variant.attempt == attempt)
        })
        .ok_or_else(not_offered)?;
    let identity = MessageIdentity::new(request.clone(), attempt.clone());
    let changed = match action {
        VariantAction::Select => {
            let stored = store
                .selected_generation_bundle(request)
                .map_err(LiveError::store)?;
            if stored.is_none_or(|selected| selected.identity != identity) {
                store
                    .select_generation_bundle_variant(&identity)
                    .map_err(LiveError::store)?
                    == AttemptMutationOutcome::Applied
            } else {
                false
            }
        }
        VariantAction::Discard => {
            store
                .discard_generation_bundle_variant(&identity)
                .map_err(LiveError::store)?;
            true
        }
        VariantAction::Keep | VariantAction::Release => {
            store
                .keep_generation_bundle_variant(&identity, action == VariantAction::Keep)
                .map_err(LiveError::store)?
                == AttemptMutationOutcome::Applied
        }
    };
    let after = offered(store, &document)
        .map_err(LiveError::store)?
        .remove(&before.target);
    Ok(json!({
        "protocol": 1,
        "action": action,
        "hold": before.hold,
        "request_id": request,
        "attempt_id": attempt,
        "changed": changed,
        "retention_seconds": DEFAULT_VARIANT_RETENTION.as_secs(),
        "offered": after.as_ref().map(|after| report(after, None)),
    }))
}

/// `select-hold|discard-hold|keep-hold <project> --request <id> --attempt <id> [--off]`.
/// Closed projects use their own writer; an open project's app applies the
/// change through its live endpoint and refreshes its inspector.
pub fn run_action(arguments: &[&str], verb: &str) -> Result<(), crate::CliError> {
    if verb == "dismiss-attempt" {
        let [package, "--request", request, "--attempt", attempt] = arguments else {
            return Err(crate::CliError::Usage(
                "usage: dismiss-attempt <project.deadpan> --request <request-id> --attempt <attempt-id>"
                    .into(),
            ));
        };
        return crate::write_json(&crate::live_project::dispatch_short(
            std::path::Path::new(package),
            None,
            crate::live_project::ShortOperation::DismissInterruptedAttempt {
                request: (*request).to_owned(),
                attempt: (*attempt).to_owned(),
            },
        )?);
    }
    let usage = || {
        crate::CliError::Usage(format!(
            "usage: {verb} <project.deadpan> --request <request-id> --attempt <attempt-id>{}",
            if verb == "keep-hold" { " [--off]" } else { "" }
        ))
    };
    let (package, request, attempt, off) = match arguments {
        [package, "--request", request, "--attempt", attempt] => (package, request, attempt, false),
        [package, "--request", request, "--attempt", attempt, "--off"] if verb == "keep-hold" => {
            (package, request, attempt, true)
        }
        _ => return Err(usage()),
    };
    let action = match (verb, off) {
        ("select-hold", _) => VariantAction::Select,
        ("discard-hold", _) => VariantAction::Discard,
        ("keep-hold", false) => VariantAction::Keep,
        ("keep-hold", true) => VariantAction::Release,
        _ => return Err(usage()),
    };
    crate::write_json(&crate::live_project::dispatch_short(
        std::path::Path::new(package),
        None,
        crate::live_project::ShortOperation::GenerationVariant {
            request: RequestId::new(*request).map_err(|_| usage())?,
            attempt: AttemptId::new(*attempt).map_err(|_| usage())?,
            action,
        },
    )?)
}

/// `ai-variants <project> [--hold <id>] [--joins]`: every pause's offered
/// variants, which one is selected and how long each stays offered. With
/// `--joins`, also the advisory join readings `:compare-ai` shows, decoded
/// read-only from the request's origin revision. Writes nothing and works
/// beside an open app.
pub fn run_report(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || {
        crate::CliError::Usage(
            "usage: ai-variants <project.deadpan> [--hold <node-id>] [--joins]".into(),
        )
    };
    let [package, flags @ ..] = arguments else {
        return Err(usage());
    };
    let (hold, joins) = match flags {
        [] => (None, false),
        ["--joins"] => (None, true),
        ["--hold", hold] => (Some(*hold), false),
        ["--hold", hold, "--joins"] | ["--joins", "--hold", hold] => (Some(*hold), true),
        _ => return Err(usage()),
    };
    let hold = hold
        .map(NodeId::new)
        .transpose()
        .map_err(|error| crate::CliError::Usage(error.to_string()))?;
    let package = std::path::Path::new(package);
    let store = ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let all = offered(&store, &document)?;
    if let Some(hold) = &hold
        && !all.values().any(|request| &request.hold == hold)
    {
        return Err(LiveError::new(
            "GenerationVariantUnavailable",
            "This pause offers no Ready AI variants",
        )
        .into());
    }
    let generated = store.generated_read_handle();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let pauses: Vec<Value> = all
        .values()
        .filter(|candidate| hold.as_ref().is_none_or(|hold| &candidate.hold == hold))
        .map(|candidate| {
            let readings = joins.then(|| {
                candidate
                    .variants
                    .iter()
                    .map(|variant| {
                        match super::joins::measure_scoped_request_joins(
                            package,
                            &generated,
                            &candidate.origin,
                            &candidate.origin_target,
                            &variant.receipt,
                            &cancelled,
                        ) {
                            Ok(report) => json!(report),
                            Err(error) => json!({ "error": error.to_string() }),
                        }
                    })
                    .collect::<Vec<_>>()
            });
            report(candidate, readings.as_deref())
        })
        .collect();
    let interrupted = store.interrupted_generation_attempts()?;
    crate::write_json(&json!({
        "protocol": 1,
        "revision_id": document.revision_id(),
        "retention_seconds": DEFAULT_VARIANT_RETENTION.as_secs(),
        "pauses": pauses,
        // Attempts a crash interrupted, offered for retry until dismissed.
        "interrupted": interrupted.attempts,
        "interrupted_warning": interrupted.warning,
    }))
}
