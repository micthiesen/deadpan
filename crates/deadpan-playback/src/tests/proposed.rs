//! Proposed documents carry distinct render identity while their real media
//! admission remains anchored in one captured, unchanged committed revision.
use super::*;

#[path = "proposed/edited_slice.rs"]
mod edited_slice;

fn gain_document(
    store: &ProjectStore,
    base: &Snapshot,
    name: &str,
    millidecibels: i32,
) -> Arc<ProjectDocument> {
    let request = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::SetAudioTreatments {
            node: base.document.root().clone(),
            treatments: AudioTreatments::from_clip_gain(
                ClipGain::new(GainDb::new(millidecibels).unwrap(), false, vec![], vec![]).unwrap(),
            ),
        },
    };
    Arc::new(
        store
            .preview(&request)
            .unwrap()
            .forward
            .apply(&base.document)
            .unwrap(),
    )
}

fn reference(snapshot: &Arc<Snapshot>, start: AudioSample, count: u32) -> Vec<f32> {
    LimitedAudio::new(Arc::new(RenderPlan::compile(&snapshot.document).unwrap()))
        .read(
            &mut Sources::new(snapshot.clone()),
            start,
            count,
            Duration::from_secs(60),
            &cancelled(),
        )
        .unwrap()
        .samples
        .into_iter()
        .flatten()
        .map(|sample| sample * 0.25)
        .collect()
}

