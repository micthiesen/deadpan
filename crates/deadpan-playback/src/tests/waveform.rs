//! The real engine's idle analysis lane, including scheduled output prefixes.

use super::*;
use crate::preparation::PreparationEvent;
use deadpan_audio::StageAudio;
use deadpan_plan::{AudioDefinitionSelector, SignalSample};
use std::sync::Condvar;

fn request(engine: &Engine, snapshot: Arc<Snapshot>, owner: &str) -> WaveformTicket {
    engine
        .request_waveform(WaveformRequest {
            snapshot,
            owner: node(owner),
        })
        .unwrap()
}

fn terminal(engine: &Engine, ticket: WaveformTicket) -> WaveformUpdate {
    let mut result = None;
    assert!(
        wait(|| {
            if let Some(update) = engine.poll_waveform() {
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
        }),
        "waveform did not reach a terminal outcome"
    );
    result.unwrap()
}

fn heard(engine: &Engine, sample: AudioSample) {
    assert!(
        wait(|| {
            let Some(update) = engine.poll() else {
                return false;
            };
            assert!(
                !matches!(update.phase, Phase::Failed | Phase::Ended),
                "terminal output before expected prefix sample: {update:?}"
            );
            update.phase == Phase::Playing && update.sample == Some(sample)
        }),
        "delivery clock did not reach expected prefix sample {sample:?}"
    );
}

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    wake: Condvar,
    entered: AtomicBool,
    cancelled: AtomicBool,
}

impl Gate {
    fn block(&self, cancelled: &AtomicBool) {
        self.entered.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut open = self.open.lock().unwrap();
        while !*open {
            self.cancelled
                .store(cancelled.load(Ordering::Acquire), Ordering::Release);
            assert!(
                Instant::now() < deadline,
                "test did not release preparation gate"
            );
            open = self
                .wake
                .wait_timeout(open, Duration::from_millis(2))
                .unwrap()
                .0;
        }
        self.cancelled
            .store(cancelled.load(Ordering::Acquire), Ordering::Release);
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

fn block_first_analysis(engine: &Engine) -> (Arc<Gate>, Arc<Mutex<Vec<PreparationEvent>>>) {
    let gate = Arc::new(Gate::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let retained_gate = gate.clone();
    let retained_events = events.clone();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            retained_events.lock().unwrap().push(event);
            if event == PreparationEvent::WaveformAdmitted
                && !retained_gate.entered.load(Ordering::Acquire)
            {
                retained_gate.block(cancelled);
            }
        }));
    (gate, events)
}

#[test]
fn silent_definition_measures_without_device_or_playback_updates() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("waveform.deadpan"), &hold(1)).unwrap();
    let snapshot = snapshot(&store, 101);
    let (engine, devices) = engine(&permit);
    let ticket = request(&engine, snapshot.clone(), "pause");
    let update = terminal(&engine, ticket);
    assert_eq!(update.status, WaveformStatus::Complete);
    assert_eq!(update.session, 101);
    assert_eq!(update.project_id, *snapshot.document.project_id());
    assert_eq!(update.revision_id, *snapshot.document.revision_id());
    assert_eq!(update.owner, node("pause"));
    let waveform = update.waveform.unwrap();
    assert_eq!(waveform.descriptor().total_samples, SignalSample(1600));
    assert_eq!(waveform.measured_end(), SignalSample(1600));
    assert_eq!(update.examined_samples, 1600);
    assert!(
        waveform
            .level(0)
            .unwrap()
            .iter()
            .all(|bin| bin.minimum() == [0.0; 2] && bin.maximum() == [0.0; 2])
    );
    assert!(devices.lock().unwrap().is_empty());
    assert_eq!(engine.poll(), None);
    let memory = engine.shared.lock().waveform.memory.clone();
    assert!(memory.resident_bytes() > 0);
    drop(engine);
    assert!(
        memory.resident_bytes() > 0,
        "UI-held peaks retain their aggregate allocation charge"
    );
    drop(waveform);
    assert_eq!(memory.resident_bytes(), 0);
}

