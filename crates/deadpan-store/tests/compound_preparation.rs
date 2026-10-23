//! Host preparation shares the authoritative Compound transaction. SQL triggers
//! below inject write failures; media authority cases also decode and admit the
//! existing bounded Original fixture before changing its stored authority.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use deadpan_analysis::{AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word};
use deadpan_core::*;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, CompoundPreparationError, ProjectStore, StoreError, TranscriptKey,
    TranscriptSelectionCorrectionRule, TranscriptSelectionCorrections, TranscriptSelectionFormat,
    TranscriptSelectionGuard,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use deadpan_store::{
    CorrectionChange, CorrectionsKey,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    single_source::SingleSourceInitialization,
    source_registration::PreparedSourceRegistration,
};
use rusqlite::{Connection, OpenFlags};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
type DatabaseCells = Vec<(String, Vec<Vec<String>>)>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn ordinary(document: &ProjectDocument, id: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command,
    }
}

fn create(path: &Path) -> Result<ProjectStore> {
    let document = ProjectDocument::new(
        ProjectId::new("prepared-compound")?,
        revision("empty"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(24, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let request = ordinary(
        &document,
        "initial",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode::hold(
                        "Initial Hold",
                        HoldRecipe {
                            duration: FrameDuration::new(12)?,
                            picture_context: None,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let transaction = apply(&document, &request)?;
    let document = transaction.forward.apply(&document)?;
    Ok(ProjectStore::create(path, &document)?)
}

fn compound(store: &ProjectStore, id: &str, authored: bool) -> Result<CommandRequest> {
    let document = store.snapshot()?;
    let set = CapturedEditSet::capture_ranges(
        &document,
        document.root(),
        &[FrameRange::new(ProjectFrame(1), ProjectFrame(3))?],
        AudioTimingId {
            allocation: revision(&format!("{id}-capture")),
            ordinal: 0,
        },
    )?;
    let mut steps = vec![ResolvedStep::Yank {
        name: RegisterName::new('a')?,
        value: Arc::new(RegisterValue::EditedSet { set: Arc::new(set) }),
    }];
    if authored {
        steps.push(ResolvedStep::Edit {
            edit: LeafEdit::new(
                revision(&format!("{id}-leaf")),
                Command::Rename {
                    node: node("hold"),
                    label: "Prepared edit".into(),
                },
            )?,
        });
    }
    Ok(ordinary(
        &document,
        id,
        Command::Compound {
            transaction: ResolvedTransaction::new(
                store.register_version()?,
                BTreeMap::new(),
                steps,
            )?,
        },
    ))
}

fn cells(path: &Path) -> Result<DatabaseCells> {
    let database = Connection::open_with_flags(
        path.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result = Vec::new();
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
        result.push((table, rows));
    }
    Ok(result)
}

#[test]
fn host_failure_rolls_back_authored_and_register_only_preparation_and_allows_retry() -> Result {
    for authored in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("failure.deadpan");
        let mut store = create(&path)?;
        let before = store.snapshot()?;
        store.commit(&ordinary(
            &before,
            "later",
            Command::Rename {
                node: node("hold"),
                label: "Later edit".into(),
            },
        ))?;
        store.undo(&revision("later"), revision("undone"))?;
        assert_eq!(store.history_availability()?, (false, true));
        let request = compound(&store, "retry", authored)?;
        let document = store.snapshot()?;
        let bank = store.registers()?;
        let before = cells(&path)?;
        let called = Cell::new(0);
        let failed = store.commit_compound_prepared(&request, None, |prepared| {
            called.set(called.get() + 1);
            assert_eq!(prepared.version, bank.version + 1);
            assert!(
                prepared
                    .entries
                    .contains_key(&RegisterName::new('a').unwrap())
            );
            Err::<(), _>("host rejected prepared state")
        });
        assert!(matches!(
            failed,
            Err(CompoundPreparationError::Preparation(
                "host rejected prepared state"
            ))
        ));
        assert_eq!(called.get(), 1);
        assert_eq!(cells(&path)?, before);
        assert_eq!(store.snapshot()?, document);
        assert_eq!(store.registers()?, bank);
        assert_eq!(store.history_availability()?, (false, true));

        let (outcome, prepared) = store.commit_compound_prepared(&request, None, |bank| {
            Ok::<_, std::convert::Infallible>(bank.clone())
        })?;
        assert_eq!(outcome.register_bank, prepared);
        assert_eq!(outcome.committed.is_some(), authored);
        assert_eq!(store.registers()?, prepared);
        assert_eq!(
            store.history_availability()?,
            if authored {
                (true, false)
            } else {
                (false, true)
            }
        );
        if !authored {
            assert_eq!(store.snapshot()?, document);
            store.redo(document.revision_id(), revision("redo-after-register"))?;
            assert_eq!(store.snapshot()?.nodes()[&node("hold")].label, "Later edit");
        }
        store.validate_report(true)?;
    }
    Ok(())
}

#[test]
fn authoritative_refusals_happen_before_host_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("refusals.deadpan");
    let mut store = create(&path)?;
    let valid = compound(&store, "candidate", true)?;
    let mut stale_head = valid.clone();
    stale_head.expected_revision = revision("stale");
    let mut stale_bank = valid.clone();
    let Command::Compound { transaction } = &valid.command else {
        unreachable!()
    };
    stale_bank.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version() + 1,
            transaction.inputs().clone(),
            transaction.steps().to_vec(),
        )?,
    };
    let mut reused_root = valid.clone();
    reused_root.new_revision = revision("initial");
    let mut reused_leaf = valid.clone();
    let mut steps = transaction.steps().to_vec();
    let ResolvedStep::Edit { edit } = &mut steps[1] else {
        unreachable!()
    };
    edit.new_revision = revision("initial");
    reused_leaf.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            transaction.inputs().clone(),
            steps,
        )?,
    };
    let before = cells(&path)?;
    for request in [stale_head, stale_bank, reused_root, reused_leaf] {
        let called = Cell::new(false);
        let result = store.commit_compound_prepared(&request, None, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        });
        assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
        assert!(!called.get());
        assert_eq!(cells(&path)?, before);
    }
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let called = Cell::new(false);
    assert!(matches!(
        reader.commit_compound_prepared(&valid, None, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        }),
        Err(CompoundPreparationError::Store(
            deadpan_store::StoreError::ReadOnly
        ))
    ));
    assert!(!called.get());
    Ok(())
}

