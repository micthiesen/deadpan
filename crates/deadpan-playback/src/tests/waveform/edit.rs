//! Admitted Before/Proposed windows use the production idle preparation lane.
use super::*;
use deadpan_audio::{EditWaveformStage, WaveformLimits};

#[path = "edit/admission.rs"]
mod admission;

fn edit_request(
    base: &Arc<Snapshot>,
    candidate: &Arc<Snapshot>,
    start: i64,
    end: i64,
) -> EditWaveformRequest {
    EditWaveformRequest {
        base: base.clone(),
        snapshot: candidate.clone(),
        samples: AudioSample(start)..AudioSample(end),
        limits: WaveformLimits::default(),
    }
}

fn edit_terminal(engine: &Engine, ticket: WaveformTicket) -> EditWaveformUpdate {
    let mut result = None;
    assert!(wait(|| {
        // Wrong-kind polling must not take the shared mailbox's Edit reply.
        assert!(engine.poll_waveform().is_none());
        if let Some(update) = engine.poll_edit_waveform() {
            assert_eq!(
                update.ticket, ticket,
                "superseded analysis must not publish"
            );
            if matches!(
                update.status,
                WaveformStatus::Complete
                    | WaveformStatus::Partial
                    | WaveformStatus::Interrupted
                    | WaveformStatus::Unavailable
            ) {
                result = Some(update);
                return true;
            }
        }
        false
    }));
    result.unwrap()
}

fn proposal(
    store: &ProjectStore,
    base: &Arc<Snapshot>,
    name: &str,
    db: i32,
    mute: bool,
    change: u64,
) -> Arc<Snapshot> {
    let request = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::SetAudioTreatments {
            node: node("root"),
            treatments: AudioTreatments::from_clip_gain(
                ClipGain::new(GainDb::new(db).unwrap(), mute, vec![], vec![]).unwrap(),
            ),
        },
    };
    let document = Arc::new(
        store
            .preview(&request)
            .unwrap()
            .forward
            .apply(&base.document)
            .unwrap(),
    );
    Arc::new(Snapshot::proposed(base, document, 70, change).unwrap())
}