#[test]
fn replacement_coalesces_pending_work_and_drops_stale_success_and_failure() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("replace-waveform.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 102);
    let (engine, devices) = engine(&permit);
    let (gate, events) = block_first_analysis(&engine);
    let first = request(&engine, base.clone(), "pause");
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    let middle = request(&engine, base.clone(), "missing-owner");
    let latest = request(&engine, base, "root");
    assert!(first.value() < middle.value() && middle.value() < latest.value());
    // Cancellation of a stale UI ticket cannot cancel its replacement.
    engine.cancel_waveform(first);
    gate.release();
    let update = terminal(&engine, latest);
    assert_eq!(update.status, WaveformStatus::Complete);
    assert_eq!(update.owner, node("root"));
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == PreparationEvent::WaveformAdmitted)
            .count(),
        2
    );
    assert!(devices.lock().unwrap().is_empty());
    assert_eq!(engine.poll(), None);
}

#[test]
fn audition_cancels_analysis_before_admitting_its_exclusive_cache_owner() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(
        &directory.path().join("priority-waveform.deadpan"),
        &hold(1),
    )
    .unwrap();
    let base = snapshot(&store, 103);
    let (engine, devices) = engine(&permit);
    let (gate, events) = block_first_analysis(&engine);
    let ticket = request(&engine, base.clone(), "pause");
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    engine.play(1, base, AudioSample(0), 0.1).unwrap();
    assert!(wait(|| gate.cancelled.load(Ordering::Acquire)));
    assert!(
        !events
            .lock()
            .unwrap()
            .contains(&PreparationEvent::PlaybackAdmitted)
    );
    let interrupted = terminal(&engine, ticket);
    assert_eq!(interrupted.status, WaveformStatus::Interrupted);
    gate.release();
    playing_device(&engine, &devices, 0);
    let events = events.lock().unwrap();
    let released = events
        .iter()
        .position(|event| *event == PreparationEvent::WaveformReleased)
        .unwrap();
    let playback = events
        .iter()
        .position(|event| *event == PreparationEvent::PlaybackAdmitted)
        .unwrap();
    assert!(released < playback);
}

#[test]
fn playback_stop_preserves_analysis_but_lifecycle_stop_revokes_it() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(
        &directory.path().join("lifecycle-waveform.deadpan"),
        &hold(1),
    )
    .unwrap();
    let base = snapshot(&store, 104);
    let (engine, _) = engine(&permit);
    let (gate, _) = block_first_analysis(&engine);
    let ticket = request(&engine, base.clone(), "pause");
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    engine.stop();
    assert!(!gate.cancelled.load(Ordering::Acquire));
    gate.release();
    assert_eq!(terminal(&engine, ticket).status, WaveformStatus::Complete);
    let (gate, _) = block_first_analysis(&engine);
    let ticket = request(&engine, base, "pause");
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    engine.stop_handle().stop();
    assert!(wait(|| gate.cancelled.load(Ordering::Acquire)));
    assert_eq!(
        terminal(&engine, ticket).status,
        WaveformStatus::Interrupted
    );
    gate.release();
}

#[test]
fn cancelling_the_current_ticket_suppresses_its_late_result() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("cancel-waveform.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 111);
    let (engine, _) = engine(&permit);
    let (gate, _) = block_first_analysis(&engine);
    let cancelled = request(&engine, base.clone(), "pause");
    assert!(wait(|| gate.entered.load(Ordering::Acquire)));
    engine.cancel_waveform(cancelled);
    assert!(engine.poll_waveform().is_none());
    assert!(wait(|| gate.cancelled.load(Ordering::Acquire)));
    let replacement = request(&engine, base, "root");
    gate.release();
    let update = terminal(&engine, replacement);
    assert_eq!(update.status, WaveformStatus::Complete);
    assert_eq!(update.owner, node("root"));
}

#[test]
fn active_output_and_ended_prefix_defer_analysis_until_device_teardown() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("prefix-waveform.deadpan"), &hold(1)).unwrap();
    let base = snapshot(&store, 105);
    let (engine, devices) = engine(&permit);
    engine
        .play(1, base.clone(), AudioSample(1590), 0.1)
        .unwrap();
    let device = playing_device(&engine, &devices, 0);
    let admitted = Arc::new(AtomicBool::new(false));
    let observe = admitted.clone();
    *engine.shared.preparation_observer.lock().unwrap() = Some(Arc::new(move |event, _| {
        if event == PreparationEvent::WaveformAdmitted {
            observe.store(true, Ordering::Release);
        }
    }));
    let ticket = request(&engine, base, "pause");
    assert_eq!(
        engine.poll_waveform().unwrap().status,
        WaveformStatus::Queued
    );
    let (report, _) = device.render(256, 0, 10_000_000);
    assert_eq!(report.status, RenderStatus::Ended);
    device.now.store(10_000_000, Ordering::Release);
    heard(&engine, AudioSample(1590));
    assert!(!admitted.load(Ordering::Acquire));
    assert!(engine.poll_waveform().is_none());
    device.now.store(10_208_334, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Ended).sample,
        Some(AudioSample(1600))
    );
    assert_eq!(terminal(&engine, ticket).status, WaveformStatus::Complete);
    assert!(!device.started.load(Ordering::Acquire));
    assert_eq!(devices.lock().unwrap().len(), 1);
}

