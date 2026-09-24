use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use deadpan_audio::{AudioSourceProvider, StageAudio};
use deadpan_core::*;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_output::{Callback, DeviceReport, Feed, RenderStatus, channel};
use deadpan_plan::RenderPlan;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use serde_json::json;

use crate::controller::Device;
use crate::sources::Sources;
use crate::*;

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn cancelled() -> AtomicBool {
    AtomicBool::new(false)
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()
        .unwrap()
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("playback").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}
fn hold(frames: i64) -> ProjectDocument {
    let mut wire: serde_json::Value = serde_json::from_str(&empty().to_json().unwrap()).unwrap();
    wire["nodes"]["root"] = json!(BeatNode::sequence("Sequence", vec![node("pause")]));
    wire["nodes"]["pause"] = json!(BeatNode {
        label: "Pause".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Hold {
            recipe: HoldRecipe {
                duration: FrameDuration::new(frames).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }
        },
    });
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn snapshot(store: &ProjectStore, session: u64) -> Arc<Snapshot> {
    let document = Arc::new(store.snapshot().unwrap());
    let sources = document
        .assets()
        .keys()
        .map(|asset| {
            let receipt = Arc::new(
                store
                    .registered_source(document.revision_id(), asset)
                    .unwrap(),
            );
            let original = store
                .original_record(receipt.original().content())
                .unwrap()
                .unwrap();
            (asset.clone(), SourceEntry { receipt, original })
        })
        .collect();
    Arc::new(Snapshot {
        session,
        document,
        sources,
        originals: store.original_import_handle().unwrap(),
    })
}
fn register(store: &mut ProjectStore) {
    let original = store
        .retain_original(
            &fixture(),
            OriginalOwnership::Managed,
            limits(),
            &cancelled(),
        )
        .unwrap()
        .record;
    let mut input = store
        .snapshot_original(original.object().content(), limits(), &cancelled())
        .unwrap();
    let audio = AudioSession::open_verified(
        &mut input,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        1,
        AudioSessionLimits::default(),
        &cancelled(),
    )
    .unwrap();
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio)).unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: store.snapshot().unwrap().revision_id().clone(),
                new_revision: revision("registered"),
                original: original.object().content().clone(),
                new_asset_id: AssetId::new("original").unwrap(),
                label: "Original".into(),
                insertion: Some(SourceInsertionRequest {
                    parent: node("root"),
                    index: 0,
                    node: node("source"),
                    label: "Source".into(),
                    purpose: Default::default(),
                }),
            },
            &decoded,
            None,
            limits(),
            &cancelled(),
        )
        .unwrap();
}