#[test]
fn exact_window_tags_before_and_real_gain_proposals_without_opening_output() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("edit-waveform.deadpan"), &empty()).unwrap();
    register(&mut store);
    let base = snapshot(&store, 121);
    let quieter = proposal(&store, &base, "waveform-quieter", -6000, false, 1);
    let muted = proposal(&store, &base, "waveform-muted", 0, true, 2);
    let (engine, devices) = engine(&permit);
    // media_probe.c authors an impulse at sample 48,000. This 513-sample
    // window includes that event in its first leaf and a one-sample last leaf;
    // the earlier 2,115..2,628 interval is silent in this MP4 fixture.
    let before = edit_terminal(
        &engine,
        engine
            .request_edit_waveform(edit_request(&base, &base, 47800, 48313))
            .unwrap(),
    );
    let after = edit_terminal(
        &engine,
        engine
            .request_edit_waveform(edit_request(&base, &quieter, 47800, 48313))
            .unwrap(),
    );
    let mute = edit_terminal(
        &engine,
        engine
            .request_edit_waveform(edit_request(&base, &muted, 47800, 48313))
            .unwrap(),
    );
    for (update, snapshot) in [(&before, &base), (&after, &quieter), (&mute, &muted)] {
        assert_eq!(update.status, WaveformStatus::Complete);
        assert_eq!(update.session, 121);
        assert_eq!(update.base_revision, *base.document.revision_id());
        assert_eq!(update.revision_id, *snapshot.document.revision_id());
        assert_eq!(update.content, snapshot.content);
        assert_eq!(update.samples, AudioSample(47800)..AudioSample(48313));
        assert_eq!(update.examined_samples, 513);
        let peaks = update.waveform.as_ref().unwrap();
        assert_eq!(
            peaks.descriptor().stage,
            EditWaveformStage::AuthoredBusBeforeLimiter
        );
        assert_eq!(peaks.measured_end(), AudioSample(48313));
        assert_eq!(
            peaks.bin_samples(0, 0),
            Some(AudioSample(47800)..AudioSample(48056))
        );
        assert_eq!(
            peaks.bin_samples(0, 1),
            Some(AudioSample(48056)..AudioSample(48312))
        );
        assert_eq!(
            peaks.bin_samples(0, 2),
            Some(AudioSample(48312)..AudioSample(48313))
        );
    }
    let before = before.waveform.unwrap();
    let after = after.waveform.unwrap();
    let mute = mute.waveform.unwrap();
    assert!(
        before
            .level(0)
            .unwrap()
            .iter()
            .any(|p| p.minimum() != [0.0; 2] || p.maximum() != [0.0; 2])
    );
    let impulse_bin = &before.level(0).unwrap()[0];
    assert!(impulse_bin.minimum() != [0.0; 2] || impulse_bin.maximum() != [0.0; 2]);
    let factor = 10_f64.powf(-6.0 / 20.0);
    for level in 0..before.level_count() {
        for ((a, b), m) in before
            .level(level)
            .unwrap()
            .iter()
            .zip(after.level(level).unwrap())
            .zip(mute.level(level).unwrap())
        {
            for channel in 0..2 {
                assert_eq!(
                    b.minimum()[channel].to_bits(),
                    ((f64::from(a.minimum()[channel]) * factor) as f32).to_bits()
                );
                assert_eq!(
                    b.maximum()[channel].to_bits(),
                    ((f64::from(a.maximum()[channel]) * factor) as f32).to_bits()
                );
            }
            assert_eq!(m.minimum(), [0.0; 2]);
            assert_eq!(m.maximum(), [0.0; 2]);
        }
    }
    assert!(devices.lock().unwrap().is_empty());
    assert_eq!(engine.poll(), None);
    assert_eq!(store.snapshot().unwrap(), *base.document);
    assert!(store.snapshot_at(quieter.document.revision_id()).is_err());
}

#[test]
fn proposed_window_requires_the_exact_captured_base_and_private_admission() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("edit-admission.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 122);
    let candidate = proposal(&store, &base, "edit-admission-proposal", -6000, false, 1);
    let (engine, devices) = engine(&permit);
    assert!(matches!(
        engine.request_waveform(WaveformRequest {
            snapshot: candidate.clone(),
            owner: node("root")
        }),
        Err(WaveformRequestError::Proposed)
    ));
    let later_head = Arc::new(Snapshot::committed(
        base.session,
        candidate.document.clone(),
        base.sources.as_ref().clone(),
        base.originals.clone(),
    ));
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&later_head, &candidate, 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    let equal_but_different_base = snapshot(&store, 122);
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&equal_but_different_base, &candidate, 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&candidate, &candidate, 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&base, &equal_but_different_base, 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    let mut forged = Snapshot::proposed(&base, candidate.document.clone(), 70, 1).unwrap();
    forged.content = ContentIdentity::Proposed {
        base_revision: base.document.revision_id().clone(),
        draft: 70,
        change: 2,
    };
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&base, &Arc::new(forged), 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    let mut replaced_receipts =
        Snapshot::proposed(&base, candidate.document.clone(), 70, 1).unwrap();
    replaced_receipts.sources = Arc::new(base.sources.as_ref().clone());
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&base, &Arc::new(replaced_receipts), 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    let mut moved = Snapshot::proposed(&base, candidate.document.clone(), 70, 1).unwrap();
    moved.session += 1;
    assert!(matches!(
        engine.request_edit_waveform(edit_request(&base, &Arc::new(moved), 0, 256)),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    for (start, end) in [(-1, 100), (100, 99)] {
        assert_eq!(
            engine.request_edit_waveform(edit_request(&base, &candidate, start, end)),
            Err(WaveformRequestError::InvalidRange)
        );
    }
    let failed = edit_terminal(
        &engine,
        engine
            .request_edit_waveform(edit_request(&base, &candidate, 1599, 1601))
            .unwrap(),
    );
    assert_eq!(failed.status, WaveformStatus::Unavailable);
    assert!(failed.waveform.is_none());
    assert_eq!(failed.samples, AudioSample(1599)..AudioSample(1601));
    assert_eq!(failed.content, candidate.content);
    assert!(failed.error.unwrap().contains("outside"));
    assert!(devices.lock().unwrap().is_empty());
}

#[test]
fn definition_and_edit_replacements_share_one_lane_and_reject_stale_results() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("edit-replacement.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 123);
    let (engine, devices) = engine(&permit);
    let (gate, events) = block_first_analysis(&engine);
    let first = engine
        .request_edit_waveform(edit_request(&base, &base, 100, 613))
        .unwrap();
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    let invalid = request(&engine, base.clone(), "missing-owner");
    let latest = engine
        .request_edit_waveform(edit_request(&base, &base, 200, 713))
        .unwrap();
    assert!(first.value() < invalid.value() && invalid.value() < latest.value());
    engine.cancel_waveform(first);
    gate.release();
    let latest = edit_terminal(&engine, latest);
    assert_eq!(latest.status, WaveformStatus::Complete);
    assert_eq!(latest.samples, AudioSample(200)..AudioSample(713));
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| **event == PreparationEvent::WaveformAdmitted)
            .count(),
        2
    );
    // The reverse replacement must also keep its reply out of the wrong API.
    let (gate, _) = block_first_analysis(&engine);
    let stale = engine
        .request_edit_waveform(edit_request(&base, &base, 100, 613))
        .unwrap();
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    let definition = request(&engine, base, "root");
    engine.cancel_waveform(stale);
    gate.release();
    assert!(engine.poll_edit_waveform().is_none());
    assert_eq!(
        terminal(&engine, definition).status,
        WaveformStatus::Complete
    );
    assert!(devices.lock().unwrap().is_empty());
}

