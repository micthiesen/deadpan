//! Process kills at random points (Gate G crash suite, docs/ADVERSARIAL.md).
//!
//! A child process re-executes this test binary and keeps a writer busy with
//! the operations whose durability matters: edit commits, AI generation
//! attempt state changes, verified backups (with their rotation) and
//! database checkpoints. The parent SIGKILLs it after a seeded random delay,
//! then reopens the package and checks that:
//!
//! - opening succeeds and validates the history,
//! - every commit the child reported as returned is in the history, and the
//!   head is the last reported commit or at most one beyond it (the commit
//!   whose report the kill cut off),
//! - writable reopening leaves no generation attempt nonterminal,
//! - every published backup verifies completely and every published
//!   checkpoint passes SQLite's integrity check (hidden staging files may
//!   remain; they are never published),
//! - the complete history replays (`validate_full`) after the last kill.
//!
//! The same package is killed repeatedly, so damage would accumulate. The
//! regression run uses a fixed seed and 8 kills; `DEADPAN_CHAOS_SEED` (hex)
//! and `DEADPAN_CHAOS_ITERATIONS` widen it. A process kill is not a power
//! loss: the kernel still writes back the page cache.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_chaos::Rng;
use deadpan_core::*;
use deadpan_jobs::{
    AttemptId, CancellationToken, ConditioningMode, HoldConstraints, MessageIdentity, MotionAmount,
    ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection, RequestId, RuntimeId,
    RuntimeVersion, Sha256, VideoSpec, WorkerMessage, WorkerStage,
};
use deadpan_store::backups::{
    BackupLimits, BackupPolicy, BackupReason, create_backup, list_backups, verify_backup,
};
use deadpan_store::generation::GenerationRequestInput;
use deadpan_store::generation_attempts::BeginGenerationAttempt;
use deadpan_store::{AccessMode, ProjectStore};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const CHILD: &str = "DEADPAN_TEST_CHAOS_KILL_CHILD";
const LOG: &str = "DEADPAN_TEST_CHAOS_KILL_LOG";
const ROUND: &str = "DEADPAN_TEST_CHAOS_KILL_ROUND";
const ATTEMPTS: &str = "DEADPAN_TEST_CHAOS_KILL_ATTEMPTS";

fn rate() -> FrameRate {
    FrameRate::new(30, 1).expect("rate")
}

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("chaos")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

const HOLDS: usize = 240;

/// A package of pauses for AI attempts: requests pin relevance, so its
/// document never changes while the attempts run.
fn holds_document() -> Result<ProjectDocument> {
    let mut document = document()?;
    for index in 0..HOLDS {
        let node = NodeId::new(format!("hold-{index}"))?;
        let edit = deadpan_core::apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("setup-{index}"))?,
                command: Command::Insert {
                    parent: document.root().clone(),
                    index,
                    subtree: Subtree {
                        root: node.clone(),
                        nodes: BTreeMap::from([(
                            node,
                            BeatNode::hold(
                                "Pause",
                                HoldRecipe {
                                    picture_context: None,
                                    duration: FrameDuration::new(12)?,
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                },
                            ),
                        )]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            },
        )?;
        document = edit.forward.apply(&document)?;
    }
    Ok(document)
}