struct Fake {
    callback: Mutex<Callback>,
    reports: Mutex<VecDeque<DeviceReport>>,
    started: AtomicBool,
    now: AtomicU64,
    route_failed: AtomicBool,
    dropped: AtomicU64,
}
impl Fake {
    fn render(
        &self,
        frames: usize,
        callback_ns: u64,
        playback_ns: u64,
    ) -> (deadpan_output::RenderReport, Vec<f32>) {
        assert!(self.started.load(Ordering::Acquire));
        let mut pcm = vec![1.0; frames * 2];
        let report = self.callback.lock().unwrap().render(&mut pcm);
        self.reports.lock().unwrap().push_back(DeviceReport {
            render: report,
            callback_ns,
            playback_ns,
            render_cost_ns: 1,
        });
        (report, pcm)
    }
}
struct FakeDevice {
    feed: Feed,
    shared: Arc<Fake>,
}
impl Device for FakeDevice {
    fn feed(&mut self) -> &mut Feed {
        &mut self.feed
    }
    fn start(&mut self) -> Result<(), String> {
        self.shared.started.store(true, Ordering::Release);
        Ok(())
    }
    fn pause(&mut self) -> Result<(), String> {
        self.feed.pause().map_err(|e| e.to_string())?;
        self.shared.started.store(false, Ordering::Release);
        Ok(())
    }
    fn check_route(&self) -> Result<(), String> {
        if self.shared.route_failed.load(Ordering::Acquire) {
            Err("test route changed".into())
        } else {
            Ok(())
        }
    }
    fn pop_report(&mut self) -> Option<DeviceReport> {
        self.shared.reports.lock().unwrap().pop_front()
    }
    fn now_ns(&self) -> Option<u64> {
        Some(self.shared.now.load(Ordering::Acquire))
    }
    fn dropped_reports(&self) -> u64 {
        self.shared.dropped.load(Ordering::Acquire)
    }
    fn error_flags(&self) -> u64 {
        0
    }
}
fn engine() -> (Engine, Arc<Mutex<Vec<Arc<Fake>>>>) {
    let devices = Arc::new(Mutex::new(Vec::new()));
    let registry = devices.clone();
    let engine = Engine::with_factory(
        Arc::new(|| {}),
        Box::new(move || {
            let (feed, callback) = channel().unwrap();
            let shared = Arc::new(Fake {
                callback: Mutex::new(callback),
                reports: Mutex::new(VecDeque::new()),
                started: AtomicBool::new(false),
                now: AtomicU64::new(0),
                route_failed: AtomicBool::new(false),
                dropped: AtomicU64::new(0),
            });
            registry.lock().unwrap().push(shared.clone());
            Ok(Box::new(FakeDevice { feed, shared }))
        }),
    )
    .unwrap();
    (engine, devices)
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for playback worker"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn playing_device(devices: &Mutex<Vec<Arc<Fake>>>, index: usize) -> Arc<Fake> {
    wait(|| {
        devices
            .lock()
            .unwrap()
            .get(index)
            .is_some_and(|device| device.started.load(Ordering::Acquire))
    });
    devices.lock().unwrap()[index].clone()
}
fn update(engine: &Engine, phase: Phase) -> Update {
    let mut found = None;
    wait(|| {
        if let Some(value) = engine.poll()
            && value.phase == phase
        {
            found = Some(value);
        }
        found.is_some()
    });
    found.unwrap()
}

#[test]
fn canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock() {
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 12);
    let mut expected_sources = Sources::new(snapshot.clone());
    let mut canonical = StageAudio::new(Arc::new(RenderPlan::compile(&snapshot.document).unwrap()));
    let expected = canonical
        .read_edge_faded(
            &mut expected_sources,
            AudioSample(160),
            256,
            Duration::from_secs(10),
            &cancelled(),
        )
        .unwrap();
    let (engine, devices) = engine();
    engine
        .play(21, snapshot.clone(), AudioSample(160), 0.25)
        .unwrap();
    let device = playing_device(&devices, 0);
    store
        .commit(&CommandRequest {
            project_id: snapshot.document.project_id().clone(),
            expected_revision: snapshot.document.revision_id().clone(),
            new_revision: revision("writer-progress"),
            command: Command::Rename {
                node: node("source"),
                label: "Writer remains available".into(),
            },
        })
        .unwrap();
    assert_ne!(
        store.snapshot().unwrap().revision_id(),
        snapshot.document.revision_id()
    );
    let before_report = engine.poll().unwrap();
    assert_eq!(before_report.phase, Phase::Preparing);
    assert_eq!(
        before_report.sample, None,
        "producer prefill is not delivered audio"
    );
    let (report, pcm) = device.render(256, 0, 10_000_000);
    assert_eq!(report.first_sample, Some(160));
    assert_eq!(
        pcm,
        expected
            .samples
            .iter()
            .flat_map(|frame| frame.iter().map(|v| v * 0.25))
            .collect::<Vec<_>>()
    );
    device
        .now
        .store(10_000_000 + 10 * 1_000_000_000 / 48_000, Ordering::Release);
    let value = update(&engine, Phase::Playing);
    assert_eq!(value.ticket, 21);
    assert_eq!(value.session, 12);
    assert_eq!(value.revision_id, *snapshot.document.revision_id());
    assert_eq!(value.generation, Some(report.generation));
    assert_eq!(
        value.sample,
        Some(AudioSample(169)),
        "integer nanosecond timestamp remains below sample ten"
    );
    engine.stop_handle().stop();
    let mut after_stop = [1.0; 512];
    let stopped = device.callback.lock().unwrap().render(&mut after_stop);
    assert_eq!(stopped.rendered_frames, 0);
    assert!(after_stop.iter().all(|sample| *sample == 0.0));
    assert_eq!(update(&engine, Phase::Stopped).ticket, 21);
}