#[test]
fn window_analysis_releases_cache_before_playback_and_keeps_partial_labels_absolute() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("edit-priority.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 124);
    let (engine, devices) = engine(&permit);
    let mut request = edit_request(&base, &base, 100, 869);
    request.limits = WaveformLimits::new(513, 4096, Duration::from_secs(10)).unwrap();
    let partial = edit_terminal(&engine, engine.request_edit_waveform(request).unwrap());
    assert_eq!(partial.status, WaveformStatus::Partial);
    assert_eq!(partial.examined_samples, 513);
    let partial = partial.waveform.unwrap();
    assert_eq!(partial.measured_end(), AudioSample(612));
    assert_eq!(
        partial.bin_samples(0, 1),
        Some(AudioSample(356)..AudioSample(612))
    );
    let empty = edit_terminal(
        &engine,
        engine
            .request_edit_waveform(edit_request(&base, &base, 1600, 1600))
            .unwrap(),
    );
    assert_eq!(empty.status, WaveformStatus::Complete);
    assert_eq!(empty.examined_samples, 0);
    assert_eq!(empty.waveform.unwrap().measured_end(), AudioSample(1600));
    let (gate, events) = block_first_analysis(&engine);
    let ticket = engine
        .request_edit_waveform(edit_request(&base, &base, 100, 613))
        .unwrap();
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    engine.play(99, base, AudioSample(0), 0.01).unwrap();
    assert!(wait(|| gate.cancelled.load(Ordering::Acquire)));
    assert_eq!(
        edit_terminal(&engine, ticket).status,
        WaveformStatus::Interrupted
    );
    assert!(
        !events
            .lock()
            .unwrap()
            .contains(&PreparationEvent::PlaybackAdmitted)
    );
    gate.release();
    playing_device(&engine, &devices, 0);
    let events = events.lock().unwrap();
    let release = events
        .iter()
        .position(|v| *v == PreparationEvent::WaveformReleased)
        .unwrap();
    let playback = events
        .iter()
        .position(|v| *v == PreparationEvent::PlaybackAdmitted)
        .unwrap();
    assert!(release < playback);
}