#[test]
fn starved_prefix_defers_analysis_then_fails_playback_without_resuming() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(
        &directory.path().join("starved-waveform.deadpan"),
        &hold(60),
    )
    .unwrap();
    let base = snapshot(&store, 106);
    let (engine, devices) = engine(&permit);
    engine.play(1, base.clone(), AudioSample(0), 0.1).unwrap();
    let device = playing_device(&engine, &devices, 0);
    let (report, _) = device.render(256, 0, 10_000_000);
    device.now.store(10_000_000, Ordering::Release);
    heard(&engine, AudioSample(0));
    let ticket = request(&engine, base, "pause");
    assert_eq!(
        engine.poll_waveform().unwrap().status,
        WaveformStatus::Queued
    );
    device.reports.lock().unwrap().push_back(DeviceReport {
        render: deadpan_output::RenderReport {
            generation: report.generation,
            status: RenderStatus::Starved,
            first_sample: Some(256),
            rendered_frames: 16,
            silent_frames: 240,
            discarded_packets: 0,
        },
        callback_ns: 5_333_333,
        playback_ns: 15_333_333,
        render_cost_ns: 1,
    });
    device.now.store(15_333_333, Ordering::Release);
    heard(&engine, AudioSample(256));
    assert!(engine.poll_waveform().is_none());
    device.now.store(15_666_667, Ordering::Release);
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("Starved")
    );
    assert_eq!(terminal(&engine, ticket).status, WaveformStatus::Complete);
    assert_eq!(devices.lock().unwrap().len(), 1);
    assert!(!device.started.load(Ordering::Acquire));
}

#[test]
fn device_open_failure_releases_queued_analysis_but_worker_panic_revokes_it() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    for panic in [false, true] {
        let store = ProjectStore::create(
            &directory
                .path()
                .join(format!("open-waveform-{panic}.deadpan")),
            &hold(1),
        )
        .unwrap();
        let base = snapshot(&store, 110);
        let gate = Arc::new(Gate::default());
        let opening = gate.clone();
        let engine = Engine::with_factory(
            resources::repaint(&permit),
            Box::new(move || {
                opening.block(&AtomicBool::new(false));
                if panic {
                    std::panic::resume_unwind(Box::new("injected output factory panic"));
                }
                Err("injected device-open failure".into())
            }),
        )
        .unwrap();
        engine.play(1, base.clone(), AudioSample(0), 0.1).unwrap();
        assert!(wait(|| gate.entered.load(Ordering::Acquire)));
        let ticket = request(&engine, base.clone(), "pause");
        assert_eq!(
            engine.poll_waveform().unwrap().status,
            WaveformStatus::Queued
        );
        gate.release();
        assert_eq!(update(&engine, Phase::Failed).ticket, 1);
        let result = terminal(&engine, ticket);
        if panic {
            assert_eq!(result.status, WaveformStatus::Unavailable);
            assert!(result.error.unwrap().contains("terminated unexpectedly"));
            assert_eq!(
                engine.request_waveform(WaveformRequest {
                    snapshot: base,
                    owner: node("pause")
                }),
                Err(WaveformRequestError::Shutdown)
            );
        } else {
            assert_eq!(result.status, WaveformStatus::Complete);
        }
    }
}

