use super::*;
use deadpan_core::AudioTimingId;
use deadpan_media::source_import_timing::derive_source_moment;
use deadpan_store::source_registration::SourceMomentInsertionRequest;

#[path = "moment/interior.rs"]
mod interior;

#[path = "moment/replacement.rs"]
mod replacement;

#[path = "moment/dormant.rs"]
mod dormant;

fn ready(parent: &Path) -> Result<(PathBuf, ProjectStore, PreparedSourceRegistration)> {
    let (path, mut store) = project(parent)?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let prepared = decoded_from_handle(store.original_import_handle()?, original.clone())?;
    let input = primary_registration(&store, &original, "registered", "original", "full");
    store.register_prepared_source(&input, &prepared, None, &cancelled())?;
    Ok((path, store, prepared))
}

fn paste(store: &ProjectStore, next: &str) -> Result<SourceMomentInsertionRequest> {
    Ok(SourceMomentInsertionRequest {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: revision(next),
        asset: asset("original"),
        parent: NodeId::new("root")?,
        index: 0,
        node: NodeId::new("moment")?,
        label: "Original moment".into(),
        timing: AudioTimingId {
            allocation: revision(next),
            ordinal: 0,
        },
        ordinals: 1..3,
    })
}

#[test]
fn paste_uses_exact_measured_selection_and_one_durable_undo_entry() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let input = paste(&store, "pasted")?;
    let preview = store.preview_prepared_source_moment(&input, &prepared, &cancelled())?;
    assert_eq!(counts(&path)?, before_counts);
    let expected = preview.forward.apply(&before)?;
    let outcome = store.commit_prepared_source_moment(&input, &prepared, None, &cancelled())?;
    assert_eq!(outcome.revision_id, input.new_revision);
    assert_eq!(store.snapshot()?, expected);
    assert_eq!(
        counts(&path)?,
        (before_counts.0 + 1, before_counts.1 + 1, before_counts.2)
    );
    assert_eq!(expected.assets(), before.assets());
    let NodeKind::Source { source } = &expected.nodes()[&input.node].kind else {
        panic!("paste must retain an editable Source");
    };
    let deadpan_core::SourceVideo::Stream { span: context, .. } = source.video else {
        panic!("paste must retain Original video");
    };
    assert_eq!(Some(context), expected.assets()[&input.asset].video);
    assert!(matches!(
        source.video_mapping,
        deadpan_core::SourceVideoMapping::SelectedPlacement { .. }
    ));
    let selection = source
        .video_mapping
        .selection_in_source(context, source.duration)?;
    let index = prepared
        .receipt()
        .snapshot()
        .video()
        .unwrap()
        .index()
        .index();
    assert_eq!(
        selection.start().ticks,
        deadpan_core::ExactRatio::integer(index.interval(deadpan_core::SourceFrameId(1))?.0)
    );
    assert_eq!(
        selection.end().ticks,
        deadpan_core::ExactRatio::integer(index.interval(deadpan_core::SourceFrameId(2))?.1)
    );
    let timing = derive_source_moment(
        prepared.receipt().snapshot().video().unwrap().index(),
        prepared.receipt().snapshot().audio(),
        input.ordinals,
        before.presentation_basis().frame_rate,
    )?;
    assert_eq!(
        expected.nodes()[&input.node].kind,
        NodeKind::Source {
            source: timing.source_node(input.asset),
        }
    );
    assert!(
        !expected
            .audio_bindings()
            .bindings()
            .contains_key(&input.node)
    );
    store.undo(expected.revision_id(), revision("undo-paste"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.audio_bindings(), before.audio_bindings());
    store.validate()?;
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(undone.revision_id(), revision("redo-paste"))?;
    assert_eq!(reopened.snapshot()?.nodes(), expected.nodes());
    assert_eq!(
        reopened.snapshot()?.audio_bindings(),
        expected.audio_bindings()
    );
    reopened.validate()?;
    Ok(())
}

#[test]
fn paste_rejects_stale_wrong_asset_and_invalid_selection_without_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    for case in ["stale", "asset", "empty", "inverted", "outside", "slot"] {
        let mut input = paste(&store, "rejected")?;
        match case {
            "stale" => input.expected_revision = revision("initial"),
            "asset" => input.asset = asset("other"),
            "empty" => input.ordinals = 1..1,
            "inverted" => input.ordinals = std::ops::Range { start: 3, end: 1 },
            "outside" => input.ordinals = 0..u64::MAX,
            "slot" => input.index = usize::MAX,
            _ => unreachable!(),
        }
        assert!(
            store
                .preview_prepared_source_moment(&input, &prepared, &cancelled())
                .is_err(),
            "{case}"
        );
        assert!(
            store
                .commit_prepared_source_moment(&input, &prepared, None, &cancelled())
                .is_err(),
            "{case}"
        );
        assert_eq!(store.snapshot()?, before);
        assert_eq!(counts(&path)?, before_counts);
    }
    let other_original = retain(&mut store, "cfr-bframes.mp4")?;
    let other = decoded_from_handle(store.original_import_handle()?, other_original)?;
    assert!(
        store
            .commit_prepared_source_moment(
                &paste(&store, "wrong-receipt")?,
                &other,
                None,
                &cancelled()
            )
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn paste_requires_existing_unchanged_receipt_and_never_repairs_evidence() -> Result {
    for case in ["missing", "modified"] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store, prepared) = ready(scratch.path())?;
        let input = paste(&store, "rejected")?;
        let before = store.snapshot()?;
        let database = Connection::open(path.join("project.sqlite"))?;
        if case == "missing" {
            database.execute("DELETE FROM source_qualifications", [])?;
        } else {
            database.execute("UPDATE source_qualifications SET snapshot=X'7B7D'", [])?;
        }
        let before_counts = counts(&path)?;
        assert!(
            store
                .preview_prepared_source_moment(&input, &prepared, &cancelled())
                .is_err()
        );
        assert!(
            store
                .commit_prepared_source_moment(&input, &prepared, None, &cancelled())
                .is_err()
        );
        assert_eq!(store.snapshot()?, before);
        assert_eq!(counts(&path)?, before_counts);
    }
    Ok(())
}