#[test]
fn proposal_admission_rejects_bad_identity_changed_assets_and_retargeted_public_fields() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::create(
        &directory.path().join("proposal-admission.deadpan"),
        &empty(),
    )
    .unwrap();
    register(&mut store);
    let base = snapshot(&store, 51);
    let document = gain_document(&store, &base, "proposal-a", -6000);
    assert!(matches!(
        Snapshot::proposed(&base, document.clone(), 0, 1),
        Err(SnapshotError::InvalidIdentity)
    ));
    assert!(matches!(
        Snapshot::proposed(&base, document.clone(), 1, 0),
        Err(SnapshotError::InvalidIdentity)
    ));
    assert!(matches!(
        Snapshot::proposed(&base, base.document.clone(), 1, 1),
        Err(SnapshotError::ReusedRevision)
    ));
    let proposal = Snapshot::proposed(&base, document.clone(), 7, 1).unwrap();
    assert_eq!(
        proposal.validate_proposed_base(base.session, &base.document),
        Ok(())
    );
    assert_eq!(
        proposal.validate_proposed_base(base.session + 1, &base.document),
        Err(SnapshotError::InvalidAdmission)
    );
    assert_eq!(
        proposal.validate_proposed_base(base.session, &Arc::new((*base.document).clone())),
        Err(SnapshotError::InvalidAdmission),
        "equal public project/revision fields cannot replace the captured base"
    );
    assert_eq!(
        base.validate_proposed_base(base.session, &base.document),
        Err(SnapshotError::InvalidAdmission),
        "a committed snapshot is not a genuine proposal"
    );
    assert!(matches!(
        Snapshot::proposed(&proposal, document.clone(), 7, 2),
        Err(SnapshotError::BaseNotCommitted)
    ));

    let mut wire = serde_json::to_value(document.as_ref()).unwrap();
    wire["assets"]["original"]["label"] = json!("Changed complete asset contract");
    let changed_asset = Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap());
    assert!(matches!(
        Snapshot::proposed(&base, changed_asset, 7, 1),
        Err(SnapshotError::ChangedAssetContracts)
    ));
    let mut wire = serde_json::to_value(document.as_ref()).unwrap();
    wire["project_id"] = json!("foreign-project");
    let foreign = Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap());
    assert!(matches!(
        Snapshot::proposed(&base, foreign, 7, 1),
        Err(SnapshotError::ForeignProject)
    ));

    // A different valid token cannot relabel an already admitted proposal.
    let mut retagged = Snapshot::proposed(&base, document.clone(), 7, 1).unwrap();
    retagged.content = ContentIdentity::Proposed {
        base_revision: base.document.revision_id().clone(),
        draft: 7,
        change: 2,
    };
    assert_eq!(
        retagged.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    assert_eq!(
        retagged.validate_proposed_base(base.session, &base.document),
        Err(SnapshotError::InvalidAdmission)
    );
    let mut moved = Snapshot::proposed(&base, document.clone(), 7, 1).unwrap();
    moved.session += 1;
    assert_eq!(
        moved.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let mut replaced = Snapshot::proposed(&base, document.clone(), 7, 1).unwrap();
    replaced.document = Arc::new(document.as_ref().clone());
    assert_eq!(
        replaced.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    let project = replaced.document.project_id().clone();
    let proposed_revision = replaced.document.revision_id().clone();
    let mut sources = Sources::new(Arc::new(replaced));
    let error = sources
        .source(
            &project,
            &proposed_revision,
            &AssetId::new("original").unwrap(),
            &cancelled(),
        )
        .err()
        .expect("replaced proposal document must reject");
    assert!(error.to_string().contains("proposal admission"));

    // Committed versus Proposed must participate even with the same document,
    // receipt Arc and Original objects. The constructor alone cannot supply a
    // cache key that hides the content domain.
    let proposal = Arc::new(proposal);
    let committed_shape = Snapshot::committed(
        base.session,
        document.clone(),
        base.sources.as_ref().clone(),
        base.originals.clone(),
    );
    let sources = Sources::new(proposal.clone());
    assert!(sources.matches(&proposal));
    assert!(!sources.matches(&committed_shape));
    let other_change = Snapshot::proposed(&base, document.clone(), 7, 2).unwrap();
    assert!(!sources.matches(&other_change));
    let mut forged = committed_shape;
    forged.content = proposal.content.clone();
    assert_eq!(
        forged.validate_admission(),
        Err(SnapshotError::InvalidAdmission)
    );
    assert!(!sources.matches(&forged));

    // Invalid admission fails on the worker and keeps its exact requested tag.
    let requested_content = retagged.content.clone();
    let (engine, _) = engine(&permit);
    engine
        .play(70, Arc::new(retagged), AudioSample(0), 0.25)
        .unwrap();
    let failure = update(&engine, Phase::Failed);
    assert_eq!(failure.ticket, 70);
    assert_eq!(failure.session, 51);
    assert_eq!(failure.content, requested_content);
    assert_eq!(failure.revision_id, *document.revision_id());
    assert!(failure.error.unwrap().contains("proposal admission"));
    assert_eq!(store.snapshot().unwrap(), *base.document);
    assert!(store.snapshot_at(document.revision_id()).is_err());
}

#[test]
fn two_gain_proposals_and_before_have_separate_real_pcm_and_tagged_device_generations() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("proposal-pcm.deadpan"), &empty()).unwrap();
    register(&mut store);
    let base = snapshot(&store, 52);
    let before_wire = base.document.to_json().unwrap();
    let first_document = gain_document(&store, &base, "draft-gain-one", -6000);
    let second_document = gain_document(&store, &base, "draft-gain-two", -18000);
    let first = Arc::new(Snapshot::proposed(&base, first_document.clone(), 23, 1).unwrap());
    let second = Arc::new(Snapshot::proposed(&base, second_document.clone(), 23, 2).unwrap());
    assert!(Arc::ptr_eq(
        &first.sources[&AssetId::new("original").unwrap()].receipt,
        &base.sources[&AssetId::new("original").unwrap()].receipt
    ));
    assert!(store.snapshot_at(first_document.revision_id()).is_err());
    assert!(store.snapshot_at(second_document.revision_id()).is_err());

    let start = AudioSample(160);
    let count = 128;
    let expected = [
        reference(&base, start, count),
        reference(&first, start, count),
        reference(&second, start, count),
    ];
    assert_ne!(expected[0], expected[1]);
    assert_ne!(expected[1], expected[2]);
    let (engine, devices) = engine(&permit);
    let window = Window::new(AudioSample(100), AudioSample(600), false).unwrap();
    let mut previous: Option<(Arc<Fake>, deadpan_output::Generation)> = None;
    for (index, (content, expected)) in [
        (&base, &expected[0]),
        (&first, &expected[1]),
        (&second, &expected[2]),
        (&base, &expected[0]),
        (&first, &expected[1]),
    ]
    .into_iter()
    .enumerate()
    {
        let ticket = index as u64 + 1;
        engine
            .play_window(
                ticket,
                content.clone(),
                Target::Sequence,
                window,
                start,
                0.25,
            )
            .unwrap();
        let device = playing_device(&engine, &devices, index);
        let preparing = engine.poll().unwrap();
        assert_eq!(preparing.phase, Phase::Preparing);
        assert_eq!(preparing.content, content.content);
        assert_eq!(preparing.revision_id, *content.document.revision_id());
        assert_eq!(preparing.sample, None);
        if let Some((old, _)) = &previous {
            let mut after_stop = [1.0; 32];
            assert_eq!(
                old.callback
                    .lock()
                    .unwrap()
                    .render(&mut after_stop)
                    .rendered_frames,
                0
            );
            assert!(after_stop.iter().all(|sample| *sample == 0.0));
        }
        let (report, pcm) = device.render(count as usize, 0, 10_000_000);
        assert_eq!(&pcm, expected);
        assert_eq!(report.first_sample, Some(start.0));
        if let Some((_, generation)) = &previous {
            assert_ne!(report.generation, *generation);
        }
        device.now.store(10_000_000, Ordering::Release);
        let playing = update(&engine, Phase::Playing);
        assert_eq!(playing.ticket, ticket);
        assert_eq!(playing.content, content.content);
        assert_eq!(playing.revision_id, *content.document.revision_id());
        assert_eq!(playing.sample, Some(start));
        previous = Some((device, report.generation));
    }
    engine.stop();
    let stopped = update(&engine, Phase::Stopped);
    assert_eq!(stopped.content, first.content);
    assert_eq!(stopped.revision_id, *first.document.revision_id());
    assert_eq!(base.content, ContentIdentity::Committed);
    assert_eq!(base.document.to_json().unwrap(), before_wire);
    assert_eq!(store.snapshot().unwrap(), *base.document);
}

