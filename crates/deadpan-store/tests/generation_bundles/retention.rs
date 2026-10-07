//! The retention policy for offered, unaccepted AI variants: keep, expiry
//! with an injected clock, and what reference-tracked cleanup then removes.
use super::storage::{distinct_variant, state};
use super::*;
use std::time::{Duration, SystemTime};

use deadpan_store::generation_retention::{
    ClockAnomaly, DEFAULT_VARIANT_RETENTION, ExpiryMode, MAX_AUTOMATIC_EXPIRY, VariantEviction,
    VariantRetention,
};
use deadpan_store::storage::{CleanupPolicy, EntryState};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn retention(store: &ProjectStore, identity: &MessageIdentity) -> Result<VariantRetention> {
    Ok(store
        .generation_variant_retention(&identity.request_id)?
        .into_iter()
        .find(|record| &record.identity == identity)
        .ok_or("no retention record")?)
}

fn offered(store: &ProjectStore, identity: &MessageIdentity) -> Result<bool> {
    let attempt = store
        .generation_attempt(identity)?
        .ok_or("attempt disappeared")?;
    Ok(attempt.checkpoint.state == JobState::Ready
        && attempt
            .bundle_receipt
            .is_some_and(|receipt| receipt.availability() == CandidateAvailability::Present))
}

fn expired_ids(store: &mut ProjectStore, now: SystemTime, dry_run: bool) -> Result<Vec<String>> {
    Ok(store
        .expire_generation_variants(
            now,
            DEFAULT_VARIANT_RETENTION,
            ExpiryMode::Explicit,
            dry_run,
        )?
        .expired
        .into_iter()
        .map(|variant| variant.attempt_id)
        .collect())
}

