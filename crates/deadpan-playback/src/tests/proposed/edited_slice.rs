use super::*;
use deadpan_store::slice_preview::SliceViewIdentities;

fn capture(document: &ProjectDocument) -> CapturedEditSlice {
    CapturedEditSlice::capture(
        document,
        document.root(),
        // Includes the fixture's opening impulse at source sample 100.
        FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn identities(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let required = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|i| node(&format!("{name}-node-{i}")))
                .collect(),
            marks: (0..required.marks)
                .map(|i| MarkId::new(format!("{name}-mark-{i}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|i| node(&format!("{name}-alias-{i}")))
            .collect(),
    }
}

fn request(base: &Snapshot, slice: &CapturedEditSlice, name: &str) -> CommandRequest {
    CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::SpliceSlice {
            parent: base.document.root().clone(),
            index: 0,
            slice: slice.clone(),
            identities: identities(slice, name),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    }
}

#[test]
fn historical_slice_admission_uses_exact_view_and_base_and_matches_committed_pcm() {
    let _permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("historical.deadpan"), &empty()).unwrap();
    register(&mut store);
    let captured = capture(&store.snapshot().unwrap());
    store
        .undo(&revision("registered"), revision("undone"))
        .unwrap();
    let base = snapshot(&store, 181);
    assert!(base.sources.is_empty());
    let request = request(&base, &captured, "proposal");
    let view = Arc::new(store.preview_edit_slice(&request).unwrap());
    assert!(matches!(
        Snapshot::proposed(&base, view.document().clone(), 1, 1),
        Err(SnapshotError::ChangedAssetContracts)
    ));
    let proposal = Arc::new(Snapshot::proposed_edit_slice(&base, view.clone(), 1, 1).unwrap());
    assert!(Arc::ptr_eq(&proposal.document, view.document()));
    assert_eq!(proposal.validate_edit_slice_view(&view), Ok(()));
    assert_eq!(
        proposal.validate_proposed_base(base.session, &base.document),
        Ok(())
    );
    assert_eq!(
        proposal.validate_proposed_base(base.session, &Arc::new((*base.document).clone())),
        Err(SnapshotError::InvalidAdmission)
    );
    assert!(store.snapshot_at(proposal.document.revision_id()).is_err());
    let alternate = Arc::new(store.preview_edit_slice(&request).unwrap());
    assert_eq!(
        proposal.validate_edit_slice_view(&alternate),
        Err(SnapshotError::InvalidAdmission)
    );
    let standalone = Arc::new(
        store
            .view_edit_slice(
                &captured,
                SliceViewIdentities {
                    empty_revision: revision("copy-empty"),
                    view_revision: revision("copy-view"),
                    root: node("copy-root"),
                    paste: identities(&captured, "copy"),
                },
            )
            .unwrap(),
    );
    assert!(matches!(
        Snapshot::proposed_edit_slice(&base, standalone, 1, 1),
        Err(SnapshotError::InvalidAdmission)
    ));
    let proposed_pcm = reference(&proposal, AudioSample(0), 4800);
    assert!(proposed_pcm.iter().any(|sample| sample.abs() > 0.001));
    store.commit(&request).unwrap();
    let committed = snapshot(&store, base.session);
    assert_eq!(*proposal.document, *committed.document);
    assert_eq!(proposed_pcm, reference(&committed, AudioSample(0), 4800));
}

#[test]
fn edited_proposal_rejects_catalog_document_and_handle_substitution() {
    let _permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tampering.deadpan");
    let mut store = ProjectStore::create(&path, &empty()).unwrap();
    register(&mut store);
    let captured = capture(&store.snapshot().unwrap());
    store
        .undo(&revision("registered"), revision("undone"))
        .unwrap();
    let base = snapshot(&store, 182);
    let view = Arc::new(
        store
            .preview_edit_slice(&request(&base, &captured, "proposal"))
            .unwrap(),
    );
    let make = || Snapshot::proposed_edit_slice(&base, view.clone(), 2, 1).unwrap();
    let mut changed = make();
    changed.sources = Arc::new(changed.sources.as_ref().clone());
    assert_eq!(
        changed.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let mut changed = make();
    Arc::make_mut(&mut changed.sources).clear();
    assert_eq!(
        changed.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let mut changed = make();
    changed.document = Arc::new((*changed.document).clone());
    assert_eq!(
        changed.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let mut changed = make();
    changed.session += 1;
    assert_eq!(
        changed.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let foreign = ProjectStore::create(&directory.path().join("other.deadpan"), &empty()).unwrap();
    let mut changed = make();
    changed.originals = foreign.original_import_handle().unwrap();
    assert_eq!(
        changed.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let foreign_base = Snapshot::committed(
        base.session,
        base.document.clone(),
        Default::default(),
        foreign.original_import_handle().unwrap(),
    );
    assert!(matches!(
        Snapshot::proposed_edit_slice(&foreign_base, view.clone(), 2, 1),
        Err(SnapshotError::InvalidAdmission)
    ));
    let proposal = Arc::new(make());
    let mut sources = Sources::new(proposal.clone());
    let asset = AssetId::new("original").unwrap();
    sources
        .source(
            proposal.document.project_id(),
            proposal.document.revision_id(),
            &asset,
            &cancelled(),
        )
        .unwrap();
    drop(store);
    assert!(
        sources
            .source(
                proposal.document.project_id(),
                proposal.document.revision_id(),
                &asset,
                &cancelled()
            )
            .is_err(),
        "warm edited PCM must observe revocation"
    );
    assert_eq!(
        proposal.validate_admission(),
        Ok(()),
        "identity survives revocation"
    );
    assert!(
        proposal.check_media_live(&cancelled()).is_err(),
        "live authority does not"
    );
    assert!(Snapshot::proposed_edit_slice(&base, view.clone(), 2, 2).is_err());
    let reopened = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite).unwrap();
    let reopened_base = snapshot(&reopened, base.session);
    assert!(matches!(
        Snapshot::proposed_edit_slice(&reopened_base, view, 2, 3),
        Err(SnapshotError::InvalidAdmission)
    ));
}

#[test]
fn large_catalog_is_checked_once_then_hot_admission_and_reads_keep_constant_work() {
    let _permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("catalog.deadpan"), &empty()).unwrap();
    register(&mut store);
    let original = snapshot(&store, 183);
    let asset = AssetId::new("original").unwrap();
    let entry = original.sources[&asset].clone();
    let mut wire = serde_json::to_value(original.document.as_ref()).unwrap();
    let mut catalog = original.sources.as_ref().clone();
    for i in 0..511 {
        let id = AssetId::new(format!("catalog-{i}")).unwrap();
        wire["assets"][id.as_str()] =
            serde_json::to_value(&original.document.assets()[&asset]).unwrap();
        catalog.insert(id, entry.clone());
    }
    let base = Snapshot::committed(
        original.session,
        Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap()),
        catalog,
        original.originals.clone(),
    );
    wire["revision_id"] = json!("large-proposal");
    let document = Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap());
    crate::sources::CATALOG_ENTRIES_CHECKED.with(|count| count.set(0));
    let proposed = Arc::new(Snapshot::proposed(&base, document, 3, 1).unwrap());
    assert_eq!(crate::sources::CATALOG_ENTRIES_CHECKED.get(), 512);
    let mut sources = Sources::new(proposed.clone());
    for _ in 0..16 {
        proposed.validate_admission().unwrap();
        sources
            .source(
                proposed.document.project_id(),
                proposed.document.revision_id(),
                &asset,
                &cancelled(),
            )
            .unwrap();
    }
    assert_eq!(
        crate::sources::CATALOG_ENTRIES_CHECKED.get(),
        512,
        "warm admission must never scan the full catalog"
    );
}

#[test]
fn closed_original_revokes_reused_loop_batches() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("loop.deadpan"), &hold(4)).unwrap();
    let base = snapshot(&store, 184);
    let slice = capture(&base.document);
    let view = Arc::new(
        store
            .preview_edit_slice(&request(&base, &slice, "loop-proposal"))
            .unwrap(),
    );
    let captured = Arc::new(Snapshot::proposed_edit_slice(&base, view, 4, 1).unwrap());
    assert!(captured.sources.is_empty());
    assert_eq!(
        captured.validate_original_proposal(),
        Err(SnapshotError::InvalidAdmission),
        "an empty edited catalog must not enter the strict Original path"
    );
    let original = Snapshot::proposed(&base, captured.document.clone(), 5, 1).unwrap();
    assert_eq!(original.validate_original_proposal(), Ok(()));
    assert_eq!(
        base.validate_original_proposal(),
        Err(SnapshotError::InvalidAdmission)
    );
    let (engine, devices) = engine(&permit);
    let (entered, admitted) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let released = Mutex::new(released);
    *engine.shared.preparation_observer.lock().unwrap() = Some(Arc::new(move |event, _| {
        if event == crate::preparation::PreparationEvent::PlaybackBatchPrepared {
            entered.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
    }));
    engine
        .play_window(
            1,
            captured.clone(),
            Target::Sequence,
            Window::new(AudioSample(0), AudioSample(8), true).unwrap(),
            AudioSample(0),
            0.25,
        )
        .unwrap();
    // The complete batch already contains reused short-loop laps, without a
    // single source lookup. Close precisely before it can be published.
    admitted.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(store);
    release.send(()).unwrap();
    let failed = update(&engine, Phase::Failed);
    assert!(failed.error.unwrap().contains("closed"));
    assert!(
        devices
            .lock()
            .unwrap()
            .iter()
            .all(|device| !device.started.load(Ordering::Acquire))
    );
}