#[test]
fn proposed_source_failure_is_tagged_and_cannot_poison_later_before_playback() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("proposal-failure.deadpan"), &empty()).unwrap();
    register(&mut store);
    let base = snapshot(&store, 53);
    let document = gain_document(&store, &base, "failed-draft", -6000);
    let mut failed = Snapshot::proposed(&base, document.clone(), 28, 1).unwrap();
    Arc::make_mut(&mut failed.sources).clear();
    let failed = Arc::new(failed);
    let expected = reference(&base, AudioSample(160), 128);
    let (engine, devices) = engine(&permit);
    engine
        .play(1, failed.clone(), AudioSample(160), 0.25)
        .unwrap();
    let error = update(&engine, Phase::Failed);
    assert_eq!(error.content, failed.content);
    assert_eq!(error.revision_id, *document.revision_id());
    assert!(error.error.unwrap().contains("proposal admission"));
    engine
        .play(2, base.clone(), AudioSample(160), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 1);
    let (_, pcm) = device.render(128, 0, 10_000_000);
    assert_eq!(pcm, expected);
    device.now.store(10_000_000, Ordering::Release);
    let recovered = update(&engine, Phase::Playing);
    assert_eq!(recovered.content, ContentIdentity::Committed);
    assert_eq!(recovered.ticket, 2);
    assert_eq!(recovered.revision_id, *base.document.revision_id());
    assert_eq!(store.snapshot().unwrap(), *base.document);
}