#[test]
fn kept_and_selected_variants_never_expire_and_keep_survives_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("retention.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let before = SystemTime::now();
    let (kept, _, kept_bytes) = distinct_variant(&mut store, &request, "attempt-1", 1, "kept")?;
    let (plain, _, plain_bytes) = distinct_variant(&mut store, &request, "attempt-2", 2, "plain")?;
    // The newest Ready variant is the request's selection.
    let (selected, _, selected_bytes) =
        distinct_variant(&mut store, &request, "attempt-3", 3, "selected")?;
    let after = SystemTime::now();
    let record = retention(&store, &plain)?;
    assert!(record.ready_at >= before - Duration::from_millis(1) && record.ready_at <= after);
    assert!(!record.kept && !record.accepted && record.eviction.is_none());
    assert_eq!(
        record.expires_at(DEFAULT_VARIANT_RETENTION),
        record.ready_at.checked_add(DEFAULT_VARIANT_RETENTION)
    );

    assert_eq!(
        store.keep_generation_bundle_variant(&kept, true)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.keep_generation_bundle_variant(&kept, true)?,
        AttemptMutationOutcome::Duplicate
    );
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let record = retention(&store, &kept)?;
    assert!(record.kept, "keep is durable");
    assert_eq!(record.expires_at(DEFAULT_VARIANT_RETENTION), None);

    let report = store.storage_report(Duration::ZERO)?.variant_retention;
    assert_eq!(
        report.retention_seconds,
        DEFAULT_VARIANT_RETENTION.as_secs()
    );
    assert_eq!(
        (
            report.offered,
            report.kept,
            report.selected,
            report.expiring,
            report.due
        ),
        (3, 1, 1, 1, 0)
    );
    assert!(report.soonest_expiry_unix_seconds.is_some());

    // Nothing is due before the period ends.
    assert!(expired_ids(&mut store, after + 6 * DAY, false)?.is_empty());
    let later = after + DEFAULT_VARIANT_RETENTION + DAY;
    // A dry run lists the same variant and writes nothing.
    assert_eq!(expired_ids(&mut store, later, true)?, vec!["attempt-2"]);
    assert!(offered(&store, &plain)?);
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-2"]);
    assert!(
        expired_ids(&mut store, later, false)?.is_empty(),
        "idempotent"
    );
    assert!(!offered(&store, &plain)?);
    assert!(offered(&store, &kept)? && offered(&store, &selected)?);
    assert!(matches!(
        retention(&store, &plain)?.eviction,
        Some((VariantEviction::Expired, _))
    ));
    // An expired variant can no longer be kept, selected or accepted.
    assert!(store.keep_generation_bundle_variant(&plain, true).is_err());
    assert!(store.select_generation_bundle_variant(&plain).is_err());

    // Its receipt no longer pins its objects; the others stay pinned.
    let full = store.storage_report(Duration::ZERO)?;
    for bytes in &plain_bytes {
        assert_eq!(state(&full, bytes), EntryState::Unreferenced);
    }
    for bytes in kept_bytes.iter().chain(&selected_bytes) {
        assert!(matches!(state(&full, bytes), EntryState::Referenced { .. }));
    }
    let report = full.variant_retention;
    assert_eq!(
        (
            report.offered,
            report.expiring,
            report.expired,
            report.discarded
        ),
        (2, 0, 1, 0)
    );
    assert_eq!(report.evicted_awaiting_cleanup, 1);
    assert_eq!(
        report.evicted_awaiting_cleanup_bytes,
        plain_bytes
            .iter()
            .map(|bytes| bytes.len() as u64)
            .sum::<u64>()
    );

    // The automatic cleanup scope touches only unreferenced generated media.
    let removed = store.clean_storage(CleanupPolicy::generated_only(Duration::ZERO, false))?;
    let mut names: Vec<_> = removed
        .removed
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    let mut expected: Vec<_> = plain_bytes
        .iter()
        .map(|bytes| format!("blake3-{}", object(bytes).content().digest()))
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    assert!(
        removed
            .removed
            .iter()
            .all(|entry| entry.namespace == "generated")
    );
    let report = store.storage_report(Duration::ZERO)?.variant_retention;
    assert_eq!(report.evicted_awaiting_cleanup, 0);

    // Un-keeping makes the variant expirable again.
    assert_eq!(
        store.keep_generation_bundle_variant(&kept, false)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-1"]);
    assert!(offered(&store, &selected)?, "the selection never expires");
    // A discard keeps its own reason.
    store.discard_generation_bundle_variant(&selected)?;
    assert!(matches!(
        retention(&store, &selected)?.eviction,
        Some((VariantEviction::Discarded, _))
    ));
    store.validate_full()?;
    Ok(())
}

#[test]
fn accepted_variants_never_expire_and_their_media_stays() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("accepted.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (accepted, accepted_receipt, accepted_bytes) =
        distinct_variant(&mut store, &request, "attempt-1", 1, "accepted")?;
    let (other, _, other_bytes) = distinct_variant(&mut store, &request, "attempt-2", 2, "other")?;
    let (newest, _, _) = distinct_variant(&mut store, &request, "attempt-3", 3, "newest")?;
    store.select_generation_bundle_variant(&accepted)?;
    let input = GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity: accepted.clone(),
        expected_receipt: accepted_receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    assert!(retention(&store, &accepted)?.accepted);
    assert_eq!(
        retention(&store, &accepted)?.expires_at(DEFAULT_VARIANT_RETENTION),
        None
    );
    let undo = RevisionId::new("undo-acceptance")?;
    store.undo_reconciled(
        &RevisionId::new("accepted")?,
        undo.clone(),
        &unchanged_relevance(&store, &undo)?,
    )?;
    // Undo leaves the operational flag: history still names the variant.
    assert!(retention(&store, &accepted)?.accepted);
    assert!(offered(&store, &accepted)?);
    store.select_generation_bundle_variant(&newest)?;

    let later = SystemTime::now() + DEFAULT_VARIANT_RETENTION + DAY;
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-2"]);
    assert!(offered(&store, &accepted)? && offered(&store, &newest)?);
    assert!(!offered(&store, &other)?);

    // Without the flag (an acceptance it did not record), the reference
    // scan still recognises the media history names.
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "UPDATE generation_variant_retention SET accepted=0 WHERE attempt_id='attempt-1'",
        [],
    )?;
    drop(database);
    store.select_generation_bundle_variant(&newest)?;
    let expiry = store.expire_generation_variants(
        later,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Explicit,
        false,
    )?;
    assert!(expiry.expired.is_empty());
    assert_eq!(expiry.skipped_accepted, 1);
    assert!(offered(&store, &accepted)?);
    // The finding is recorded, so later passes skip it without scanning.
    assert!(retention(&store, &accepted)?.named_elsewhere);
    let again = store.expire_generation_variants(
        later,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Explicit,
        false,
    )?;
    assert_eq!((again.expired.len(), again.skipped_accepted), (0, 0));

    // Cleanup removes only the expired variant's masters; the accepted
    // variant's objects stay referenced by history and on disk.
    store.clean_storage(CleanupPolicy::everything(Duration::ZERO, false))?;
    let report = store.storage_report(Duration::ZERO)?;
    for bytes in &other_bytes {
        assert_eq!(state(&report, bytes), EntryState::Unexpected, "removed");
    }
    for bytes in &accepted_bytes {
        assert!(matches!(
            state(&report, bytes),
            EntryState::Referenced { .. }
        ));
    }
    store.validate_full()?;
    // Redo still admits the accepted pictures.
    let redo = RevisionId::new("redo-acceptance")?;
    store.redo_reconciled(&undo, redo.clone(), &unchanged_relevance(&store, &redo)?)?;
    store.validate_full()?;
    Ok(())
}

