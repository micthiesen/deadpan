#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Media qualification uses the existing real decoded fixture. Transcript
//! words are synthetic annotations; SQL writes below are corruption witnesses.

use deadpan_analysis::{AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word};
use deadpan_core::*;
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, CorrectionChange, CorrectionsKey, ProjectStore, TranscriptKey,
    TranscriptSelectionCorrections, TranscriptSelectionGuard,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    single_source::SingleSourceInitialization,
    source_registration::PreparedSourceRegistration,
};
use rusqlite::{Connection, OpenFlags, params};
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
type Cells = Vec<(String, Vec<Vec<String>>)>;

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn clock(proposal: &Transcript) -> deadpan_analysis::CorrectionClock {
    Corrections::clock_of(proposal)
}
fn correction_key(key: &TranscriptKey) -> CorrectionsKey {
    CorrectionsKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
    }
}

fn create(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("guard.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("guard")?,
        revision("initial"),
        node("root"),
    )?;
    Ok((
        path.clone(),
        ProjectStore::create_single_source(&path, &document)?,
    ))
}

fn ready(parent: &Path) -> Result<(PathBuf, ProjectStore, TranscriptKey, Transcript)> {
    let (path, mut store) = create(parent)?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
        .canonicalize()?;
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let record = store
        .retain_original(&fixture, OriginalOwnership::Managed, limits, &active())?
        .record;
    let mut original =
        store
            .original_import_handle()?
            .snapshot_original(&record, limits, &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        asset("decode"),
        SourceSessionLimits::default(),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active())?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    let source = PreparedSourceRegistration::from_decoded(original, &decoded, &active())?;
    let measured = source.receipt().snapshot().audio().unwrap();
    let key = TranscriptKey {
        content: source.receipt().original().content().to_string(),
        audio_stream: measured.stream().stream_index,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "guard-test".into(),
    };
    let first = measured.frames().first().unwrap().valid_start;
    let last = measured.frames().last().unwrap().valid_end;
    let duration_cs = u32::try_from(
        (u64::try_from(last - first)? * 100).div_ceil(u64::from(measured.stream().sample_rate)),
    )?;
    let proposal = Transcript::new(
        AnalysedAudio {
            origin: first,
            sample_rate: measured.stream().sample_rate,
            duration_cs,
        },
        vec![
            Word {
                text: "hello".into(),
                start_cs: 1,
                end_cs: 3,
                probability: 0.9,
                segment: 0,
            },
            Word {
                text: "world".into(),
                start_cs: 4,
                end_cs: 7,
                probability: 0.8,
                segment: 0,
            },
        ],
    )?;
    store.initialize_prepared_source(
        &SingleSourceInitialization {
            expected_revision: revision("initial"),
            new_revision: revision("baseline"),
            new_asset_id: asset("original"),
            node: node("full-original"),
            label: "The original".into(),
        },
        &source,
        &active(),
    )?;
    store.save_transcript(&key, &proposal)?;
    Ok((path, store, key, proposal))
}

fn cells(path: &Path) -> Result<Cells> {
    let database = Connection::open_with_flags(
        path.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut output = Vec::new();
    for table in tables {
        let mut statement =
            database.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| Ok(format!("{:?}", row.get_ref(column)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.sort();
        output.push((table, rows));
    }
    Ok(output)
}

fn compound(store: &ProjectStore, id: &str, cut: bool) -> Result<CommandRequest> {
    let document = store.snapshot()?;
    let selected = FrameRange::new(ProjectFrame(1), ProjectFrame(2))?;
    let timing = AudioTimingId {
        allocation: revision(&format!("{id}-capture")),
        ordinal: 0,
    };
    let set = CapturedEditSet::capture_ranges(&document, document.root(), &[selected], timing)?;
    let mut steps = vec![ResolvedStep::Yank {
        name: RegisterName::new('a')?,
        value: Arc::new(RegisterValue::EditedSet { set: Arc::new(set) }),
    }];
    if cut {
        let required = document
            .range_deletion(document.root(), selected)?
            .required_ids;
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision(&format!("{id}-delete")),
                Command::DeleteRange {
                    parent: document.root().clone(),
                    range: selected,
                    identities: SplitIdentities {
                        nodes: (0..required)
                            .map(|index| node(&format!("{id}-split-{index}")))
                            .collect(),
                    },
                    timing: AudioTimingId {
                        allocation: revision(&format!("{id}-delete")),
                        ordinal: 0,
                    },
                },
            )?,
        });
    }
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(
                store.register_version()?,
                BTreeMap::new(),
                steps,
            )?,
        },
    })
}