#[test]
fn short_eos_waits_for_scheduled_prefix_and_seek_replaces_generation() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(1)).unwrap();
    let snapshot = snapshot(&store, 1);
    let (engine, devices) = engine();
    engine
        .play(1, snapshot.clone(), AudioSample(1590), 0.1)
        .unwrap();
    let first = playing_device(&devices, 0);
    let (report, _) = first.render(256, 0, 10_000_000);
    assert_eq!(report.rendered_frames, 10);
    assert_eq!(report.status, RenderStatus::Ended);
    first.now.store(10_000_000, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Playing).sample,
        Some(AudioSample(1590))
    );
    first.now.store(10_208_334, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Ended).sample,
        Some(AudioSample(1600))
    );
    engine.play(2, snapshot, AudioSample(1500), 0.1).unwrap();
    let second = playing_device(&devices, 1);
    let (next, _) = second.render(256, 0, 10_000_000);
    assert_ne!(next.generation, report.generation);
    assert_eq!(next.first_sample, Some(1500));
    second.now.store(10_000_000, Ordering::Release);
    assert_eq!(update(&engine, Phase::Playing).ticket, 2);
}

#[test]
fn source_provider_rejects_foreign_contracts_and_revoked_cold_handles() {
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 2);
    let asset = AssetId::new("original").unwrap();
    let mut sources = Sources::new(snapshot.clone());
    assert!(
        sources
            .source(
                snapshot.document.project_id(),
                &revision("other"),
                &asset,
                &cancelled()
            )
            .is_err()
    );
    assert!(
        sources
            .source(
                &ProjectId::new("other").unwrap(),
                snapshot.document.revision_id(),
                &asset,
                &cancelled()
            )
            .is_err()
    );
    let mut wrong_asset = snapshot.document.assets()[&asset].clone();
    wrong_asset.label = "changed contract".into();
    assert!(
        sources
            .source_for_context(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &asset,
                &wrong_asset,
                &cancelled()
            )
            .is_err()
    );
    sources
        .source(
            snapshot.document.project_id(),
            snapshot.document.revision_id(),
            &asset,
            &cancelled(),
        )
        .unwrap();
    drop(store);
    // Already admitted private bytes remain valid; closing revokes new IO.
    sources
        .source(
            snapshot.document.project_id(),
            snapshot.document.revision_id(),
            &asset,
            &cancelled(),
        )
        .unwrap();
    assert!(
        Sources::new(snapshot.clone())
            .source(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &asset,
                &cancelled()
            )
            .is_err()
    );
    let stopped = AtomicBool::new(true);
    assert!(
        sources
            .source(
                snapshot.document.project_id(),
                snapshot.document.revision_id(),
                &asset,
                &stopped
            )
            .is_err()
    );
}

#[test]
fn stop_restart_and_failure_never_publish_an_old_session() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(60)).unwrap();
    let (engine, devices) = engine();
    engine
        .play(10, snapshot(&store, 10), AudioSample(0), 0.1)
        .unwrap();
    let first = playing_device(&devices, 0);
    engine
        .play(11, snapshot(&store, 11), AudioSample(100), 0.1)
        .unwrap();
    let second = playing_device(&devices, 1);
    let mut old_pcm = [1.0; 512];
    assert_eq!(
        first
            .callback
            .lock()
            .unwrap()
            .render(&mut old_pcm)
            .rendered_frames,
        0
    );
    assert!(old_pcm.iter().all(|value| *value == 0.0));
    second.render(256, 0, 10_000_000);
    second.now.store(10_000_000, Ordering::Release);
    let current = update(&engine, Phase::Playing);
    assert_eq!((current.ticket, current.session), (11, 11));
    second.dropped.store(1, Ordering::Release);
    let failed = update(&engine, Phase::Failed);
    assert_eq!((failed.ticket, failed.session), (11, 11));
    assert!(failed.error.unwrap().contains("reports were lost"));
    assert!(!second.started.load(Ordering::Acquire));
    engine.shutdown();
    assert_eq!(
        engine.play(12, snapshot(&store, 12), AudioSample(0), 0.1),
        Err(RequestError::Shutdown)
    );
}

