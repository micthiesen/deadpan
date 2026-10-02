//! Revocation is independent of snapshot identity and of source reads.

use super::*;

fn silent_slice(store: &ProjectStore, base: &Arc<Snapshot>) -> Arc<Snapshot> {
    let slice = CapturedEditSlice::capture(
        &base.document,
        base.document.root(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
        AudioTimingId {
            allocation: revision("waveform-capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let required = slice.identity_requirements().unwrap();
    let request = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision("waveform-slice"),
        command: Command::SpliceSlice {
            parent: base.document.root().clone(),
            index: 0,
            slice,
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|i| node(&format!("waveform-slice-node-{i}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|i| MarkId::new(format!("waveform-slice-mark-{i}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|i| node(&format!("waveform-slice-alias-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("waveform-slice"),
                ordinal: 0,
            },
        },
    };
    let view = Arc::new(store.preview_edit_slice(&request).unwrap());
    let proposed = Arc::new(Snapshot::proposed_edit_slice(base, view, 81, 1).unwrap());
    assert!(
        proposed.sources.is_empty(),
        "no source read can check liveness"
    );
    assert_eq!(proposed.validate_admission(), Ok(()));
    proposed.check_media_live(&cancelled()).unwrap();
    proposed
}

fn block_event(engine: &Engine, boundary: PreparationEvent) -> Arc<Gate> {
    let gate = Arc::new(Gate::default());
    let retained = gate.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == boundary {
                retained.block(cancelled);
            }
        }));
    gate
}

fn assert_revoked(
    update: &EditWaveformUpdate,
    ticket: WaveformTicket,
    base: &Snapshot,
    proposed: &Snapshot,
    end: i64,
) {
    assert_eq!(update.ticket, ticket);
    assert_eq!(update.session, proposed.session);
    assert_eq!(update.project_id, *proposed.document.project_id());
    assert_eq!(update.base_revision, *base.document.revision_id());
    assert_eq!(update.revision_id, *proposed.document.revision_id());
    assert_eq!(update.content, proposed.content);
    assert_eq!(update.samples, AudioSample(100)..AudioSample(end));
    assert_eq!(update.status, WaveformStatus::Unavailable);
    assert!(update.waveform.is_none());
    assert_eq!(update.examined_samples, 0);
    let error = update.error.as_deref().unwrap();
    assert!(error.contains("media admission expired"), "{error}");
    assert!(error.contains("closed"), "{error}");
}

#[test]
fn revoked_silent_and_empty_proposals_fail_before_worker_admission() {
    let permit = resources::pcm();
    for end in [613, 100] {
        let directory = tempfile::tempdir().unwrap();
        let store =
            ProjectStore::create(&directory.path().join("revoked.deadpan"), &hold(4)).unwrap();
        let base = snapshot(&store, 141);
        let proposed = silent_slice(&store, &base);
        let (engine, devices) = engine(&permit);
        let admitted = Arc::new(AtomicBool::new(false));
        let observed = admitted.clone();
        *engine.shared.preparation_observer.lock().unwrap() = Some(Arc::new(move |event, _| {
            if event == PreparationEvent::WaveformAdmitted {
                observed.store(true, Ordering::Release);
            }
        }));
        drop(store);
        assert_eq!(proposed.validate_admission(), Ok(()));
        assert!(proposed.check_media_live(&cancelled()).is_err());
        // Synchronous validation checks captured identity only. Authority is
        // checked on the worker, including the no-bus-read empty case.
        let request = edit_request(&base, &proposed, 100, end);
        request.validate().unwrap();
        let ticket = engine.request_edit_waveform(request).unwrap();
        let update = edit_terminal(&engine, ticket);
        assert_revoked(&update, ticket, &base, &proposed, end);
        assert!(!admitted.load(Ordering::Acquire));
        assert!(devices.lock().unwrap().is_empty());
    }
}

#[test]
fn revocation_after_admission_or_measurement_discards_silent_empty_and_partial_results() {
    let permit = resources::pcm();
    for boundary in [
        PreparationEvent::WaveformAdmitted,
        PreparationEvent::WaveformReleased,
    ] {
        for (end, maximum_samples) in [(613, 513), (100, 513), (869, 513)] {
            let directory = tempfile::tempdir().unwrap();
            let store = ProjectStore::create(
                &directory.path().join("during-measurement.deadpan"),
                &hold(4),
            )
            .unwrap();
            let base = snapshot(&store, 142);
            let proposed = silent_slice(&store, &base);
            let (engine, devices) = engine(&permit);
            let gate = Arc::new(Gate::default());
            let retained_gate = gate.clone();
            let published = Arc::new(AtomicBool::new(false));
            let retained_published = published.clone();
            *engine.shared.preparation_observer.lock().unwrap() =
                Some(Arc::new(move |event, cancelled| {
                    if event == boundary {
                        retained_gate.block(cancelled);
                    }
                    if event == PreparationEvent::EditWaveformPublished {
                        retained_published.store(true, Ordering::Release);
                    }
                }));
            let mut request = edit_request(&base, &proposed, 100, end);
            request.limits =
                WaveformLimits::new(maximum_samples, 4096, Duration::from_secs(10)).unwrap();
            let ticket = engine.request_edit_waveform(request).unwrap();
            assert!(wait(|| gate.entered.load(Ordering::Acquire)));
            drop(store);
            assert_eq!(proposed.validate_admission(), Ok(()));
            gate.release();
            assert!(wait(|| published.load(Ordering::Acquire)));
            // Inspect the worker mailbox directly so poll's independent guard
            // cannot conceal a missing terminal authority check.
            let raw = engine.shared.lock().waveform.edit_reply_for_test().unwrap();
            assert_revoked(&raw, ticket, &base, &proposed, end);
            let update = edit_terminal(&engine, ticket);
            assert_revoked(&update, ticket, &base, &proposed, end);
            assert!(devices.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn poll_revokes_already_queued_silent_and_empty_success_without_changing_identity() {
    let permit = resources::pcm();
    for end in [613, 100] {
        let directory = tempfile::tempdir().unwrap();
        let store =
            ProjectStore::create(&directory.path().join("queued.deadpan"), &hold(4)).unwrap();
        let base = snapshot(&store, 143);
        let proposed = silent_slice(&store, &base);
        let (engine, devices) = engine(&permit);
        // This event follows terminal publication, while the admitting store
        // is still open. The complete peak DTO is already in the mailbox.
        let gate = block_event(&engine, PreparationEvent::EditWaveformPublished);
        let ticket = engine
            .request_edit_waveform(edit_request(&base, &proposed, 100, end))
            .unwrap();
        assert!(wait(|| gate.entered.load(Ordering::Acquire)));
        let queued = engine.shared.lock().waveform.edit_reply_for_test().unwrap();
        assert_eq!(queued.status, WaveformStatus::Complete);
        assert!(queued.waveform.is_some());
        assert_eq!(queued.examined_samples, u64::try_from(end - 100).unwrap());
        drop(queued);
        drop(store);
        assert_eq!(proposed.validate_admission(), Ok(()));
        // No worker remains able to replace this already-published success.
        assert_eq!(
            engine
                .shared
                .lock()
                .waveform
                .edit_reply_for_test()
                .unwrap()
                .status,
            WaveformStatus::Complete
        );
        assert!(engine.poll_waveform().is_none());
        let update = engine.poll_edit_waveform().unwrap();
        gate.release();
        assert_revoked(&update, ticket, &base, &proposed, end);
        assert!(engine.poll_edit_waveform().is_none());
        assert!(devices.lock().unwrap().is_empty());
    }
}

#[test]
fn revocation_before_progress_publication_hides_new_peaks_and_terminal_coverage() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("progress.deadpan"), &hold(4)).unwrap();
    let base = snapshot(&store, 144);
    let proposed = silent_slice(&store, &base);
    let (engine, devices) = engine(&permit);
    let progress = Arc::new(Gate::default());
    let released = Arc::new(Gate::default());
    let progress_gate = progress.clone();
    let release_gate = released.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| match event {
            PreparationEvent::EditWaveformProgress => progress_gate.block(cancelled),
            PreparationEvent::WaveformReleased => release_gate.block(cancelled),
            _ => {}
        }));
    let ticket = engine
        .request_edit_waveform(edit_request(&base, &proposed, 100, 869))
        .unwrap();
    assert!(wait(|| progress.entered.load(Ordering::Acquire)));
    drop(store);
    progress.release();
    assert!(wait(|| released.entered.load(Ordering::Acquire)));
    // Hold terminal publication so an invalid progress DTO cannot be hidden
    // by a later Unavailable result in the same one-slot mailbox.
    let raw = engine.shared.lock().waveform.edit_reply_for_test().unwrap();
    assert_revoked(&raw, ticket, &base, &proposed, 869);
    let update = engine.poll_edit_waveform().unwrap();
    released.release();
    assert_revoked(&update, ticket, &base, &proposed, 869);
    let terminal = edit_terminal(&engine, ticket);
    assert_revoked(&terminal, ticket, &base, &proposed, 869);
    assert!(devices.lock().unwrap().is_empty());
}

#[test]
fn committed_and_base_only_silent_windows_keep_existing_store_close_semantics() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("base-only.deadpan"), &hold(4)).unwrap();
    let base = snapshot(&store, 145);
    let proposed = proposal(&store, &base, "base-only-proposal", -6000, false, 1);
    drop(store);
    assert!(base.originals.check_live(&cancelled()).is_err());
    let (engine, devices) = engine(&permit);
    for candidate in [&base, &proposed] {
        candidate.check_media_live(&cancelled()).unwrap();
        let ticket = engine
            .request_edit_waveform(edit_request(&base, candidate, 100, 613))
            .unwrap();
        let update = edit_terminal(&engine, ticket);
        assert_eq!(update.status, WaveformStatus::Complete);
        assert_eq!(update.examined_samples, 513);
        let peaks = update.waveform.unwrap();
        assert_eq!(peaks.measured_end(), AudioSample(613));
        assert!(
            peaks
                .level(0)
                .unwrap()
                .iter()
                .all(|bin| { bin.minimum() == [0.0; 2] && bin.maximum() == [0.0; 2] })
        );
    }
    assert!(devices.lock().unwrap().is_empty());
}
