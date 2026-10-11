use super::*;
use crate::preparation::PreparationEvent;

fn warmed(engine: &Engine, ticket: WarmTicket) -> WarmUpdate {
    // The idle horizon has one 60-second worker budget; debug canonical DSP
    // can legitimately exceed the short controller-transition test timeout.
    let deadline = Instant::now() + Duration::from_secs(70);
    loop {
        let last = engine.audio_warming(ticket);
        if last
            .as_ref()
            .is_some_and(|u| !matches!(u.status, WarmStatus::Queued | WarmStatus::Preparing))
        {
            return last.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "warming did not finish: {last:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn idle_warming_never_opens_a_device_and_bounds_the_output_horizon() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("idle.deadpan"), &hold(1800)).unwrap();
    let snapshot = snapshot(&store, 119);
    let (engine, devices) = engine(&permit);
    let ticket = engine
        .warm_audio(snapshot, Target::Sequence, AudioSample(1600))
        .unwrap();
    let complete = warmed(&engine, ticket);
    assert_eq!(complete.status, WarmStatus::Complete);
    assert_eq!(
        complete.prepared_until,
        AudioSample(1600 + WARM_HORIZON_FRAMES)
    );
    assert!(devices.lock().unwrap().is_empty());
    assert_eq!(engine.diagnostics().generations, 0);
    assert!(
        engine.poll().is_none(),
        "warming cannot publish a transport clock"
    );
}

#[test]
fn matching_playback_finishes_the_current_warm_read_and_reuses_its_admission() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("handoff.deadpan"), &hold(1800)).unwrap();
    let snapshot = snapshot(&store, 120);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let interrupted = Arc::new(AtomicBool::new(false));
    let admits = Arc::new(AtomicU64::new(0));
    let (in_gate, out_gate, was_cancelled, admissions) = (
        entered.clone(),
        release.clone(),
        interrupted.clone(),
        admits.clone(),
    );
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::PlaybackAdmitted {
                admissions.fetch_add(1, Ordering::Relaxed);
            }
            if event == PreparationEvent::WarmingBatchPrepared
                && !in_gate.swap(true, Ordering::AcqRel)
            {
                assert!(wait(
                    || out_gate.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire)
                ));
                was_cancelled.store(cancelled.load(Ordering::Acquire), Ordering::Release);
            }
        }));
    let ticket = engine
        .warm_audio(snapshot.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    assert!(devices.lock().unwrap().is_empty());
    engine.play(1, snapshot, AudioSample(0), 0.25).unwrap();
    release.store(true, Ordering::Release);
    let device = playing_device(&engine, &devices, 0);
    assert!(!interrupted.load(Ordering::Acquire));
    assert_eq!(
        admits.load(Ordering::Acquire),
        0,
        "matching play must retain the worker cache"
    );
    assert_eq!(
        engine.audio_warming(ticket).unwrap().prepared_until,
        AudioSample(8192)
    );
    let (report, pcm) = device.render(256, 0, 0);
    assert_eq!(report.rendered_frames, 256);
    assert!(pcm.iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_new_cursor_cancels_running_warming_and_only_the_latest_request_finishes() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("replace.deadpan"), &hold(900)).unwrap();
    let snapshot = snapshot(&store, 121);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let gate = entered.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::WarmingAdmitted && !gate.swap(true, Ordering::AcqRel) {
                assert!(wait(|| cancelled.load(Ordering::Acquire)));
            }
        }));
    let first = engine
        .warm_audio(snapshot.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    let latest = engine
        .warm_audio(snapshot, Target::Sequence, AudioSample(48_000))
        .unwrap();
    assert!(engine.audio_warming(first).is_none());
    assert_eq!(warmed(&engine, latest).status, WarmStatus::Complete);
    assert!(devices.lock().unwrap().is_empty());
}