#[test]
fn invalid_gain_and_outside_start_are_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(1)).unwrap();
    let (engine, devices) = engine();
    let snapshot = snapshot(&store, 1);
    for gain in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert_eq!(
            engine.play(1, snapshot.clone(), AudioSample(0), gain),
            Err(RequestError::InvalidGain)
        );
    }
    assert_eq!(
        engine.play(1, snapshot.clone(), AudioSample(-1), 0.1),
        Err(RequestError::InvalidStart)
    );
    assert!(devices.lock().unwrap().is_empty());
    engine.play(2, snapshot, AudioSample(1601), 0.1).unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("past the sequence end")
    );
}

#[test]
fn seek_reuses_private_pcm_but_a_new_session_must_reopen_sources() {
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 4);
    let (engine, devices) = engine();
    engine
        .play(1, snapshot.clone(), AudioSample(0), 0.1)
        .unwrap();
    playing_device(&devices, 0);
    engine.stop();
    update(&engine, Phase::Stopped);
    drop(store);
    engine
        .play(2, snapshot.clone(), AudioSample(100), 0.1)
        .unwrap();
    let reused = playing_device(&devices, 1);
    reused.render(256, 0, 10_000_000);
    reused.now.store(10_000_000, Ordering::Release);
    assert_eq!(update(&engine, Phase::Playing).ticket, 2);
    let next_session = Arc::new(Snapshot {
        session: 5,
        document: snapshot.document.clone(),
        sources: snapshot.sources.clone(),
        originals: snapshot.originals.clone(),
    });
    engine.play(3, next_session, AudioSample(0), 0.1).unwrap();
    let failed = update(&engine, Phase::Failed);
    assert_eq!(failed.ticket, 3);
    assert_eq!(failed.session, 5);
    assert!(failed.error.unwrap().contains("closed"));
}

#[test]
fn full_prefill_can_queue_eos_before_the_first_maximum_callback() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(6)).unwrap();
    let (engine, devices) = engine();
    engine
        .play(1, snapshot(&store, 1), AudioSample(9600 - 8192), 0.1)
        .unwrap();
    let device = playing_device(&devices, 0);
    let (report, _) = device.render(8192, 0, 1_000_000);
    assert_eq!(report.status, RenderStatus::Playing);
    assert_eq!(report.rendered_frames, 8192);
    let (eos, _) = device.render(256, 170_666_667, 171_666_667);
    assert_eq!(eos.status, RenderStatus::Ended);
    assert_eq!(eos.rendered_frames, 0);
    device.now.store(1_000_000 + 170_666_667, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Ended).sample,
        Some(AudioSample(9600))
    );
}

#[test]
fn starvation_keeps_its_submitted_prefix_clock_then_fails_without_resume() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(60)).unwrap();
    let (engine, devices) = engine();
    engine
        .play(1, snapshot(&store, 1), AudioSample(0), 0.1)
        .unwrap();
    let device = playing_device(&devices, 0);
    let (report, _) = device.render(256, 0, 10_000_000);
    device.now.store(10_000_000, Ordering::Release);
    update(&engine, Phase::Playing);
    // The fake host reports a submitted 16-frame prefix followed by starvation.
    // Queue-level starvation itself is covered in deadpan-output; this proves
    // the controller waits for the delivery deadline, not report receipt.
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
    assert_eq!(
        update(&engine, Phase::Playing).sample,
        Some(AudioSample(256))
    );
    device.now.store(15_666_667, Ordering::Release);
    let failed = update(&engine, Phase::Failed);
    assert_eq!(failed.sample, Some(AudioSample(272)));
    assert!(failed.error.unwrap().contains("Starved"));
    assert!(!device.started.load(Ordering::Acquire));
    thread::sleep(Duration::from_millis(20));
    assert_eq!(engine.poll(), None, "starvation must not silently resume");
}