#[test]
fn prepared_commit_matches_preview_commit_history_and_reopening() -> Result {
    let scratch = tempfile::tempdir()?;
    let old_path = scratch.path().join("old.deadpan");
    let new_path = scratch.path().join("new.deadpan");
    let mut old = create(&old_path)?;
    let mut new = create(&new_path)?;
    let request = compound(&old, "accepted", true)?;
    let preview = old.preview_compound(&request)?;
    let expected = old.commit_compound(&request, None)?;
    let called = Cell::new(0);
    let (outcome, prepared) = new.commit_compound_prepared(&request, None, |bank| {
        called.set(called.get() + 1);
        assert_eq!(*bank, preview.register_bank);
        Ok::<_, std::convert::Infallible>(bank.clone())
    })?;
    assert_eq!(called.get(), 1);
    assert_eq!(
        serde_json::to_value(outcome)?,
        serde_json::to_value(expected)?
    );
    assert_eq!(new.registers()?, prepared);
    assert_eq!(new.snapshot()?, old.snapshot()?);
    assert_eq!(cells(&new_path)?, cells(&old_path)?);
    for store in [&mut old, &mut new] {
        store.undo(&revision("accepted"), revision("undo"))?;
        assert_eq!(
            store.snapshot()?.nodes()[&node("hold")].label,
            "Initial Hold"
        );
        assert_eq!(store.registers()?, prepared);
        store.redo(&revision("undo"), revision("redo"))?;
        assert_eq!(
            store.snapshot()?.nodes()[&node("hold")].label,
            "Prepared edit"
        );
        store.validate_report(true)?;
    }
    assert_eq!(cells(&new_path)?, cells(&old_path)?);
    drop(old);
    drop(new);
    let old = ProjectStore::open(&old_path, AccessMode::ReadOnly)?;
    let new = ProjectStore::open(&new_path, AccessMode::ReadOnly)?;
    assert_eq!(
        new.snapshot_with_registers()?,
        old.snapshot_with_registers()?
    );
    new.validate_report(true)?;
    Ok(())
}

struct PreparedHost(Arc<AtomicUsize>);