#[test]
fn idle_preparation_failure_never_starts_output_and_stop_revokes_pending_work() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("failure.deadpan"), &hold(30)).unwrap();
    let snapshot = snapshot(&store, 122);
    let (engine, devices) = engine(&permit);
    let ticket = engine
        .warm_audio(snapshot.clone(), Target::Sequence, AudioSample(99_999))
        .unwrap();
    let failure = warmed(&engine, ticket);
    assert_eq!(failure.status, WarmStatus::Unavailable);
    assert!(
        failure
            .error
            .unwrap()
            .contains("outside the captured audio")
    );
    assert!(devices.lock().unwrap().is_empty());
    let ticket = engine
        .warm_audio(snapshot, Target::Sequence, AudioSample(0))
        .unwrap();
    engine.stop();
    assert_eq!(
        engine.audio_warming(ticket).unwrap().status,
        WarmStatus::Interrupted
    );
    assert_eq!(engine.diagnostics().generations, 0);
}

#[test]
fn warm_real_source_preserves_canonical_pcm_and_current_media_admission() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::create(&directory.path().join("pcm.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 123);
    let mut sources = Sources::new(snapshot.clone());
    let mut canonical =
        LimitedAudio::new(Arc::new(RenderPlan::compile(&snapshot.document).unwrap()));
    let expected = canonical
        .read(
            &mut sources,
            AudioSample(160),
            256,
            Duration::from_secs(60),
            &cancelled(),
        )
        .unwrap();
    let (engine, devices) = engine(&permit);
    let ticket = engine
        .warm_audio(snapshot.clone(), Target::Sequence, AudioSample(160))
        .unwrap();
    assert_eq!(warmed(&engine, ticket).status, WarmStatus::Complete);
    assert!(devices.lock().unwrap().is_empty());
    engine.play(1, snapshot, AudioSample(160), 0.25).unwrap();
    let device = playing_device(&engine, &devices, 0);
    let (report, actual) = device.render(256, 0, 0);
    assert_eq!(report.first_sample, Some(160));
    assert_eq!(
        actual,
        expected
            .samples
            .iter()
            .flat_map(|frame| frame.map(|v| v * 0.25))
            .collect::<Vec<_>>()
    );
}

#[test]
fn power_policy_defers_optional_warming_without_gating_direct_playback() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("policy.deadpan"), &hold(30)).unwrap();
    let snapshot = snapshot(&store, 124);
    let checks = Arc::new(AtomicU64::new(0));
    let counted = checks.clone();
    let (engine, devices) = engine_with_guard(
        &permit,
        Arc::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
            Some("Low Power Mode is on".into())
        }),
    );
    let ticket = engine
        .warm_audio(snapshot.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    let deferred = warmed(&engine, ticket);
    assert_eq!(deferred.status, WarmStatus::Interrupted);
    assert_eq!(deferred.error.as_deref(), Some("Low Power Mode is on"));
    assert_eq!(checks.load(Ordering::Acquire), 1);
    assert!(devices.lock().unwrap().is_empty());
    engine
        .play(1, snapshot.clone(), AudioSample(0), 0.25)
        .unwrap();
    playing_device(&engine, &devices, 0);
    assert_eq!(checks.load(Ordering::Acquire), 1);
    assert_eq!(
        engine.warm_audio(snapshot, Target::Sequence, AudioSample(0)),
        Err(WarmRequestError::PlaybackActive)
    );
}

#[test]
fn foreign_playback_revokes_running_warming_before_new_admission() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("foreign.deadpan"), &hold(120)).unwrap();
    let captured = snapshot(&store, 125);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let gate = entered.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::WarmingAdmitted {
                gate.store(true, Ordering::Release);
                assert!(wait(|| cancelled.load(Ordering::Acquire)));
            }
        }));
    let ticket = engine
        .warm_audio(captured, Target::Sequence, AudioSample(0))
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    engine
        .play(1, snapshot(&store, 126), AudioSample(1600), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 0);
    assert_eq!(
        engine.audio_warming(ticket).unwrap().status,
        WarmStatus::Interrupted
    );
    assert_eq!(device.render(256, 0, 0).0.first_sample, Some(1600));
}

#[test]
fn waveform_request_preempts_idle_audio_without_opening_output() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("waveform.deadpan"), &hold(30)).unwrap();
    let captured = snapshot(&store, 127);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let gate = entered.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::WarmingAdmitted {
                gate.store(true, Ordering::Release);
                assert!(wait(|| cancelled.load(Ordering::Acquire)));
            }
        }));
    let warm = engine
        .warm_audio(captured.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    let waveform = engine
        .request_waveform(WaveformRequest {
            snapshot: captured,
            owner: node("pause"),
        })
        .unwrap();
    assert!(wait(|| engine.poll_waveform().is_some_and(|u| u.ticket
        == waveform
        && u.status == WaveformStatus::Complete)));
    assert_eq!(
        engine.audio_warming(warm).unwrap().status,
        WarmStatus::Interrupted
    );
    assert!(devices.lock().unwrap().is_empty());
    assert!(engine.poll().is_none());
}