fn reject_unchanged(
    path: &Path,
    store: &mut ProjectStore,
    guard: &TranscriptSelectionGuard,
) -> Result {
    // Snapshot after the intentional analysis change, before either failed attempt.
    let before = cells(path)?;
    for (id, cut) in [("stale-yank", false), ("stale-cut", true)] {
        let request = compound(store, id, cut)?;
        assert!(
            store
                .preview_compound_with_transcript_guard(&request, guard)
                .is_err()
        );
        assert!(
            store
                .commit_compound_with_transcript_guard(&request, None, guard)
                .is_err()
        );
        assert_eq!(cells(path)?, before);
    }
    let document = store.snapshot()?;
    assert!(
        store
            .validate_selection_at(
                document.project_id(),
                document.revision_id(),
                store.register_version()?,
                guard
            )
            .is_err()
    );
    assert_eq!(cells(path)?, before);
    Ok(())
}

fn replacement(proposal: &Transcript) -> Transcript {
    let mut words = proposal.words().to_vec();
    words[0].text = "hullo".into();
    Transcript::new(proposal.audio(), words).unwrap()
}

#[test]
fn same_key_replacement_rejects_both_compound_branches_without_any_sql_write() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let pinned = store.transcript_selection(&key)?;
    let head = store.head_revision()?;
    store.save_transcript(&key, &replacement(&proposal))?;
    assert_eq!(store.head_revision()?, head);
    assert_eq!(store.register_version()?, 0);
    reject_unchanged(&path, &mut store, &pinned.guard)?;
    let fresh = store.transcript_selection(&key)?;
    let request = compound(&store, "fresh-yank", false)?;
    let before = store.snapshot()?;
    let unchanged = cells(&path)?;
    assert!(
        store
            .preview_compound_with_transcript_guard(&request, &fresh.guard)?
            .edit
            .is_none()
    );
    assert_eq!(cells(&path)?, unchanged);
    assert!(
        store
            .commit_compound_with_transcript_guard(&request, None, &fresh.guard)?
            .committed
            .is_none()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.history_availability()?, (false, false));
    assert_eq!(store.register_version()?, 1);
    let mut other = key.clone();
    other.model_sha256 = "0".repeat(64);
    store.save_transcript(&other, &proposal)?;
    assert_eq!(store.transcript_selection(&key)?.guard, fresh.guard);
    let request = compound(&store, "fresh-cut", true)?;
    assert!(
        store
            .commit_compound_with_transcript_guard(&request, None, &fresh.guard)?
            .committed
            .is_some()
    );
    assert_eq!(
        store.snapshot()?.duration()?.frames(),
        before.duration()?.frames() - 1
    );
    assert_eq!(store.history_availability()?, (true, false));
    store.save_transcript(&key, &proposal)?;
    // Analysis authority does not leak into historical Undo or Redo.
    store.undo(&revision("fresh-cut"), revision("undo-cut"))?;
    store.redo(&revision("undo-cut"), revision("redo-cut"))?;
    assert_eq!(store.validate_report(true)?.revisions, 5);
    Ok(())
}

#[test]
fn correction_apply_undo_redo_versions_and_present_empty_each_invalidate_guard() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let absent = store.transcript_selection(&key)?.guard;
    assert_eq!(absent.corrections, TranscriptSelectionCorrections::Absent);
    let corrected = Corrections::empty(clock(&proposal)).edit_word_text(
        &CorrectedTranscript::recognized(proposal.clone()),
        0,
        "hullo",
        &[],
    )?;
    let applied = store.change_analysis_corrections(
        &correction_key(&key),
        0,
        CorrectionChange::Apply {
            corrections: corrected,
            label: "Correct hello".into(),
        },
    )?;
    assert_eq!(applied.version, 1);
    reject_unchanged(&path, &mut store, &absent)?;
    let at_apply = store.transcript_selection(&key)?.guard;
    let undone =
        store.change_analysis_corrections(&correction_key(&key), 1, CorrectionChange::Undo)?;
    assert_eq!(undone.version, 2);
    let at_undo = store.transcript_selection(&key)?.guard;
    assert!(matches!(
        at_undo.corrections,
        TranscriptSelectionCorrections::Present { version: 2, .. }
    ));
    assert_eq!(undone.corrections, Corrections::empty(clock(&proposal)));
    reject_unchanged(&path, &mut store, &at_apply)?;
    reject_unchanged(&path, &mut store, &absent)?;
    assert_eq!(
        store
            .change_analysis_corrections(&correction_key(&key), 2, CorrectionChange::Redo)?
            .version,
        3
    );
    reject_unchanged(&path, &mut store, &at_undo)?;
    reject_unchanged(&path, &mut store, &at_apply)?;
    let fresh = store.transcript_selection(&key)?.guard;
    let request = compound(&store, "corrected-yank", false)?;
    assert!(
        store
            .commit_compound_with_transcript_guard(&request, None, &fresh)?
            .committed
            .is_none()
    );
    Ok(())
}