impl Drop for PreparedHost {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn one_leaf_plan(store: &ProjectStore) -> Result<SemanticPlan> {
    one_leaf_plan_with_audio(store, HoldAudio::Silence)
}

fn one_leaf_plan_with_audio(store: &ProjectStore, audio: HoldAudio) -> Result<SemanticPlan> {
    let document = store.snapshot()?;
    let planned = plan_semantic_with_speech(
        &document,
        &SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: node("root"),
            cursor: ProjectFrame(5),
            selected_child: None,
            visual_selection: None,
        },
        &SemanticProgram::new(vec![SemanticInstruction::InsertPause {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(3).unwrap(),
            },
            black: false,
        }])?,
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: store.register_version()?,
        },
        revision("cached-outer"),
        |request| {
            let SemanticAllocationRequest::InsertPause {
                required_split_ids, ..
            } = request
            else {
                panic!("pause")
            };
            Ok(SemanticAllocation::InsertPause {
                new_revision: revision("cached-leaf"),
                id: node("cached-pause"),
                split: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|index| node(&format!("cached-split-{index}")))
                        .collect(),
                },
            })
        },
        |_, _| panic!("no Original"),
        |_| panic!("no speech"),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
                audio: audio.clone(),
            })
        },
    )?;
    assert!(planned.one_leaf_replay().is_some());
    Ok(planned)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn one_leaf_admitted_original(
    parent: &Path,
) -> Result<(std::path::PathBuf, ProjectStore, TranscriptKey, Transcript)> {
    use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

    // Match transcript_selection_guard::ready: source admission is obtained
    // from real decoding. Only the word annotations below are synthetic.
    let active = || AtomicBool::new(false);
    let path = parent.join("admitted-cache.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("admitted-cache")?,
        revision("initial"),
        node("root"),
    )?;
    let mut store = ProjectStore::create_single_source(&path, &document)?;
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
        AssetId::new("decode")?,
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
        engine: "cached-guard".into(),
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
            new_asset_id: AssetId::new("original")?,
            node: node("full-original"),
            label: "The original".into(),
        },
        &source,
        &active(),
    )?;
    store.save_transcript(&key, &proposal)?;
    Ok((path, store, key, proposal))
}

#[test]
fn one_leaf_replay_prepared_commit_retains_exact_history_undo_redo_and_copy() -> Result {
    let scratch = tempfile::tempdir()?;
    let cached_path = scratch.path().join("cached.deadpan");
    let plain_path = scratch.path().join("plain.deadpan");
    let mut cached = create(&cached_path)?;
    let mut plain = create(&plain_path)?;
    let planned = one_leaf_plan(&cached)?;
    let request = planned.request.as_ref().unwrap();
    let candidate = planned.one_leaf_replay();
    let (actual, actual_bank) =
        cached.commit_compound_prepared_with_one_leaf(request, None, None, candidate, |bank| {
            Ok::<_, std::convert::Infallible>(bank.clone())
        })?;
    let (expected, expected_bank) = plain.commit_compound_prepared(request, None, |bank| {
        Ok::<_, std::convert::Infallible>(bank.clone())
    })?;
    assert_eq!(
        serde_json::to_string(&actual)?,
        serde_json::to_string(&expected)?
    );
    assert_eq!(actual_bank, expected_bank);
    assert_eq!(
        cached.snapshot()?.to_compact_json()?,
        planned.document.to_compact_json()?
    );
    assert_eq!(cells(&cached_path)?, cells(&plain_path)?);
    for store in [&mut cached, &mut plain] {
        store.undo(&revision("cached-outer"), revision("cached-undo"))?;
        store.redo(&revision("cached-undo"), revision("cached-redo"))?;
        store.validate_report(true)?;
    }
    assert_eq!(cells(&cached_path)?, cells(&plain_path)?);
    drop(cached);
    drop(plain);
    let cached = ProjectStore::open(&cached_path, AccessMode::ReadOnly)?;
    let plain = ProjectStore::open(&plain_path, AccessMode::ReadOnly)?;
    assert_eq!(
        cached.snapshot_with_registers()?,
        plain.snapshot_with_registers()?
    );
    let copy_path = scratch.path().join("copy.deadpan");
    deadpan_store::portable::copy_portable(
        &cached_path,
        &copy_path,
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    let copy = ProjectStore::open(&copy_path, AccessMode::ReadOnly)?;
    assert_eq!(
        copy.snapshot_with_registers()?,
        cached.snapshot_with_registers()?
    );
    copy.validate_report(true)?;
    Ok(())
}

#[test]
fn one_leaf_replay_store_refusals_precede_callback_and_leave_no_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("refused-cache.deadpan");
    let mut store = create(&path)?;
    let planned = one_leaf_plan(&store)?;
    let valid = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &valid.command else {
        panic!("compound")
    };
    let mut stale = valid.clone();
    stale.expected_revision = revision("stale");
    let mut bank = valid.clone();
    bank.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version() + 1,
            transaction.inputs().clone(),
            transaction.steps().to_vec(),
        )?,
    };
    let mut frozen = valid.clone();
    frozen.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            BTreeMap::from([(
                RegisterName::new('a')?,
                Some(Arc::new(RegisterValue::Macro {
                    program: Arc::new(SemanticProgram::new(vec![
                        SemanticInstruction::MoveFrames {
                            forward: true,
                            count: std::num::NonZeroU32::new(1).unwrap(),
                        },
                    ])?),
                })),
            )]),
            transaction.steps().to_vec(),
        )?,
    };
    let mut reused = valid.clone();
    reused.new_revision = revision("initial");
    let mut reused_leaf = valid.clone();
    let mut steps = transaction.steps().to_vec();
    let ResolvedStep::Edit { edit } = &mut steps[0] else {
        panic!("Edit")
    };
    edit.new_revision = revision("initial");
    reused_leaf.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            transaction.inputs().clone(),
            steps,
        )?,
    };
    let before = cells(&path)?;
    for request in [stale, bank, frozen, reused, reused_leaf] {
        let mut errors = Vec::new();
        for candidate in [planned.one_leaf_replay(), None] {
            let called = Cell::new(false);
            let error = store
                .commit_compound_prepared_with_one_leaf(&request, None, None, candidate, |_| {
                    called.set(true);
                    Ok::<_, std::convert::Infallible>(())
                })
                .unwrap_err();
            assert!(!called.get());
            errors.push(error.to_string());
            assert_eq!(cells(&path)?, before);
        }
        assert_eq!(errors[0], errors[1]);
    }
    Ok(())
}

