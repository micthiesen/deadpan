//! Actual actor requests over a qualified decoded Original. Transcript words
//! are synthetic annotations, and never stand in for media measurements.
use super::*;
use deadpan_analysis::{AnalysedAudio, Corrections, Transcript, Word};

fn create(service: &ProjectService) -> ProjectUpdate {
    // Keep independent idle retention from writing its watermark while these
    // tests compare every database cell around a refused command.
    service.set_automatic_retention(false);
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    initialized
}

fn key(update: &ProjectUpdate) -> deadpan_store::TranscriptKey {
    let workspace = update.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!()
    };
    let receipt = &workspace.sources[asset].receipt;
    deadpan_store::TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "native-search-fixture".into(),
    }
}

fn proposal(update: &ProjectUpdate, first: &str) -> Transcript {
    let workspace = update.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!()
    };
    let audio = workspace.sources[asset].receipt.snapshot().audio().unwrap();
    let origin = audio.frames().first().unwrap().valid_start;
    let end = audio.frames().last().unwrap().valid_end;
    let duration_cs = u32::try_from(
        (u64::try_from(end - origin).unwrap() * 100)
            .div_ceil(u64::from(audio.stream().sample_rate)),
    )
    .unwrap();
    Transcript::new(
        AnalysedAudio {
            origin,
            sample_rate: audio.stream().sample_rate,
            duration_cs,
        },
        [
            (first, 1, 3),
            ("world", 4, 6),
            ("hello", 20, 22),
            ("world", 23, 25),
        ]
        .into_iter()
        .map(|(text, start_cs, end_cs)| Word {
            text: text.into(),
            start_cs,
            end_cs,
            probability: 0.9,
            segment: 0,
        })
        .collect(),
    )
    .unwrap()
}

fn save_transcript(
    service: &ProjectService,
    update: &ProjectUpdate,
    attempt: u64,
    key: deadpan_store::TranscriptKey,
    transcript: Transcript,
) -> ProjectUpdate {
    let workspace = update.workspace.as_ref().unwrap();
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveTranscript {
            lifecycle_epoch: None,
            expected_session: workspace.session,
            attempt,
            key,
            transcript: Arc::new(transcript),
        })
        .unwrap();
    let saved = wait(service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|saved| saved.attempt == attempt)
    });
    assert_eq!(saved.transcript_save.as_ref().unwrap().error, None);
    saved
}

fn apply(update: &ProjectUpdate, request: u64, instruction: SemanticInstruction) -> Operation {
    let Operation::Run {
        id, scope, context, ..
    } = run(update, request, 'a', 1, 0)
    else {
        unreachable!()
    };
    Operation::Apply {
        id,
        instruction: Box::new(instruction),
        repeat_version: None,
        scope,
        context,
    }
}

fn query() -> deadpan_core::SemanticTranscriptQuery {
    deadpan_core::SemanticTranscriptQuery::new("hello world").unwrap()
}
fn search(update: &ProjectUpdate, request: u64) -> Operation {
    apply(
        update,
        request,
        SemanticInstruction::SelectTranscriptSearch { query: query() },
    )
}