#[test]
fn real_definition_peaks_match_canonical_pcm_and_each_request_readmits_evidence() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("real-waveform.deadpan"), &empty()).unwrap();
    register(&mut store);
    let base = snapshot(&store, 107);
    let (engine, devices) = engine(&permit);
    let cold_started = Instant::now();
    let ticket = request(&engine, base.clone(), "source");
    let update = terminal(&engine, ticket);
    assert_eq!(update.status, WaveformStatus::Complete);
    let cold_ms = cold_started.elapsed().as_secs_f64() * 1000.0;
    let waveform = update.waveform.unwrap();
    let profile = if cfg!(debug_assertions) {
        "debug-test"
    } else {
        "release-test"
    };
    eprintln!(
        "waveform qualification: profile={profile}, fixture=cfr-bframes.mp4, output=none, cold_request_to_terminal_ms={cold_ms:.3}, samples={}",
        waveform.measured_end().0
    );
    assert!(devices.lock().unwrap().is_empty());
    assert_eq!(engine.poll(), None);
    let plan = Arc::new(RenderPlan::compile(&base.document).unwrap());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: node("source"),
        })
        .unwrap();
    let mut audio = StageAudio::new(plan.clone());
    let mut sources = Sources::new(base.clone());
    for (index, bin) in waveform.level(0).unwrap().iter().enumerate() {
        let span = waveform.bin_samples(0, index).unwrap();
        let mut minimum = [f32::INFINITY; 2];
        let mut maximum = [f32::NEG_INFINITY; 2];
        let mut cursor = span.start.0;
        while cursor < span.end.0 {
            let count = u32::try_from((span.end.0 - cursor).min(173)).unwrap();
            let block = audio
                .read_definition(
                    &mut sources,
                    &definition,
                    SignalSample(cursor),
                    count,
                    Duration::from_secs(20),
                    &cancelled(),
                )
                .unwrap();
            for sample in block.samples {
                for channel in 0..2 {
                    minimum[channel] = minimum[channel].min(sample[channel]);
                    maximum[channel] = maximum[channel].max(sample[channel]);
                }
            }
            cursor += i64::from(count);
        }
        assert_eq!(bin.minimum(), minimum);
        assert_eq!(bin.maximum(), maximum);
    }
    drop(audio);
    drop(sources);
    // The completed analysis has released its source/DSP caches. This measures
    // real canonical cold preparation into the existing headless output queue,
    // not a physical device, acoustic latency, or in-flight decode cancellation.
    let prefill_started = Instant::now();
    engine.play(1, base.clone(), AudioSample(0), 0.1).unwrap();
    playing_device(&engine, &devices, 0);
    let prefill_ms = prefill_started.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "waveform qualification: profile={profile}, fixture=cfr-bframes.mp4, output=headless-fake-device, after_analysis_cold_request_to_prefill_ms={prefill_ms:.3}"
    );
    engine.stop();
    let missing = Arc::new(Snapshot::committed(
        base.session,
        base.document.clone(),
        Default::default(),
        base.originals.clone(),
    ));
    let failed = terminal(&engine, request(&engine, missing, "source"));
    assert_eq!(failed.status, WaveformStatus::Partial);
    assert_eq!(failed.waveform.unwrap().measured_end(), SignalSample(0));
    assert!(failed.error.unwrap().contains("receipt"));
    assert_eq!(
        devices.lock().unwrap().len(),
        1,
        "analysis cannot open another output device"
    );
    drop(store);
    let revoked = terminal(&engine, request(&engine, base, "source"));
    assert_eq!(revoked.status, WaveformStatus::Partial);
    assert_eq!(revoked.waveform.unwrap().measured_end(), SignalSample(0));
    assert!(revoked.error.is_some());
}

#[test]
fn proposed_snapshots_and_foreign_owners_fail_without_opening_output() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(
        &directory.path().join("waveform-admission.deadpan"),
        &hold(1),
    )
    .unwrap();
    let base = snapshot(&store, 108);
    let mut wire = serde_json::to_value(base.document.as_ref()).unwrap();
    wire["revision_id"] = json!("proposed-waveform");
    let document = Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap());
    let proposed = Arc::new(Snapshot::proposed(&base, document.clone(), 1, 1).unwrap());
    let (engine, devices) = engine(&permit);
    assert_eq!(
        engine.request_waveform(WaveformRequest {
            snapshot: proposed,
            owner: node("pause")
        }),
        Err(WaveformRequestError::Proposed)
    );
    let mut forged = Snapshot::proposed(&base, document, 1, 2).unwrap();
    forged.content = ContentIdentity::Committed;
    assert!(matches!(
        engine.request_waveform(WaveformRequest {
            snapshot: Arc::new(forged),
            owner: node("pause")
        }),
        Err(WaveformRequestError::InvalidAdmission(_))
    ));
    let failure = terminal(&engine, request(&engine, base, "missing-owner"));
    assert_eq!(failure.status, WaveformStatus::Unavailable);
    assert_eq!(failure.owner, node("missing-owner"));
    assert!(failure.waveform.is_none());
    assert!(failure.error.is_some());
    assert!(devices.lock().unwrap().is_empty());
}

#[path = "waveform/edit.rs"]
mod edit;