/// Retention states survive closing and reopening the current schema.
#[test]
fn variant_retention_survives_reopening() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().canonicalize()?.join("previous.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (accepted, accepted_receipt, _) =
        distinct_variant(&mut store, &request, "attempt-1", 1, "accepted")?;
    let (discarded, _, _) = distinct_variant(&mut store, &request, "attempt-2", 2, "discarded")?;
    let (plain, _, _) = distinct_variant(&mut store, &request, "attempt-3", 3, "plain")?;
    store.select_generation_bundle_variant(&accepted)?;
    let input = GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity: accepted.clone(),
        expected_receipt: accepted_receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    store.discard_generation_bundle_variant(&discarded)?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let record = retention(&store, &plain)?;
    assert!(!record.kept && !record.accepted && record.eviction.is_none());
    assert!(retention(&store, &accepted)?.accepted);
    assert!(matches!(
        retention(&store, &discarded)?.eviction,
        Some((VariantEviction::Discarded, _))
    ));
    store.validate_full()?;
    Ok(())
}

/// Automatic expiry trusts the wall clock only near the recorded times:
/// a clock behind them, before 1970, or more than one period past the last
/// pass defers; explicit cleanup confirms the clock; regular passes expire.
#[test]
fn automatic_expiry_defers_on_clock_anomalies_and_explicit_cleanup_confirms() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("clock.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (old, _, old_bytes) = distinct_variant(&mut store, &request, "attempt-1", 1, "old")?;
    distinct_variant(&mut store, &request, "attempt-2", 2, "selected")?;
    let ready = retention(&store, &old)?.ready_at;
    let automatic = |store: &mut ProjectStore, now: SystemTime| {
        store.expire_generation_variants(
            now,
            DEFAULT_VARIANT_RETENTION,
            ExpiryMode::Automatic,
            false,
        )
    };

    // Behind the newest recorded time, or before 1970: nothing happens.
    let behind = automatic(&mut store, ready - 2 * 60 * 60 * Duration::from_secs(1))?;
    assert!(matches!(
        behind.clock_anomaly,
        Some(ClockAnomaly::Behind { .. })
    ));
    let pre_epoch = automatic(&mut store, std::time::UNIX_EPOCH - DAY)?;
    assert!(matches!(
        pre_epoch.clock_anomaly,
        Some(ClockAnomaly::Behind { .. })
    ));
    // An explicit cleanup with a pre-1970 clock expires nothing and fails nothing.
    let explicit = store.expire_generation_variants(
        std::time::UNIX_EPOCH - DAY,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Explicit,
        false,
    )?;
    assert!(explicit.expired.is_empty());

    // Eight days later with no pass in between looks like a clock set ahead.
    let ahead = automatic(&mut store, ready + 8 * DAY)?;
    assert!(matches!(
        ahead.clock_anomaly,
        Some(ClockAnomaly::Ahead { .. })
    ));
    assert!(ahead.expired.is_empty() && offered(&store, &old)?);
    let report = store.storage_report(Duration::ZERO)?.variant_retention;
    assert!(
        report.clock_anomaly.is_none(),
        "the report checks the real clock"
    );
    assert_eq!(report.last_pass_unix_seconds, None);

    // Regular passes keep the watermark current, so the policy proceeds.
    for day in [2, 4, 6] {
        let pass = automatic(&mut store, ready + day * DAY)?;
        assert!(pass.clock_anomaly.is_none() && pass.expired.is_empty());
    }
    let dry = store.expire_generation_variants(
        ready + 8 * DAY,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Automatic,
        true,
    )?;
    let bytes: u64 = old_bytes.iter().map(|bytes| bytes.len() as u64).sum();
    assert_eq!((dry.expired.len(), dry.expired_bytes), (1, bytes));
    assert!(offered(&store, &old)?, "a dry run writes nothing");
    let pass = automatic(&mut store, ready + 8 * DAY)?;
    assert_eq!(pass.expired.len(), 1);
    assert!(!offered(&store, &old)?);
    Ok(())
}