fn cells(path: &Path) -> Vec<(String, Vec<Vec<String>>)> {
    let database = rusqlite::Connection::open_with_flags(
        path.join("project.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    tables
        .into_iter()
        .map(|table| {
            let mut statement = database
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| Ok(format!("{:?}", row.get_ref(column)?)))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows.sort();
            (table, rows)
        })
        .collect()
}

fn refuse_without_writes(service: &ProjectService, current: &ProjectUpdate, operation: Operation) {
    let path = &current.workspace.as_ref().unwrap().path;
    let before = cells(path);
    let refused = command(service, ProjectRequest::Macro(operation));
    assert!(
        refused.macros.as_ref().unwrap().result.is_err(),
        "{:?}",
        refused.macros
    );
    assert_eq!(cells(path), before);
}

#[test]
fn search_actor_pins_entry_absence_exact_key_proposal_and_correction_versions() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initialized = create(&service);
    let missing = search(&initialized, 1);
    let key = key(&initialized);
    let proposal = proposal(&initialized, "hello");
    let saved = save_transcript(&service, &initialized, 1, key.clone(), proposal.clone());
    assert!(
        saved
            .workspace
            .as_ref()
            .unwrap()
            .transcript
            .as_ref()
            .unwrap()
            .selection_guard
            .is_some()
    );
    refuse_without_writes(&service, &saved, missing);
    let stale = search(&saved, 2);
    let replaced = save_transcript(
        &service,
        &saved,
        2,
        key.clone(),
        self::proposal(&saved, "hullo"),
    );
    refuse_without_writes(&service, &replaced, stale);
    let restored = save_transcript(&service, &replaced, 3, key.clone(), proposal.clone());
    let stale = search(&restored, 3);
    let workspace = restored.workspace.as_ref().unwrap();
    let _ = service.take_update();
    service
        .submit(ProjectRequest::ChangeCorrections(
            crate::project::CorrectionRequest {
                expected_session: workspace.session,
                attempt: 1,
                key: workspace.corrections.as_ref().unwrap().key.clone(),
                expected_version: 0,
                change: deadpan_store::CorrectionChange::Apply {
                    corrections: Corrections::empty(Corrections::clock_of(&proposal)),
                    label: "Empty correction".into(),
                },
                transcript: workspace.transcript.clone(),
                activity: workspace.speech_activity.clone(),
            },
        ))
        .unwrap();
    let corrected = wait(&service, |update| {
        update
            .correction_save
            .as_ref()
            .is_some_and(|saved| saved.attempt == 1)
    });
    assert_eq!(corrected.correction_save.as_ref().unwrap().error, None);
    refuse_without_writes(&service, &corrected, stale);
    let mut other = key.clone();
    other.model_sha256 = "0".repeat(64);
    let stale = search(&corrected, 4);
    let another = save_transcript(&service, &corrected, 4, other, proposal);
    assert_ne!(
        another
            .workspace
            .as_ref()
            .unwrap()
            .transcript
            .as_ref()
            .unwrap()
            .key,
        key
    );
    refuse_without_writes(&service, &another, stale);
    let current = send(&service, search(&another, 5));
    assert!(
        matches!(&receipt(&current).outcome, Outcome::Applied { visual_selection: Some(deadpan_core::SemanticVisualSelection::TranscriptSearch { query: actual, ranges }), committed: None, .. } if actual == &query() && ranges.len() == 2)
    );
    assert_eq!(
        receipt(&current).transcript_guard.as_ref(),
        id(&another, 0).transcript_guard.as_deref()
    );
}

#[test]
fn search_macro_body_keeps_query_copies_and_cuts_one_union_with_one_history_entry() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initialized = create(&service);
    let saved = save_transcript(
        &service,
        &initialized,
        1,
        key(&initialized),
        proposal(&initialized, "hello"),
    );
    let select = SemanticInstruction::SelectTranscriptSearch { query: query() };
    let body = program(vec![
        select,
        SemanticInstruction::Yank {
            selector: SemanticSelector::TranscriptSearch { query: query() },
            register: RegisterName::new('y').unwrap(),
        },
        SemanticInstruction::Cut {
            selector: SemanticSelector::TranscriptSearch { query: query() },
            register: RegisterName::new('d').unwrap(),
        },
    ]);
    let stored = send(&service, save(&saved, 1, 'm', body.clone()));
    let before = rows(&stored.workspace.as_ref().unwrap().path);
    let executed = send(&service, run(&stored, 2, 'm', 1, 0));
    assert!(receipt(&executed).committed().is_some());
    assert!(receipt(&executed).transcript_guard.is_some());
    let after = rows(&executed.workspace.as_ref().unwrap().path);
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1 + 1);
    let bank = executed.registers.as_ref().unwrap();
    assert!(matches!(bank.entries.get(&'m'), Some(Value::Macro(saved)) if saved == &body));
    assert!(bank.entries.contains_key(&'y'));
    assert!(bank.entries.contains_key(&'d'));
    let workspace = executed.workspace.as_ref().unwrap();
    let reader = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert!(reader.validate_report(true).is_ok());
    assert_eq!(
        serde_json::to_value(&body).unwrap()["instructions"][0]["query"],
        "hello world"
    );
}