#[test]
fn paste_rechecks_original_and_live_writer_session() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let input = paste(&store, "rejected")?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let object = path.join("Media/Originals").join(format!(
        "blake3-{}",
        prepared.receipt().original().content().digest()
    ));
    fs::remove_file(object)?;
    assert!(
        store
            .commit_prepared_source_moment(&input, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(
        reopened
            .commit_prepared_source_moment(&input, &prepared, None, &cancelled())
            .is_err()
    );
    assert_eq!(reopened.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    Ok(())
}

#[test]
fn failed_paste_transaction_keeps_revision_history_and_receipt_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let input = paste(&store, "pasted")?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TRIGGER fail_moment_cursor BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='pasted'
         BEGIN SELECT RAISE(ABORT, 'injected moment commit failure'); END;",
    )?;
    assert!(matches!(
        store.commit_prepared_source_moment(&input, &prepared, None, &cancelled()),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
            if message == "injected moment commit failure"
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    database.execute_batch("DROP TRIGGER fail_moment_cursor")?;
    store.commit_prepared_source_moment(&input, &prepared, None, &cancelled())?;
    store.validate()?;
    Ok(())
}

#[test]
fn paste_reconciles_complete_generation_relevance_in_its_single_transaction() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let hold = NodeId::new("hold")?;
    let duration = FrameDuration::new(12)?;
    store.commit(&command(
        &store,
        "with-hold",
        Command::Insert {
            parent: NodeId::new("root")?,
            index: 1,
            subtree: Subtree {
                root: hold.clone(),
                nodes: BTreeMap::from([(
                    hold.clone(),
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            duration,
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
    ))?;
    let before = store.snapshot()?;
    let current = store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new("moment-bridge")?,
        expected_revision: before.revision_id().clone(),
        hold_id: hold,
        context_sha256: Sha256::new("a".repeat(64))?,
        constraints: HoldConstraints {
            video: VideoSpec::new(duration, before.presentation_basis().frame_rate, 512, 320)?,
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: ProviderSelection {
            pack_id: ProviderPackId::new("pack")?,
            pack_version: ProviderPackVersion::new("v1")?,
            runtime_id: RuntimeId::new("runtime")?,
            runtime_version: RuntimeVersion::new("v1")?,
            seed: 1,
        },
    })?;
    let input = paste(&store, "pasted")?;
    let preview = store.preview_prepared_source_moment(&input, &prepared, &cancelled())?;
    let expected = preview.forward.apply(&before)?;
    let before_counts = counts(&path)?;
    assert!(matches!(
        store.commit_prepared_source_moment(&input, &prepared, None, &cancelled()),
        Err(StoreError::GenerationRelevanceRequired)
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(store.current_generation_requests()?, vec![current.clone()]);
    let relevance = RelevancePlan {
        from_revision: input.expected_revision.clone(),
        to_revision: input.new_revision.clone(),
        observations: vec![RelevanceObservation {
            request_id: current.request_id.clone(),
            binding: current.binding.clone(),
            after_context: ContextObservation::Resolved(Sha256::new("b".repeat(64))?),
        }],
    };
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TRIGGER fail_moment_relevance BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='pasted'
         BEGIN SELECT CASE WHEN
             (SELECT relevance FROM generation_requests WHERE request_id='moment-bridge')='stale'
             AND EXISTS(SELECT 1 FROM revisions WHERE id='pasted')
             AND EXISTS(SELECT 1 FROM history WHERE revision_id='pasted')
             AND (SELECT count(*) FROM source_qualifications)=1
         THEN RAISE(ABORT, 'moment relevance staged atomically')
         ELSE RAISE(ABORT, 'moment tentative writes missing') END; END;",
    )?;
    assert!(
        matches!(store.commit_prepared_source_moment(&input, &prepared, Some(&relevance), &cancelled()),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
            if message == "moment relevance staged atomically")
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(store.current_generation_requests()?, vec![current.clone()]);
    database.execute_batch("DROP TRIGGER fail_moment_relevance")?;
    store.commit_prepared_source_moment(&input, &prepared, Some(&relevance), &cancelled())?;
    assert_eq!(store.snapshot()?, expected);
    assert_eq!(
        counts(&path)?,
        (before_counts.0 + 1, before_counts.1 + 1, before_counts.2)
    );
    let mut stale = current;
    stale.relevance = Relevance::Stale;
    assert_eq!(store.generation_request(&stale.request_id)?, Some(stale));
    store.validate()?;
    Ok(())
}