fn insert(store: &mut ProjectStore, revision: &str, node: &str) -> Result<RevisionId> {
    let current = store.snapshot()?;
    let id = NodeId::new(node)?;
    Ok(store
        .commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command: Command::Insert {
                parent: current.root().clone(),
                index: 0,
                subtree: Subtree {
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                    root: id.clone(),
                    nodes: BTreeMap::from([(
                        id,
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(12)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                },
            },
        })?
        .revision_id)
}

/// Start an AI attempt on a pause and advance it partway.
fn attempt(
    store: &mut ProjectStore,
    hold: &str,
    request: &str,
    step: u64,
) -> Result<MessageIdentity> {
    let allocated = store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new(request)?,
        expected_revision: store.snapshot()?.revision_id().clone(),
        hold_id: NodeId::new(hold)?,
        context_sha256: Sha256::new("c".repeat(64))?,
        constraints: HoldConstraints {
            video: VideoSpec::new(FrameDuration::new(12)?, rate(), 512, 320)?,
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: None,
        },
        provider: ProviderSelection {
            pack_id: ProviderPackId::new("pack")?,
            pack_version: ProviderPackVersion::new("v1")?,
            runtime_id: RuntimeId::new("runtime")?,
            runtime_version: RuntimeVersion::new("v1")?,
            seed: step,
        },
    })?;
    let identity = MessageIdentity::new(allocated.request_id.clone(), AttemptId::new("attempt")?);
    store.begin_generation_attempt(BeginGenerationAttempt {
        identity: identity.clone(),
        cancellation_token: CancellationToken::new("cancel")?,
    })?;
    let stages = [
        WorkerStage::Preflight,
        WorkerStage::ModelLoading,
        WorkerStage::Inference,
    ];
    for stage in stages.iter().take((step % 4) as usize) {
        store.record_generation_worker_message(&WorkerMessage::Stage {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            stage: *stage,
        })?;
    }
    Ok(identity)
}