#[test]
fn delayed_search_completion_declines_new_analysis_but_retains_durable_success() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initialized = create(&service);
    let key = key(&initialized);
    let saved = save_transcript(
        &service,
        &initialized,
        1,
        key.clone(),
        proposal(&initialized, "hello"),
    );
    let cut = send(
        &service,
        apply(
            &saved,
            1,
            SemanticInstruction::Cut {
                selector: SemanticSelector::TranscriptSearch { query: query() },
                register: RegisterName::new('a').unwrap(),
            },
        ),
    );
    let durable = receipt(&cut).clone();
    let committed = durable.committed().unwrap().revision.clone();
    let guard = cut
        .workspace
        .as_ref()
        .unwrap()
        .transcript
        .as_ref()
        .unwrap()
        .selection_guard
        .as_ref();
    assert!(durable.matches_transcript(guard));
    assert!(!durable.matches_transcript(None));
    let replaced = save_transcript(&service, &cut, 2, key.clone(), proposal(&cut, "hullo"));
    let changed = replaced
        .workspace
        .as_ref()
        .unwrap()
        .transcript
        .as_ref()
        .unwrap()
        .selection_guard
        .as_ref();
    assert!(!durable.matches_transcript(changed));
    assert_eq!(durable.committed().unwrap().revision, committed);
    assert_eq!(
        replaced.workspace.as_ref().unwrap().document.revision_id(),
        &committed
    );
    let mut preferred = key;
    preferred.model_sha256 = "0".repeat(64);
    let new_key = save_transcript(
        &service,
        &replaced,
        3,
        preferred,
        proposal(&replaced, "hello"),
    );
    assert!(
        !durable.matches_transcript(
            new_key
                .workspace
                .as_ref()
                .unwrap()
                .transcript
                .as_ref()
                .unwrap()
                .selection_guard
                .as_ref()
        )
    );
    // The same completion gate leaves ordinary transcript-independent actions alone.
    let ordinary = send(&service, apply(&new_key, 2, movement(1)));
    assert_eq!(receipt(&ordinary).transcript_guard, None);
    assert!(receipt(&ordinary).matches_transcript(None));
    assert!(receipt(&ordinary).matches_transcript(changed));
    assert_eq!(durable.committed().unwrap().revision, committed);
}