/// After a long absence, explicit cleanup confirms the clock and expires.
#[test]
fn explicit_cleanup_expires_after_a_suspected_clock_jump() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("absent.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (old, _, _) = distinct_variant(&mut store, &request, "attempt-1", 1, "old")?;
    distinct_variant(&mut store, &request, "attempt-2", 2, "selected")?;
    let later = retention(&store, &old)?.ready_at + 30 * DAY;
    assert!(
        store
            .expire_generation_variants(
                later,
                DEFAULT_VARIANT_RETENTION,
                ExpiryMode::Automatic,
                false
            )?
            .clock_anomaly
            .is_some()
    );
    assert!(matches!(
        store.generation_retention_clock(later, DEFAULT_VARIANT_RETENTION)?,
        Some(ClockAnomaly::Ahead { .. })
    ));
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-1"]);
    assert_eq!(
        store.generation_retention_clock(later, DEFAULT_VARIANT_RETENTION)?,
        None
    );
    // The explicit pass set the watermark, so automatic passes trust the clock.
    let next = store.expire_generation_variants(
        later + DAY,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Automatic,
        false,
    )?;
    assert!(next.clock_anomaly.is_none());
    Ok(())
}

#[test]
fn an_automatic_pass_expires_at_most_its_cap() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cap.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let count = MAX_AUTOMATIC_EXPIRY as u64 + 2;
    for ordinal in 1..=count {
        distinct_variant(
            &mut store,
            &request,
            &format!("attempt-{ordinal}"),
            ordinal,
            &format!("v{ordinal}"),
        )?;
    }
    let start = SystemTime::now();
    for day in [3, 6] {
        store.expire_generation_variants(
            start + day * DAY,
            DEFAULT_VARIANT_RETENTION,
            ExpiryMode::Automatic,
            false,
        )?;
    }
    // Every variant but the selected newest is due.
    let first = store.expire_generation_variants(
        start + 8 * DAY,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Automatic,
        false,
    )?;
    assert_eq!(first.expired.len(), MAX_AUTOMATIC_EXPIRY);
    assert_eq!(first.deferred_by_cap, 1);
    let second = store.expire_generation_variants(
        start + 8 * DAY,
        DEFAULT_VARIANT_RETENTION,
        ExpiryMode::Automatic,
        false,
    )?;
    assert_eq!((second.expired.len(), second.deferred_by_cap), (1, 0));
    Ok(())
}