/// The child: keep the writer busy until killed. Each completed operation
/// is reported on its own line only after it returned.
fn child(path: &Path, attempts_path: &Path, log: &Path, round: u64) -> Result {
    let mut report = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite)?;
    let mut attempts = ProjectStore::open(attempts_path, AccessMode::ReadWrite)?;
    let mut next_hold = (round as usize * 64) % HOLDS;
    let cancelled = AtomicBool::new(false);
    let policy = BackupPolicy {
        keep_recent: 3,
        hourly_hours: 0,
        daily_days: 0,
        weekly_weeks: 0,
        keep_safety: 0,
        max_count: 4,
        ..BackupPolicy::default()
    };
    for step in 0u64.. {
        let name = format!("c{round}-{step}");
        writeln!(report, "attempt-commit {name}")?;
        let revision = insert(&mut store, &name, &format!("pause-{name}"))?;
        writeln!(report, "commit {revision}")?;
        match step % 4 {
            1 => {
                // Each request needs a pause without a current request.
                if next_hold < HOLDS {
                    attempt(
                        &mut attempts,
                        &format!("hold-{next_hold}"),
                        &format!("request-{name}"),
                        step,
                    )?;
                    next_hold += 1;
                    writeln!(report, "generation request-{name}")?;
                }
            }
            2 => {
                create_backup(
                    path,
                    BackupReason::Periodic,
                    &policy,
                    BackupLimits {
                        pages_per_step: 1,
                        ..BackupLimits::default()
                    },
                    &cancelled,
                )?;
                writeln!(report, "backup")?;
            }
            3 => {
                store.checkpoint()?;
                writeln!(report, "checkpoint")?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn seed() -> u64 {
    std::env::var("DEADPAN_CHAOS_SEED")
        .ok()
        .and_then(|text| u64::from_str_radix(text.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x00de_adba_c0ff_ee01)
}

fn iterations() -> u64 {
    std::env::var("DEADPAN_CHAOS_ITERATIONS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(8)
}

/// SIGKILL can interrupt any write in `writeln!`, including between the
/// record prefix and its identity. Only a newline acknowledges a complete
/// report. A torn commit report is covered by its preceding attempt report.
fn complete_reports(log: &str) -> impl Iterator<Item = &str> {
    log.split_inclusive('\n')
        .filter_map(|line| line.strip_suffix('\n'))
}

#[test]
fn a_kill_at_any_report_byte_preserves_only_complete_acknowledgements() -> Result {
    let records = [
        "attempt-commit c0-1\n",
        "commit c0-1\n",
        "generation request-c0-1\n",
        "backup\n",
        "checkpoint\n",
    ];
    let log = records.concat();
    for cut in 0..=log.len() {
        let prefix = &log[..cut];
        let reports = complete_reports(prefix).collect::<Vec<_>>();
        let expected = prefix.bytes().filter(|byte| *byte == b'\n').count();
        assert_eq!(reports.len(), expected, "cut at byte {cut}");
        for (line, original) in reports.iter().zip(&records) {
            assert_eq!(*line, original.trim_end(), "cut at byte {cut}");
            if let Some(revision) = line
                .strip_prefix("attempt-commit ")
                .or_else(|| line.strip_prefix("commit "))
            {
                RevisionId::new(revision)?;
            } else if let Some(request) = line.strip_prefix("generation ") {
                RequestId::new(request)?;
            }
        }
    }
    // A complete malformed report still reaches validation and fails.
    let bad = complete_reports("commit \n").next().unwrap();
    assert!(RevisionId::new(bad.strip_prefix("commit ").unwrap()).is_err());
    Ok(())
}

#[test]
fn random_process_kills_during_commits_attempts_backups_and_checkpoints_never_corrupt_the_project()
-> Result {
    if let Some(path) = std::env::var_os(CHILD) {
        let log = PathBuf::from(std::env::var_os(LOG).ok_or("missing log")?);
        let attempts = PathBuf::from(std::env::var_os(ATTEMPTS).ok_or("missing attempts")?);
        let round = std::env::var(ROUND)?.parse()?;
        return child(Path::new(&path), &attempts, &log, round);
    }
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().canonicalize()?.join("chaos.deadpan");
    let attempts_path = scratch.path().canonicalize()?.join("attempts.deadpan");
    drop(ProjectStore::create(&path, &document()?)?);
    drop(ProjectStore::create(&attempts_path, &holds_document()?)?);
    let mut rng = Rng::new(seed());
    let mut committed = vec![RevisionId::new("r0")?];
    let mut generations = Vec::new();
    let mut summary = Vec::new();
    for round in 0..iterations() {
        let log = scratch.path().join(format!("round-{round}.log"));
        let mut child = ProcessCommand::new(std::env::current_exe()?)
            .args([
                "--exact",
                "random_process_kills_during_commits_attempts_backups_and_checkpoints_never_corrupt_the_project",
                "--nocapture",
            ])
            .env(CHILD, &path)
            .env(ATTEMPTS, &attempts_path)
            .env(LOG, &log)
            .env(ROUND, round.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let delay = 40 + rng.below(700) as u64;
        std::thread::sleep(Duration::from_millis(delay));
        let exited_early = child.try_wait()?;
        child.kill()?;
        let output = child.wait_with_output()?;
        if let Some(status) = exited_early {
            return Err(format!(
                "round {round}: the child exited before the kill ({status}): {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let lines = std::fs::read_to_string(&log).unwrap_or_default();
        let mut attempted = None;
        let mut operations = 0;
        for line in complete_reports(&lines) {
            operations += 1;
            if let Some(name) = line.strip_prefix("attempt-commit ") {
                attempted = Some(RevisionId::new(name)?);
            } else if let Some(revision) = line.strip_prefix("commit ") {
                committed.push(RevisionId::new(revision)?);
                attempted = None;
            } else if let Some(request) = line.strip_prefix("generation ") {
                generations.push(RequestId::new(request)?);
            }
        }
        summary.push(format!(
            "round {round}: killed after {delay} ms, {operations} reports"
        ));

        // Reopening validates and recovers; the head is the last reported
        // commit or the one the kill cut off before its report.
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)
            .map_err(|error| format!("round {round}: reopening failed: {error}"))?;
        let head = store.head_revision()?;
        let last = committed.last().expect("initial revision");
        assert!(
            head == *last || attempted.as_ref() == Some(&head),
            "round {round}: head {head} is neither the last reported commit {last} nor the cut-off {attempted:?}"
        );
        if attempted.as_ref() == Some(&head) {
            committed.push(head.clone());
        }
        for revision in &committed {
            store
                .snapshot_at(revision)
                .map_err(|error| format!("round {round}: revision {revision} lost: {error}"))?;
        }
        let attempts = ProjectStore::open(&attempts_path, AccessMode::ReadWrite)
            .map_err(|error| format!("round {round}: reopening the AI package failed: {error}"))?;
        for request in &generations {
            let identity = MessageIdentity::new(request.clone(), AttemptId::new("attempt")?);
            let attempt = attempts
                .generation_attempt(&identity)?
                .ok_or_else(|| format!("round {round}: reported attempt {request} is missing"))?;
            assert!(
                attempt.checkpoint.state.is_terminal(),
                "round {round}: attempt {request} still running after a writable reopen"
            );
        }
        drop(attempts);
        for backup in list_backups(&path)? {
            verify_backup(&backup).map_err(|error| {
                format!("round {round}: backup {} is invalid: {error}", backup.id)
            })?;
        }
        for entry in std::fs::read_dir(path.join("Snapshots"))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("checkpoint-") {
                continue;
            }
            let connection = rusqlite::Connection::open_with_flags(
                entry.path(),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let integrity: String =
                connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            assert_eq!(
                integrity, "ok",
                "round {round}: checkpoint {name} is damaged"
            );
        }
        drop(store);
    }
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.validate_full()?;
    ProjectStore::open(&attempts_path, AccessMode::ReadWrite)?.validate_full()?;
    // At least some rounds reached the slower operations.
    assert!(committed.len() > iterations() as usize, "{summary:?}");
    eprintln!("{}", summary.join("\n"));
    Ok(())
}

const RESTORE_CHILD: &str = "DEADPAN_TEST_CHAOS_RESTORE_CHILD";

/// Kills while restoring backups back and forth: the live database is
/// always one of the two restored states or the pre-restore state, never a
/// mixture, and every published backup (including the automatic
/// `before-restore` ones) verifies.
#[test]
fn random_process_kills_during_restores_leave_one_whole_state() -> Result {
    if let Some(path) = std::env::var_os(RESTORE_CHILD) {
        let path = PathBuf::from(path);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        let ids: Vec<String> = std::env::var("DEADPAN_TEST_CHAOS_RESTORE_IDS")?
            .split(',')
            .map(str::to_owned)
            .collect();
        let policy = BackupPolicy {
            keep_safety: 2,
            ..BackupPolicy::default()
        };
        for step in 0usize.. {
            store.restore_backup(
                &ids[step % 2],
                &policy,
                BackupLimits::default(),
                &AtomicBool::new(false),
            )?;
        }
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().canonicalize()?.join("restore.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let first = insert(&mut store, "first", "pause-first")?;
    let backup = |path: &Path| {
        create_backup(
            path,
            BackupReason::Manual,
            &BackupPolicy::default(),
            BackupLimits::default(),
            &AtomicBool::new(false),
        )
    };
    let a = backup(&path)?;
    let second = insert(&mut store, "second", "pause-second")?;
    let b = backup(&path)?;
    drop(store);
    let heads = [first, second];
    let ids = format!("{},{}", a.backup.id, b.backup.id);
    let mut rng = Rng::new(seed() ^ 0x5eed);
    for round in 0..iterations() {
        let mut child = ProcessCommand::new(std::env::current_exe()?)
            .args([
                "--exact",
                "random_process_kills_during_restores_leave_one_whole_state",
                "--nocapture",
            ])
            .env(RESTORE_CHILD, &path)
            .env("DEADPAN_TEST_CHAOS_RESTORE_IDS", &ids)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        std::thread::sleep(Duration::from_millis(40 + rng.below(500) as u64));
        let early = child.try_wait()?;
        child.kill()?;
        let output = child.wait_with_output()?;
        if let Some(status) = early {
            return Err(format!(
                "round {round}: the child exited before the kill ({status}): {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)
            .map_err(|error| format!("round {round}: reopening failed: {error}"))?;
        let head = store.head_revision()?;
        assert!(heads.contains(&head), "round {round}: head {head}");
        for listed in list_backups(&path)? {
            verify_backup(&listed).map_err(|error| {
                format!("round {round}: backup {} is invalid: {error}", listed.id)
            })?;
        }
        store.validate_full()?;
    }
    Ok(())
}

const MIGRATE_CHILD: &str = "DEADPAN_TEST_CHAOS_MIGRATE_CHILD";

fn add_release_table(
    transaction: &rusqlite::Transaction<'_>,
) -> std::result::Result<(), deadpan_store::StoreError> {
    transaction.execute_batch("CREATE TABLE synthetic_release(id INTEGER PRIMARY KEY) STRICT;")?;
    Ok(())
}

fn accept(_: &rusqlite::Connection) -> std::result::Result<(), deadpan_store::StoreError> {
    Ok(())
}

/// Kills during a synthetic N to N+1 release migration: each package is
/// afterwards either the old schema, opening for writing with its history
/// intact, or the new one, which this build views read-only with the same
/// document. Never a mixture.
#[test]
fn random_process_kills_during_a_migration_leave_the_old_or_the_new_database() -> Result {
    let steps = [deadpan_store::migration::Migration {
        from: deadpan_store::DATABASE_SCHEMA_VERSION,
        apply: add_release_table,
    }];
    if let Some(path) = std::env::var_os(MIGRATE_CHILD) {
        deadpan_store::migration::migrate_package_with(
            Path::new(&path),
            &steps,
            deadpan_store::DATABASE_SCHEMA_VERSION + 1,
            accept,
        )?;
        // Finished before the kill: wait to be killed like the others.
        std::thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let pristine = root.join("pristine.deadpan");
    let mut store = ProjectStore::create(&pristine, &document()?)?;
    for index in 0..40 {
        insert(&mut store, &format!("m{index}"), &format!("pause-m{index}"))?;
    }
    let expected = store.snapshot()?;
    drop(store);
    let mut rng = Rng::new(seed() ^ 0x0419);
    let (mut old, mut new) = (0, 0);
    for round in 0..iterations() {
        let path = root.join(format!("round-{round}.deadpan"));
        copy_package(&pristine, &path)?;
        let mut child = ProcessCommand::new(std::env::current_exe()?)
            .args([
                "--exact",
                "random_process_kills_during_a_migration_leave_the_old_or_the_new_database",
                "--nocapture",
            ])
            .env(MIGRATE_CHILD, &path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        std::thread::sleep(Duration::from_millis(5 + rng.below(250) as u64));
        child.kill()?;
        let _ = child.wait_with_output()?;
        match ProjectStore::open(&path, AccessMode::ReadWrite) {
            Ok(store) => {
                old += 1;
                assert_eq!(store.snapshot()?, expected, "round {round}");
                store.validate_full()?;
            }
            Err(deadpan_store::StoreError::NewerSchema { .. }) => {
                new += 1;
                let viewer = ProjectStore::open(&path, AccessMode::ReadOnly)?;
                assert_eq!(viewer.snapshot()?, expected, "round {round}");
            }
            Err(error) => {
                let listing: Vec<String> = std::fs::read_dir(&path)?
                    .flatten()
                    .map(|entry| {
                        format!(
                            "{} {:?}",
                            entry.file_name().to_string_lossy(),
                            entry.metadata().map(|metadata| metadata.len()).ok()
                        )
                    })
                    .collect();
                return Err(format!("round {round}: {error:?}; package: {listing:?}").into());
            }
        }
        for listed in list_backups(&path)? {
            // Raw pre-migration backups hold the old schema intact.
            let connection = rusqlite::Connection::open_with_flags(
                &listed.path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let integrity: String =
                connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            assert_eq!(integrity, "ok", "round {round}");
        }
    }
    eprintln!("migration kills: {old} left the old database, {new} the new one");
    Ok(())
}

fn copy_package(source: &Path, destination: &Path) -> Result {
    std::fs::create_dir(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".writer") {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_package(&entry.path(), &destination.join(&*name))?;
        } else {
            std::fs::copy(entry.path(), destination.join(&*name))?;
        }
    }
    Ok(())
}