#[test]
fn stop_cancels_a_promoted_warm_read_before_output_activation() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("stop.deadpan"), &hold(30)).unwrap();
    let captured = snapshot(&store, 128);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let (gate, finished) = (entered.clone(), stopped.clone());
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::WarmingBatchPrepared {
                gate.store(true, Ordering::Release);
                assert!(wait(|| cancelled.load(Ordering::Acquire)));
                finished.store(true, Ordering::Release);
            }
        }));
    let warm = engine
        .warm_audio(captured.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    engine.play(1, captured, AudioSample(0), 0.25).unwrap();
    engine.stop_handle().stop();
    assert!(wait(|| stopped.load(Ordering::Acquire)));
    assert_eq!(
        engine.audio_warming(warm).unwrap().status,
        WarmStatus::Interrupted
    );
    assert!(
        devices
            .lock()
            .unwrap()
            .iter()
            .all(|d| !d.started.load(Ordering::Acquire))
    );
    assert_eq!(engine.diagnostics().generations, 0);
}

#[test]
fn idle_horizon_prepares_a_future_preserve_and_seek_reuses_its_canonical_pcm() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("future.deadpan"), &empty()).unwrap();
    register(&mut store);
    let captured = snapshot(&store, 129);
    let mut wire: serde_json::Value =
        serde_json::from_str(&captured.document.to_json().unwrap()).unwrap();
    wire["root"] = json!("output");
    wire["revision_id"] = json!("future-preserve");
    wire["nodes"]["output"] = json!(BeatNode::sequence(
        "Output",
        vec![node("pause"), node("stretch")]
    ));
    let silent: serde_json::Value = serde_json::from_str(&hold(150).to_json().unwrap()).unwrap();
    wire["nodes"]["pause"] = silent["nodes"]["pause"].clone();
    wire["nodes"]["stretch"] = json!(BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "Future Preserve".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: node("root"),
            duration: FrameDuration::new(30).unwrap(),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(15)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    });
    let edited = Arc::new(Snapshot::committed(
        captured.session,
        Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap()),
        captured.sources.as_ref().clone(),
        captured.originals.clone(),
    ));
    let start = AudioSample(5 * 48_000 + 160);
    let mut sources = Sources::new(edited.clone());
    let mut canonical = LimitedAudio::new(Arc::new(RenderPlan::compile(&edited.document).unwrap()));
    let expected = canonical
        .read(
            &mut sources,
            start,
            256,
            Duration::from_secs(60),
            &cancelled(),
        )
        .unwrap();
    assert!(expected.samples.iter().flatten().any(|v| v.abs() > 0.00001));
    let (engine, devices) = engine(&permit);
    let admits = Arc::new(AtomicU64::new(0));
    let counted = admits.clone();
    *engine.shared.preparation_observer.lock().unwrap() = Some(Arc::new(move |event, _| {
        if event == PreparationEvent::PlaybackAdmitted {
            counted.fetch_add(1, Ordering::Relaxed);
        }
    }));
    let warm = engine
        .warm_audio(edited.clone(), Target::Sequence, AudioSample(0))
        .unwrap();
    let complete = warmed(&engine, warm);
    assert_eq!(complete.status, WarmStatus::Complete, "{complete:?}");
    assert_eq!(complete.prepared_until, AudioSample(6 * 48_000));
    assert!(devices.lock().unwrap().is_empty());
    engine.play(1, edited, start, 0.25).unwrap();
    let device = playing_device(&engine, &devices, 0);
    assert_eq!(
        admits.load(Ordering::Acquire),
        0,
        "future seek must retain the prepared stage owner"
    );
    let (report, actual) = device.render(256, 0, 0);
    assert_eq!(report.first_sample, Some(start.0));
    assert_eq!(
        actual,
        expected
            .samples
            .iter()
            .flat_map(|frame| frame.map(|v| v * 0.25))
            .collect::<Vec<_>>()
    );
}