#[test]
fn one_leaf_replay_host_and_sql_failures_roll_back_even_after_a_hit() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("failed-cache.deadpan");
    let mut store = create(&path)?;
    let planned = one_leaf_plan(&store)?;
    let request = planned.request.as_ref().unwrap();
    let before = cells(&path)?;
    for candidate in [planned.one_leaf_replay(), None] {
        let result =
            store.commit_compound_prepared_with_one_leaf(request, None, None, candidate, |_| {
                Err::<(), _>("host rejected")
            });
        assert!(matches!(
            result,
            Err(CompoundPreparationError::Preparation("host rejected"))
        ));
        assert_eq!(cells(&path)?, before);
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER reject_cached_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'injected cached history failure'); END;")?;
    let before = cells(&path)?;
    for candidate in [planned.one_leaf_replay(), None] {
        let dropped = Arc::new(AtomicUsize::new(0));
        let called = Cell::new(0);
        let result =
            store.commit_compound_prepared_with_one_leaf(request, None, None, candidate, |_| {
                called.set(called.get() + 1);
                Ok::<_, std::convert::Infallible>(PreparedHost(Arc::clone(&dropped)))
            });
        assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
        assert_eq!(called.get(), 1);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert_eq!(cells(&path)?, before);
    }
    database.execute_batch("DROP TRIGGER reject_cached_history;")?;
    store.commit_compound_prepared_with_one_leaf(
        request,
        None,
        None,
        planned.one_leaf_replay(),
        |_| Ok::<_, std::convert::Infallible>(()),
    )?;
    store.validate_report(true)?;
    Ok(())
}