/// The person's explicit pick stays protected when a later variant takes
/// the selection, until they pick another or discard it.
#[test]
fn an_explicit_pick_survives_a_later_variant_taking_the_selection() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("picked.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (picked, _, _) = distinct_variant(&mut store, &request, "attempt-1", 1, "picked")?;
    let (plain, _, _) = distinct_variant(&mut store, &request, "attempt-2", 2, "plain")?;
    store.select_generation_bundle_variant(&picked)?;
    assert!(retention(&store, &picked)?.picked);
    let (newest, _, _) = distinct_variant(&mut store, &request, "attempt-3", 3, "newest")?;
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .map(|selected| selected.identity),
        Some(newest.clone()),
        "the new variant took the selection"
    );
    assert_eq!(
        retention(&store, &picked)?.expires_at(DEFAULT_VARIANT_RETENTION),
        None
    );
    let later = SystemTime::now() + 8 * DAY;
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-2"]);
    assert!(offered(&store, &picked)? && !offered(&store, &plain)?);
    // Picking another variant moves the protection.
    store.select_generation_bundle_variant(&newest)?;
    assert!(!retention(&store, &picked)?.picked && retention(&store, &newest)?.picked);
    assert_eq!(expired_ids(&mut store, later, false)?, vec!["attempt-1"]);
    store.validate_full()?;
    Ok(())
}

/// A plan from a read-only open is applied on the writer with a recheck: a
/// variant kept in between is not expired.
#[test]
fn a_read_only_plan_is_rechecked_on_the_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("plan.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (first, _, _) = distinct_variant(&mut store, &request, "attempt-1", 1, "first")?;
    let (second, _, _) = distinct_variant(&mut store, &request, "attempt-2", 2, "second")?;
    distinct_variant(&mut store, &request, "attempt-3", 3, "newest")?;
    let later = SystemTime::now() + 8 * DAY;
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let plan =
        reader.plan_generation_expiry(later, DEFAULT_VARIANT_RETENTION, ExpiryMode::Explicit)?;
    assert!(!plan.is_empty());
    let preview = plan.preview();
    assert!(preview.dry_run);
    assert_eq!(
        preview
            .expired
            .iter()
            .map(|variant| variant.attempt_id.clone())
            .collect::<Vec<_>>(),
        vec!["attempt-1", "attempt-2"]
    );
    store.keep_generation_bundle_variant(&first, true)?;
    let applied = store.apply_generation_expiry(&plan, false)?;
    assert_eq!(
        applied
            .expired
            .iter()
            .map(|variant| variant.attempt_id.clone())
            .collect::<Vec<_>>(),
        vec!["attempt-2"]
    );
    assert!(offered(&store, &first)? && !offered(&store, &second)?);
    // A plan of another package is refused.
    let other_package = scratch.path().join("other.deadpan");
    let mut other = ProjectStore::create(&other_package, &document()?)?;
    assert!(other.apply_generation_expiry(&plan, true).is_err());
    Ok(())
}

/// Acceptance refuses to commit when its variant has no retention record.
#[test]
fn acceptance_requires_the_retention_record() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("record.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (variant, receipt, _) = distinct_variant(&mut store, &request, "attempt-1", 1, "one")?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute("DELETE FROM generation_variant_retention", [])?;
    drop(database);
    let input = GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity: variant,
        expected_receipt: receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    let before = store.snapshot()?;
    assert!(
        store
            .accept_generation_bundle(
                &input,
                &unchanged_relevance(&store, &input.new_revision)?,
                media_limits(),
            )
            .is_err()
    );
    assert_eq!(store.snapshot()?, before, "nothing committed");
    Ok(())
}
