use super::*;
use crate::preparation::PreparationEvent;

// A scope always releases the controller, including on an assertion failure.
pub(super) struct Park<'a>(&'a Engine);
impl<'a> Park<'a> {
    pub(super) fn new(engine: &'a Engine) -> Self {
        engine
            .shared
            .controller_hooks
            .stall
            .store(true, Ordering::Release);
        let parked = Self(engine);
        assert!(wait(|| engine
            .shared
            .controller_hooks
            .stalled
            .load(Ordering::Acquire)));
        parked
    }
}
impl Drop for Park<'_> {
    fn drop(&mut self) {
        self.0
            .shared
            .controller_hooks
            .stall
            .store(false, Ordering::Release);
    }
}

#[test]
fn full_lookahead_waits_for_space_and_stop_drops_every_queued_frame() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("bounded.deadpan"), &hold(1800)).unwrap();
    let captured = snapshot(&store, 130);
    let (engine, devices) = engine(&permit);
    engine
        .play(1, captured.clone(), AudioSample(0), 0.25)
        .unwrap();
    let first = playing_device(&engine, &devices, 0);
    let parked = Park::new(&engine);
    assert!(wait(|| engine.shared.lock().reply.is_full()));
    let frames = engine.shared.lock().reply.frames();
    assert!(frames > 29 * 48_000 && frames <= 30 * 48_000);
    assert_eq!(
        deadpan_diagnostics::QUEUES
            .playback_prefill_frames
            .level()
            .current,
        8192
    );
    engine.stop();
    assert_eq!(engine.shared.lock().reply.frames(), 0);
    let (stopped, pcm) = first.render(256, 0, 0);
    assert_eq!(stopped.rendered_frames, 0);
    assert!(pcm.iter().all(|v| *v == 0.0));
    drop(parked);
    engine.play(2, captured, AudioSample(1600), 0.25).unwrap();
    let second = playing_device(&engine, &devices, 1);
    assert_eq!(second.render(256, 0, 0).0.first_sample, Some(1600));
}

#[test]
fn a_future_preserve_finishes_before_its_audio_is_heard_and_delivery_is_canonical() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("future.deadpan"), &empty()).unwrap();
    register(&mut store);
    let captured = snapshot(&store, 131);
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
    let plan = Arc::new(RenderPlan::compile(&edited.document).unwrap());
    let end = plan.audio_duration().unwrap().0;
    let mut canonical = LimitedAudio::new(plan);
    let mut sources = Sources::new(edited.clone());
    let mut expected = Vec::new();
    for start in (0..end).step_by(8192) {
        let block = canonical
            .read(
                &mut sources,
                AudioSample(start),
                (end - start).min(8192) as u32,
                Duration::from_secs(60),
                &cancelled(),
            )
            .unwrap();
        expected.extend(
            block
                .samples
                .into_iter()
                .flat_map(|frame| frame.map(|v| v * 0.25)),
        );
    }
    assert!(expected.iter().any(|v| v.abs() > 0.00001));
    let (engine, devices) = engine(&permit);
    let batches = Arc::new(AtomicU64::new(0));
    let observed = batches.clone();
    *engine.shared.preparation_observer.lock().unwrap() = Some(Arc::new(move |event, _| {
        if event == PreparationEvent::PlaybackBatchPrepared {
            observed.fetch_add(1, Ordering::Release);
        }
    }));
    // Park the controller as it starts the device, before it moves more than
    // the prefill out of the prepared FIFO; parking only after observing the
    // start lets it fill the four-second device queue first.
    engine
        .shared
        .controller_hooks
        .stall_at_activation
        .store(true, Ordering::Release);
    engine.play(1, edited, AudioSample(0), 0.25).unwrap();
    let device = playing_device(&engine, &devices, 0);
    let parked = Park::new(&engine);
    assert!(
        wait(|| batches.load(Ordering::Acquire) >= (end as u64).div_ceil(8192)),
        "the future Preserve must finish while the heard clock is still at its initial boundary"
    );
    assert!(engine.shared.lock().reply.frames() > 4 * 48_000);
    assert_eq!(device.now.load(Ordering::Acquire), 0);
    // Resume the control worker with a current delivery report before its
    // watchdog examines the deliberately parked fake clock.
    let (_, first) = device.render(256, 0, 0);
    device
        .now
        .store(128 * 1_000_000_000 / 48_000, Ordering::Release);
    drop(parked);
    update(&engine, Phase::Playing);
    let submitted = || {
        engine
            .shared
            .controller_hooks
            .submitted_frames
            .load(Ordering::Acquire)
    };
    let mut actual = first;
    let mut frames = 256_i64;
    while frames < end {
        let count = (end - frames).min(256) as usize;
        // These callbacks run several times faster than a device's playback
        // rate, starting from only the prefill. Give the controller, which
        // each report wakes, a bounded moment to submit this callback's frames,
        // well inside its one-second stopped-clock watchdog.
        let deadline = Instant::now() + Duration::from_millis(250);
        while submitted() < (frames as u64 + count as u64) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        let now = frames as u64 * 1_000_000_000 / 48_000;
        assert!(
            device.started.load(Ordering::Acquire),
            "stopped at {frames}: {:?}",
            engine.poll()
        );
        let (report, pcm) = device.render(count, now, now);
        assert_eq!(
            report.rendered_frames, count,
            "unexpected output status: {report:?}"
        );
        actual.extend(pcm);
        // A callback describes an interval, not producer progress. Keep the
        // fake heard clock inside the submitted content until its next report.
        device.now.store(
            (frames as u64 + count as u64 / 2) * 1_000_000_000 / 48_000,
            Ordering::Release,
        );
        frames += count as i64;
        thread::sleep(Duration::from_millis(1));
    }
    let end_ns = end as u64 * 1_000_000_000 / 48_000;
    let (terminal, _) = device.render(256, end_ns, end_ns);
    assert_eq!(terminal.rendered_frames, 0);
    device.now.store(end_ns + 1, Ordering::Release);
    assert_eq!(actual, expected);
    assert_eq!(update(&engine, Phase::Ended).sample, Some(AudioSample(end)));
    assert_eq!(engine.diagnostics().starved, 0);
    assert_eq!(engine.shared.lock().reply.frames(), 0);
}