#[test]
fn one_leaf_replay_transcript_guard_is_current_authority_before_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("guarded-cache.deadpan");
    let mut store = create(&path)?;
    let planned = one_leaf_plan(&store)?;
    let guard = TranscriptSelectionGuard {
        format: TranscriptSelectionFormat::V1,
        asset: AssetId::new("missing-original")?,
        qualification: SourceQualificationId::new("a".repeat(64))?,
        key: TranscriptKey {
            content: "b".repeat(64),
            audio_stream: 0,
            model_sha256: "c".repeat(64),
            language: "en".into(),
            engine: "cached-guard".into(),
        },
        proposal_sha256: "d".repeat(64),
        correction_rule: TranscriptSelectionCorrectionRule::V1,
        corrections: TranscriptSelectionCorrections::Absent,
    };
    let before = cells(&path)?;
    let mut errors = Vec::new();
    for candidate in [planned.one_leaf_replay(), None] {
        let called = Cell::new(false);
        let error = store
            .commit_compound_prepared_with_one_leaf(
                planned.request.as_ref().unwrap(),
                None,
                Some(&guard),
                candidate,
                |_| {
                    called.set(true);
                    Ok::<_, std::convert::Infallible>(())
                },
            )
            .unwrap_err();
        assert!(!called.get());
        errors.push(error.to_string());
        assert_eq!(cells(&path)?, before);
    }
    assert_eq!(errors[0], errors[1]);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn one_leaf_replay_admitted_reverse_audio_rechecks_stored_qualification() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, _, _) = one_leaf_admitted_original(scratch.path())?;
    let document = store.snapshot()?;
    let asset = AssetId::new("original")?;
    let record = &document.assets()[&asset];
    let qualification = record.source_qualification.as_ref().unwrap();
    let full = record.audio.unwrap();
    let source = SourceAudio {
        asset,
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks + 17,
                time_base: full.start().time_base,
            },
            SourceTimestamp {
                ticks: full.start().ticks + 113,
                time_base: full.start().time_base,
            },
        )?,
    };
    let planned = one_leaf_plan_with_audio(&store, HoldAudio::Reverse { source })?;
    let request = planned.request.as_ref().unwrap();
    // Establish that the exact request passes real Store media admission before
    // damaging its receipt. The candidate's immutable entry stays identical.
    store.preview_compound(request)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(
        database.execute(
            "DELETE FROM source_qualifications WHERE id=?1",
            [qualification.as_str()],
        )?,
        1
    );
    let before = cells(&path)?;
    let mut errors = Vec::new();
    for candidate in [planned.one_leaf_replay(), None] {
        let called = Cell::new(0);
        let error = store
            .commit_compound_prepared_with_one_leaf(request, None, None, candidate, |_| {
                called.set(called.get() + 1);
                Ok::<_, std::convert::Infallible>(())
            })
            .unwrap_err();
        let CompoundPreparationError::Store(error) = error;
        assert!(matches!(&error, StoreError::SourceRegistration(message)
            if message == "Hold audio source qualification is missing"));
        assert_eq!(called.get(), 0);
        errors.push((format!("{error:?}"), error.to_string()));
        assert_eq!(store.snapshot()?, document);
        assert_eq!(cells(&path)?, before);
    }
    assert_eq!(errors[0], errors[1]);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn one_leaf_replay_changed_corrections_invalidate_admitted_transcript_guard() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, key, proposal) = one_leaf_admitted_original(scratch.path())?;
    let document = store.snapshot()?;
    let planned = one_leaf_plan(&store)?;
    let request = planned.request.as_ref().unwrap();
    let guard = store.transcript_selection(&key)?.guard;
    assert_eq!(guard.corrections, TranscriptSelectionCorrections::Absent);
    store.preview_compound_with_transcript_guard(request, &guard)?;
    let corrected = Corrections::empty(Corrections::clock_of(&proposal)).edit_word_text(
        &CorrectedTranscript::recognized(proposal.clone()),
        0,
        "hullo",
        &[],
    )?;
    let applied = store.change_analysis_corrections(
        &CorrectionsKey {
            content: key.content.clone(),
            audio_stream: key.audio_stream,
        },
        0,
        CorrectionChange::Apply {
            corrections: corrected,
            label: "Correct hello".into(),
        },
    )?;
    assert_eq!(applied.version, 1);
    let fresh = store.transcript_selection(&key)?.guard;
    assert!(matches!(
        fresh.corrections,
        TranscriptSelectionCorrections::Present { version: 1, .. }
    ));
    assert_ne!(fresh, guard);
    // The current guard still accepts this exact edit. Only the captured
    // analysis authority became stale; the cached project entry did not.
    store.preview_compound_with_transcript_guard(request, &fresh)?;
    let before = cells(&path)?;
    let mut errors = Vec::new();
    for candidate in [planned.one_leaf_replay(), None] {
        let called = Cell::new(0);
        let error = store
            .commit_compound_prepared_with_one_leaf(request, None, Some(&guard), candidate, |_| {
                called.set(called.get() + 1);
                Ok::<_, std::convert::Infallible>(())
            })
            .unwrap_err();
        let CompoundPreparationError::Store(error) = error;
        assert!(matches!(&error, StoreError::Integrity(message)
            if message == "transcript selection analysis changed"));
        assert_eq!(called.get(), 0);
        errors.push((format!("{error:?}"), error.to_string()));
        assert_eq!(store.snapshot()?, document);
        assert_eq!(cells(&path)?, before);
    }
    assert_eq!(errors[0], errors[1]);
    Ok(())
}