#[test]
fn canonical_values_bind_digest_and_same_version_corruption_is_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    store.change_analysis_corrections(
        &correction_key(&key),
        0,
        CorrectionChange::Apply {
            corrections: Corrections::empty(clock(&proposal)),
            label: "Empty".into(),
        },
    )?;
    let pinned = store.transcript_selection(&key)?.guard;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute(
        "UPDATE transcripts SET value=?1",
        [serde_json::to_string_pretty(&proposal)?],
    )?;
    database.execute(
        "UPDATE analysis_corrections SET value=?1",
        [serde_json::to_string_pretty(&Corrections::empty(clock(
            &proposal,
        )))?],
    )?;
    assert_eq!(store.transcript_selection(&key)?.guard, pinned);
    let corrections = Corrections::empty(clock(&proposal)).edit_word_text(
        &CorrectedTranscript::recognized(proposal.clone()),
        0,
        "different",
        &[],
    )?;
    // Explicit corruption/repair witness: retain the version, change valid bytes.
    database.execute(
        "UPDATE analysis_corrections SET value=?1",
        [serde_json::to_string(&corrections)?],
    )?;
    assert_eq!(
        store
            .analysis_corrections(&correction_key(&key))?
            .unwrap()
            .version,
        1
    );
    reject_unchanged(&path, &mut store, &pinned)?;
    let repaired = store.transcript_selection(&key)?.guard;
    let mut mismatch = repaired.clone();
    mismatch.proposal_sha256 = "f".repeat(64);
    reject_unchanged(&path, &mut store, &mismatch)?;
    let encoded = serde_json::to_value(&repaired)?;
    for (field, value) in [
        ("format", serde_json::json!("unknown")),
        ("correction_rule", serde_json::json!("unknown")),
        ("proposal_sha256", serde_json::json!("BAD")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<TranscriptSelectionGuard>(invalid).is_err());
    }
    Ok(())
}

#[test]
fn unreadable_current_undo_redo_and_missing_proposal_never_become_absence() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let corrections = Corrections::empty(clock(&proposal));
    store.change_analysis_corrections(
        &correction_key(&key),
        0,
        CorrectionChange::Apply {
            corrections: corrections.clone(),
            label: "First".into(),
        },
    )?;
    let pinned = store.transcript_selection(&key)?.guard;
    let database = Connection::open(path.join("project.sqlite"))?;
    // Valid JSON with an unsupported typed shape bypasses SQL's syntax check.
    database.execute("UPDATE analysis_corrections SET value='{}'", [])?;
    assert!(store.transcript_selection(&key).is_err());
    reject_unchanged(&path, &mut store, &pinned)?;
    let repaired = store.change_analysis_corrections(
        &correction_key(&key),
        1,
        CorrectionChange::DiscardUnreadable {
            clock: clock(&proposal),
        },
    )?;
    assert_eq!(repaired.version, 2);
    reject_unchanged(&path, &mut store, &pinned)?;
    for stack in ["undo", "redo"] {
        let fresh = store.transcript_selection(&key)?.guard;
        database.execute("INSERT INTO analysis_correction_steps(content,audio_stream,stack,position,label,value) VALUES(?1,?2,?3,63,'Damaged','{}')", params![key.content, key.audio_stream, stack])?;
        assert!(store.transcript_selection(&key).is_err());
        reject_unchanged(&path, &mut store, &fresh)?;
        let version = store
            .analysis_corrections(&correction_key(&key))?
            .unwrap()
            .version;
        store.change_analysis_corrections(
            &correction_key(&key),
            version,
            CorrectionChange::DiscardUnreadable {
                clock: clock(&proposal),
            },
        )?;
    }
    let fresh = store.transcript_selection(&key)?.guard;
    database.execute("UPDATE transcripts SET value='{}'", [])?;
    reject_unchanged(&path, &mut store, &fresh)?;
    database.execute("DELETE FROM transcripts", [])?;
    assert!(store.transcript_selection(&key).is_err());
    reject_unchanged(&path, &mut store, &fresh)?;
    store.save_transcript(&key, &proposal)?;
    assert!(store.transcript_selection(&key).is_ok());
    Ok(())
}