#[test]
fn route_change_and_backwards_clock_stop_output() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(60)).unwrap();
    let (engine, devices) = engine();
    let snapshot = snapshot(&store, 1);
    engine
        .play(1, snapshot.clone(), AudioSample(0), 0.1)
        .unwrap();
    let first = playing_device(&devices, 0);
    first.route_failed.store(true, Ordering::Release);
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("route changed")
    );
    engine.play(2, snapshot, AudioSample(0), 0.1).unwrap();
    let second = playing_device(&devices, 1);
    second.render(256, 0, 10_000_000);
    second.now.store(10_000_000, Ordering::Release);
    update(&engine, Phase::Playing);
    second.now.store(9_000_000, Ordering::Release);
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("backwards")
    );
}

#[test]
fn canonical_playback_consumes_pause_bindings_and_a_real_preserve_stage() {
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let before = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("pause"),
            command: Command::InsertTime {
                at: ProjectFrame(1),
                hold: HoldRecipe {
                    duration: FrameDuration::new(1).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
                id: node("pause"),
                identities: SplitIdentities {
                    nodes: (0..before.nodes().len() + 4)
                        .map(|n| node(&format!("split-{n}")))
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: revision("pause"),
                    ordinal: 0,
                },
            },
        })
        .unwrap();
    let captured = snapshot(&store, 1);
    assert!(!captured.document.audio_bindings().is_empty());
    let original_frames = captured.document.duration().unwrap().frames();
    // Build an immutable core fixture with a new outer Preserve. The host's
    // actual retained receipts stay unchanged; this is not a persisted edit.
    let mut wire: serde_json::Value =
        serde_json::from_str(&captured.document.to_json().unwrap()).unwrap();
    wire["root"] = json!("output");
    wire["revision_id"] = json!("preserve-fixture");
    wire["nodes"]["output"] = json!(BeatNode::sequence("Output", vec![node("stretch")]));
    wire["nodes"]["stretch"] = json!(BeatNode {
        label: "Preserve".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: node("root"),
            duration: FrameDuration::new(original_frames * 2).unwrap(),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(original_frames)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    });
    let edited = Arc::new(Snapshot {
        session: captured.session,
        document: Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap()),
        sources: captured.sources.clone(),
        originals: captured.originals.clone(),
    });
    let mut sources = Sources::new(edited.clone());
    let mut canonical = StageAudio::new(Arc::new(RenderPlan::compile(&edited.document).unwrap()));
    let mut expected = Vec::new();
    for start in (0..2048).step_by(256) {
        let block = canonical
            .read_edge_faded(
                &mut sources,
                AudioSample(start),
                256,
                Duration::from_secs(10),
                &cancelled(),
            )
            .unwrap();
        expected.extend(
            block
                .samples
                .into_iter()
                .flat_map(|frame| frame.map(|v| v * 0.1)),
        );
    }
    assert!(expected.iter().any(|value| value.abs() > 0.00001));
    let (engine, devices) = engine();
    engine.play(1, edited, AudioSample(0), 0.1).unwrap();
    let device = playing_device(&devices, 0);
    let (_, actual) = device.render(2048, 0, 10_000_000);
    assert_eq!(actual, expected);
}

#[test]
fn worker_panic_is_reported_and_disables_further_requests() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(1)).unwrap();
    let engine = Engine::with_factory(
        Arc::new(|| {}),
        Box::new(|| panic!("injected host failure")),
    )
    .unwrap();
    let snapshot = snapshot(&store, 1);
    engine
        .play(1, snapshot.clone(), AudioSample(0), 0.1)
        .unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("terminated unexpectedly")
    );
    assert_eq!(
        engine.play(2, snapshot, AudioSample(0), 0.1),
        Err(RequestError::Shutdown)
    );
}