/// These proposals use the fixture's qualified source clock and picture count.
/// Their synthetic probabilities and cut measures test annotation routing,
/// without claiming detector or model accuracy.
fn nonword_motion_does_not_consume_transcript(shots: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initialized = create(&service);
    let transcript_key = key(&initialized);
    let saved = save_transcript(
        &service,
        &initialized,
        1,
        transcript_key.clone(),
        proposal(&initialized, "hello"),
    );
    let workspace = saved.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!()
    };
    let qualification = &workspace.sources[asset].receipt;
    let content = qualification.original().content().to_string();
    let annotated = if shots {
        let video = qualification.snapshot().video().unwrap();
        let pictures = video.index().index().frames().len();
        let cut = pictures / 2;
        let analysis = deadpan_analysis::ShotAnalysis::from_changes(
            (0..pictures)
                .map(|picture| {
                    if picture == 0 {
                        [0, 0, 0]
                    } else if picture == cut {
                        [90, 200, 90]
                    } else if picture == cut + 1 {
                        [2, 1, 90]
                    } else {
                        [2, 1, 2]
                    }
                })
                .collect(),
        )
        .unwrap();
        assert_eq!(analysis.boundaries(), [cut]);
        let key = deadpan_store::ShotAnalysisKey {
            content,
            video_stream: video.index().stream_index(),
            signature_version: deadpan_analysis::SIGNATURE_VERSION.into(),
            interpretation: None,
        };
        let _ = service.take_update();
        service
            .submit(ProjectRequest::SaveShotAnalysis {
                expected_session: workspace.session,
                attempt: 1,
                key,
                analysis: Arc::new(analysis),
            })
            .unwrap();
        let update = wait(&service, |update| {
            update
                .shot_save
                .as_ref()
                .is_some_and(|save| save.attempt == 1)
        });
        assert_eq!(update.shot_save.as_ref().unwrap().error, None);
        update
    } else {
        let audio = qualification.snapshot().audio().unwrap();
        let origin = audio.frames().first().unwrap().valid_start;
        let sample_rate = audio.stream().sample_rate;
        let samples = audio
            .valid_samples()
            .checked_mul(u64::from(deadpan_analysis::ANALYSIS_SAMPLE_RATE))
            .unwrap()
            .div_ceil(u64::from(sample_rate));
        let quiet = samples / 4..samples / 2;
        let activity = deadpan_analysis::SpeechActivity::new(
            deadpan_analysis::ActivityAudio {
                origin,
                sample_rate,
                samples,
            },
            (0..samples.div_ceil(deadpan_analysis::VAD_HOP))
                .map(|hop| {
                    if quiet.contains(&(hop * deadpan_analysis::VAD_HOP)) {
                        5
                    } else {
                        230
                    }
                })
                .collect(),
            (0..samples.div_ceil(deadpan_analysis::ENERGY_HOP))
                .map(|frame| {
                    if quiet.contains(&(frame * deadpan_analysis::ENERGY_HOP)) {
                        20
                    } else {
                        180
                    }
                })
                .collect(),
        )
        .unwrap();
        assert_eq!(activity.pauses().len(), 1);
        let key = deadpan_store::SpeechActivityKey {
            content,
            audio_stream: audio.stream().stream_index,
            model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
            engine: "native-motion-fixture".into(),
        };
        let _ = service.take_update();
        service
            .submit(ProjectRequest::SaveSpeechActivity {
                lifecycle_epoch: None,
                expected_session: workspace.session,
                attempt: 1,
                key,
                activity: Arc::new(activity),
            })
            .unwrap();
        let update = wait(&service, |update| {
            update
                .activity_save
                .as_ref()
                .is_some_and(|save| save.attempt == 1)
        });
        assert_eq!(update.activity_save.as_ref().unwrap().error, None);
        update
    };
    assert!(
        annotated
            .workspace
            .as_ref()
            .unwrap()
            .transcript
            .as_ref()
            .unwrap()
            .selection_guard
            .is_some()
    );
    let instruction = if shots {
        SemanticInstruction::MoveShots {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        }
    } else {
        SemanticInstruction::MovePauses {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        }
    };
    let mut captured = apply(&annotated, 1, instruction);
    let Operation::Apply {
        id: captured_id, ..
    } = &mut captured
    else {
        unreachable!()
    };
    // A native request may have captured no displayed transcript authority;
    // pause and shot annotations still provide their independent motions.
    captured_id.transcript_guard = None;
    let replacement = save_transcript(
        &service,
        &annotated,
        2,
        transcript_key,
        proposal(&annotated, "hullo"),
    );
    assert_eq!(
        replacement
            .workspace
            .as_ref()
            .unwrap()
            .document
            .revision_id(),
        annotated.workspace.as_ref().unwrap().document.revision_id()
    );
    assert_eq!(
        replacement.registers.as_ref().unwrap().version,
        annotated.registers.as_ref().unwrap().version
    );
    let path = &replacement.workspace.as_ref().unwrap().path;
    let unchanged = cells(path);
    let moved = send(&service, captured);
    assert_eq!(receipt(&moved).transcript_guard, None);
    assert!(
        matches!(&receipt(&moved).outcome, Outcome::Applied { cursor, committed: None, .. } if cursor.0 > 0)
    );
    assert_eq!(cells(path), unchanged);
}

#[test]
fn pause_motion_with_qualified_activity_ignores_absent_transcript_capture_and_replacement() {
    nonword_motion_does_not_consume_transcript(false);
}

#[test]
fn shot_motion_with_qualified_picture_count_ignores_absent_transcript_capture_and_replacement() {
    nonword_motion_does_not_consume_transcript(true);
}