#[test]
fn one_leaf_replay_historical_allocations_refuse_before_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("historical-allocation.deadpan");
    let mut store = create(&path)?;
    let initial = store.snapshot()?;
    store.commit(&ordinary(
        &initial,
        "later",
        Command::Rename {
            node: node("hold"),
            label: "Advance the head".into(),
        },
    ))?;
    let document = store.snapshot()?;
    assert_eq!(document.revision_id(), &revision("later"));
    let planned = one_leaf_plan(&store)?;
    let valid = planned.request.as_ref().unwrap();
    store.preview_compound(valid)?;
    let mut reused_outer = valid.clone();
    reused_outer.new_revision = revision("initial");
    let Command::Compound { transaction } = &valid.command else {
        panic!("Compound")
    };
    let mut steps = transaction.steps().to_vec();
    let ResolvedStep::Edit { edit } = &mut steps[0] else {
        panic!("Edit")
    };
    edit.new_revision = revision("initial");
    let mut reused_leaf = valid.clone();
    reused_leaf.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            transaction.expected_bank_version(),
            transaction.inputs().clone(),
            steps,
        )?,
    };
    let before = cells(&path)?;
    for request in [reused_outer, reused_leaf] {
        assert_ne!(request.new_revision, request.expected_revision);
        let mut errors = Vec::new();
        for candidate in [planned.one_leaf_replay(), None] {
            let called = Cell::new(0);
            let error = store
                .commit_compound_prepared_with_one_leaf(&request, None, None, candidate, |_| {
                    called.set(called.get() + 1);
                    Ok::<_, std::convert::Infallible>(())
                })
                .unwrap_err();
            let CompoundPreparationError::Store(error) = error;
            assert!(matches!(&error, StoreError::RevisionReused(id) if id == "initial"));
            assert_eq!(called.get(), 0);
            errors.push((format!("{error:?}"), error.to_string()));
            assert_eq!(store.snapshot()?, document);
            assert_eq!(cells(&path)?, before);
        }
        assert_eq!(errors[0], errors[1]);
    }
    Ok(())
}

#[test]
fn write_failure_discards_host_state_and_rolls_back_before_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("write-failure.deadpan");
    let mut store = create(&path)?;
    let request = compound(&store, "accepted", true)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER reject_prepared_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'injected prepared history failure'); END;")?;
    let before = cells(&path)?;
    let dropped = Arc::new(AtomicUsize::new(0));
    let called = Cell::new(0);
    let result = store.commit_compound_prepared(&request, None, |_| {
        called.set(called.get() + 1);
        Ok::<_, std::convert::Infallible>(PreparedHost(Arc::clone(&dropped)))
    });
    assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
    assert_eq!(called.get(), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(cells(&path)?, before);
    assert_eq!(store.snapshot()?.revision_id(), &revision("initial"));
    assert_eq!(store.register_version()?, 0);
    database.execute_batch("DROP TRIGGER reject_prepared_history;")?;
    let (outcome, host) = store.commit_compound_prepared(&request, None, |_| {
        Ok::<_, std::convert::Infallible>(PreparedHost(Arc::clone(&dropped)))
    })?;
    assert!(outcome.committed.is_some());
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    drop(host);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    store.validate_report(true)?;
    Ok(())
}

#[test]
fn unavailable_transcript_authority_refuses_before_host_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("guard.deadpan");
    let mut store = create(&path)?;
    let request = compound(&store, "guarded", true)?;
    let guard = TranscriptSelectionGuard {
        format: TranscriptSelectionFormat::V1,
        asset: AssetId::new("missing-original")?,
        qualification: SourceQualificationId::new("a".repeat(64))?,
        key: TranscriptKey {
            content: "b".repeat(64),
            audio_stream: 0,
            model_sha256: "c".repeat(64),
            language: "en".into(),
            engine: "guard-test".into(),
        },
        proposal_sha256: "d".repeat(64),
        correction_rule: TranscriptSelectionCorrectionRule::V1,
        corrections: TranscriptSelectionCorrections::Absent,
    };
    let before = cells(&path)?;
    let called = Cell::new(false);
    let result =
        store.commit_compound_with_transcript_guard_prepared(&request, None, &guard, |_| {
            called.set(true);
            Ok::<_, std::convert::Infallible>(())
        });
    assert!(matches!(result, Err(CompoundPreparationError::Store(_))));
    assert!(!called.get());
    assert_eq!(cells(&path)?, before);
    Ok(())
}