#[test]
fn readonly_selection_receipt_checks_project_head_bank_and_analysis_together() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let guard = read_only.transcript_selection(&key)?.guard;
    let document = read_only.snapshot()?;
    let before = cells(&path)?;
    read_only.validate_selection_at(document.project_id(), document.revision_id(), 0, &guard)?;
    assert_eq!(cells(&path)?, before);
    assert!(
        read_only
            .validate_selection_at(&ProjectId::new("other")?, document.revision_id(), 0, &guard)
            .is_err()
    );
    store.save_transcript(&key, &replacement(&proposal))?;
    assert!(
        read_only
            .validate_selection_at(document.project_id(), document.revision_id(), 0, &guard)
            .is_err()
    );
    let guard = read_only.transcript_selection(&key)?.guard;
    store.change_analysis_corrections(
        &correction_key(&key),
        0,
        CorrectionChange::Apply {
            corrections: Corrections::empty(clock(&proposal)),
            label: "Empty".into(),
        },
    )?;
    assert!(
        read_only
            .validate_selection_at(document.project_id(), document.revision_id(), 0, &guard)
            .is_err()
    );
    let guard = read_only.transcript_selection(&key)?.guard;
    let request = compound(&store, "bank-only", false)?;
    store.commit_compound_with_transcript_guard(&request, None, &guard)?;
    assert!(
        read_only
            .validate_selection_at(document.project_id(), document.revision_id(), 0, &guard)
            .is_err()
    );
    let request = compound(&store, "head-change", true)?;
    store.commit_compound_with_transcript_guard(&request, None, &guard)?;
    assert!(
        read_only
            .validate_selection_at(document.project_id(), document.revision_id(), 2, &guard)
            .is_err()
    );
    let current = store.snapshot()?;
    let before = cells(&path)?;
    read_only.validate_selection_at(
        current.project_id(),
        current.revision_id(),
        store.register_version()?,
        &guard,
    )?;
    assert_eq!(cells(&path)?, before);
    Ok(())
}

#[test]
fn qualified_original_stream_and_ready_workflow_are_required_only_for_guarded_paths() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let mut wrong = key.clone();
    wrong.audio_stream += 1;
    store.save_transcript(&wrong, &proposal)?;
    assert!(store.transcript_selection(&wrong).is_err());
    let mut wrong_content = key.clone();
    wrong_content.content = "other".into();
    store.save_transcript(&wrong_content, &proposal)?;
    assert!(store.transcript_selection(&wrong_content).is_err());
    let pinned = store.transcript_selection(&key)?.guard;
    // An unrelated damaged correction stream cannot block the consumed key.
    store.change_analysis_corrections(
        &correction_key(&wrong),
        0,
        CorrectionChange::Apply {
            corrections: Corrections::empty(clock(&proposal)),
            label: "Other stream".into(),
        },
    )?;
    Connection::open(path.join("project.sqlite"))?.execute(
        "UPDATE analysis_corrections SET value='{}' WHERE audio_stream=?1",
        [wrong.audio_stream],
    )?;
    assert_eq!(store.transcript_selection(&key)?.guard, pinned);
    let request = compound(&store, "ordinary-yank", false)?;
    // No proposal is needed for an ordinary captured copy.
    Connection::open(path.join("project.sqlite"))?.execute("DELETE FROM transcripts", [])?;
    assert!(store.commit_compound(&request, None)?.committed.is_none());
    let other_scratch = tempfile::tempdir()?;
    let (_, awaiting) = create(other_scratch.path())?;
    awaiting.save_transcript(&key, &proposal)?;
    assert!(awaiting.transcript_selection(&key).is_err());
    Ok(())
}

#[test]
fn skipped_word_corrections_refuse_snapshot_and_recheck_without_partial_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = ready(scratch.path())?;
    let pinned = store.transcript_selection(&key)?.guard;
    let start_cs = proposal.audio().duration_cs + 1;
    let corrections = Corrections::new(
        clock(&proposal),
        vec![deadpan_analysis::WordCorrection {
            start_cs,
            end_cs: start_cs + 1,
            words: vec![deadpan_analysis::CorrectedWord {
                text: "late".into(),
                start_cs,
                end_cs: start_cs + 1,
            }],
        }],
        vec![],
    )?;
    store.change_analysis_corrections(
        &correction_key(&key),
        0,
        CorrectionChange::Apply {
            corrections,
            label: "Outside proposal".into(),
        },
    )?;
    let before = cells(&path)?;
    assert!(
        store
            .transcript_selection(&key)
            .unwrap_err()
            .to_string()
            .contains("skipped word regions")
    );
    assert_eq!(cells(&path)?, before);
    reject_unchanged(&path, &mut store, &pinned)?;
    Ok(())
}

#[test]
fn deleting_every_edit_beat_retains_the_protected_original_analysis_binding() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, _) = ready(scratch.path())?;
    let pinned = store.transcript_selection(&key)?.guard;
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("delete-all"),
        command: Command::DeleteRipple {
            node: node("full-original"),
            timing: AudioTimingId {
                allocation: revision("delete-all"),
                ordinal: 0,
            },
        },
    })?;
    assert_eq!(store.snapshot()?.duration()?, FrameDuration::ZERO);
    assert_eq!(store.transcript_selection(&key)?.guard, pinned);
    let current = store.snapshot()?;
    let before = cells(&path)?;
    store.validate_selection_at(current.project_id(), current.revision_id(), 0, &pinned)?;
    assert_eq!(cells(&path)?, before);
    Ok(())
}
