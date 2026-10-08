//! Extension through normal allocation, supervision, real qualification and publication.

use super::*;
use crate::generation::conditioning::{ExtensionInputs, prepare_extension_scoped_with_provider};
use crate::generation::runtime::{plan_for_manifest, selected_provider_for_manifest};
use deadpan_core::{ExtensionDirection, FrameRate};
use deadpan_jobs::{ExtensionGenerationPlan, GenerationOptions};

fn manifest() -> deadpan_models::packs::PackManifest {
    deadpan_models::packs::approved_pack("ltx-2.3-q4-extension").unwrap()
}

fn target() -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: hold_id(),
        repeats: vec![],
    }
}

fn extension_plan(direction: ExtensionDirection, frames: i64) -> ExtensionGenerationPlan {
    let mode = match direction {
        ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
        ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
    };
    let GenerationPlan::Extension(plan) = plan_for_manifest(
        &manifest(),
        mode,
        FrameDuration::new(frames).unwrap(),
        FrameRate::new(30, 1).unwrap(),
    )
    .unwrap() else {
        panic!("Extension manifest must yield an Extension plan")
    };
    plan
}

fn project_with_context(
    directory: &Path,
    direction: ExtensionDirection,
    opposite: bool,
) -> ProjectStore {
    let root = NodeId::new("root").unwrap();
    let mut document = ProjectDocument::new(
        ProjectId::new("normal-extension").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 768,
            height: 320,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    let names = ["left", "pause", "right"].into_iter().filter(|name| {
        opposite
            || *name == "pause"
            || match direction {
                ExtensionDirection::FromLeft => *name == "left",
                ExtensionDirection::FromRight => *name == "right",
            }
    });
    for (index, name) in names.enumerate() {
        let node = NodeId::new(name).unwrap();
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("setup-{index}")).unwrap(),
            command: Command::Insert {
                parent: root.clone(),
                index,
                subtree: Subtree {
                    root: node.clone(),
                    nodes: BTreeMap::from([(
                        node,
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(30).unwrap(),
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
        };
        document = deadpan_core::apply(&document, &request)
            .unwrap()
            .forward
            .apply(&document)
            .unwrap();
    }
    ProjectStore::create(&directory.join("test.deadpan"), &document).unwrap()
}

fn captured(
    directory: &Path,
    store: &ProjectStore,
    direction: ExtensionDirection,
) -> ExtensionInputs {
    let plan = extension_plan(direction, 30);
    let SelectedGenerationProvider::Extension(selected) =
        selected_provider_for_manifest(&manifest(), &GenerationPlan::Extension(plan.clone()), 7)
            .unwrap()
    else {
        panic!("selected Extension provider")
    };
    prepare_extension_scoped_with_provider(
        &directory.join("test.deadpan"),
        &store.head_revision().unwrap(),
        &target(),
        &plan,
        &selected,
        &GenerationOptions::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn allocate_extension(
    directory: &Path,
    store: &mut ProjectStore,
    direction: ExtensionDirection,
) -> Allocated {
    let inputs = captured(directory, store, direction);
    let selected = selected_provider_for_manifest(
        &manifest(),
        &GenerationPlan::Extension(inputs.plan.clone()),
        7,
    )
    .unwrap();
    let expected_revision = store.head_revision().unwrap();
    allocate_scoped_with_provider(
        store,
        AllocateInput {
            hold: hold_id(),
            expected_revision,
            seed: 7,
            inputs: inputs.into(),
        },
        target(),
        selected.selection().clone(),
    )
    .unwrap()
}

fn selected(allocated: &Allocated) -> SelectedGenerationProvider {
    selected_provider_for_manifest(
        &manifest(),
        &allocated.inputs.plan(),
        allocated.provider().seed,
    )
    .unwrap()
}

#[test]
fn bridge_capture_uses_selected_plan_and_refuses_a_different_saved_clock() {
    let directory = tempfile::tempdir().unwrap();
    let store = project_with_context(directory.path(), ExtensionDirection::FromLeft, true);
    let default = crate::generation::development_capability();
    let capability = deadpan_jobs::BridgeCapability::new(
        true,
        FrameRate::new(12, 1).unwrap(),
        default.frame_counts(),
        default.dimensions(),
    );
    let supplied = BridgeGenerationPlan::new(
        FrameDuration::new(30).unwrap(),
        FrameRate::new(30, 1).unwrap(),
        &capability,
        crate::generation::native_dimensions(),
    )
    .unwrap();
    let capture = |plan: &BridgeGenerationPlan| {
        conditioning::prepare_bridge_scoped_with_plan(
            &directory.path().join("test.deadpan"),
            &store.head_revision().unwrap(),
            &target(),
            plan,
            &GenerationOptions::default(),
            &AtomicBool::new(false),
        )
    };
    let inputs = capture(&supplied).unwrap();
    assert_eq!(inputs.plan, supplied);
    let context: deadpan_models::BridgeContext = serde_json::from_slice(&inputs.manifest).unwrap();
    assert_eq!(context.plan(), &supplied);
    for (frames, rate) in [(29, 30), (30, 24)] {
        let mismatch = BridgeGenerationPlan::new(
            FrameDuration::new(frames).unwrap(),
            FrameRate::new(rate, 1).unwrap(),
            &capability,
            crate::generation::native_dimensions(),
        )
        .unwrap();
        assert!(
            capture(&mismatch)
                .unwrap_err()
                .contains("saved Hold duration or project rate")
        );
    }
}

#[test]
fn extension_allocation_retains_operation_origin_and_rejects_changed_variants() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let directory = tempfile::tempdir().unwrap();
        let mut store = project_with_context(directory.path(), direction, false);
        let before = store.snapshot().unwrap();
        let first = allocate_extension(directory.path(), &mut store, direction);
        assert_eq!(first.protocol(), ProtocolVersion::V3);
        assert!(matches!(
            &first.host_message,
            HostMessage::GenerateExtension {
                protocol: ProtocolVersion::V3,
                ..
            }
        ));
        assert_eq!(first.request.plan.as_ref(), Some(&first.inputs.plan()));
        assert_eq!(
            current_scoped_request(&store, &target())
                .unwrap()
                .unwrap()
                .request_id,
            first.request.request_id
        );
        assert!(
            current_scoped_bridge_request(&store, &target())
                .unwrap()
                .is_none()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(state(&store, &first), JobState::Queued);

        let mut changed = first.inputs().clone();
        let PreparedInputs::Extension(inputs) = &mut changed else {
            unreachable!()
        };
        inputs.plan = extension_plan(direction, 29);
        assert!(allocate_variant(&mut store, first.request.clone(), changed).is_err());
        finish(
            &mut store,
            &first,
            WorkerRun::without_workspace(RunResult::Cancelled, RunTimings::default()),
        )
        .unwrap();
        let second =
            allocate_variant(&mut store, first.request.clone(), first.inputs().clone()).unwrap();
        assert_eq!(second.ordinal(), 2);
        assert_eq!(second.provider().seed, 8);
        assert_eq!(
            second.request.origin_revision,
            first.request.origin_revision
        );
        assert_eq!(second.inputs.manifest(), first.inputs.manifest());
        assert_eq!(second.inputs.plan(), first.inputs.plan());
        assert_eq!(second.provider().pack_id, first.provider().pack_id);
    }
}

#[test]
fn extension_workspace_refuses_wrong_provider_missing_context_and_changed_signatures() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project_with_context(directory.path(), ExtensionDirection::FromLeft, true);
    let first = allocate_extension(directory.path(), &mut store, ExtensionDirection::FromLeft);
    let bridge = selected_provider_for_manifest(
        &deadpan_models::packs::approved_pack("ltx-2.3-q4-bridge").unwrap(),
        &GenerationPlan::Bridge(inputs().plan),
        7,
    )
    .unwrap();
    assert!(prepare_workspace(&first, None, bridge, &AtomicBool::new(false)).is_err());
    let mut wrong_provider = first.clone();
    let HostMessage::GenerateExtension { provider, .. } = &mut wrong_provider.host_message else {
        unreachable!()
    };
    provider.seed += 1;
    assert!(
        prepare_workspace(
            &wrong_provider,
            None,
            selected(&first),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    for defect in ["count", "picture", "signatures"] {
        let mut bad = first.clone();
        let PreparedInputs::Extension(inputs) = &mut bad.inputs else {
            unreachable!()
        };
        match defect {
            "count" => {
                inputs.context_pngs.pop();
            }
            "picture" => inputs.context_pngs[0][0] ^= 1,
            "signatures" => inputs.continuity_signatures[0] ^= 1,
            _ => unreachable!(),
        }
        assert!(
            prepare_workspace(&bad, None, selected(&first), &AtomicBool::new(false)).is_err(),
            "{defect}"
        );
    }
    let prepared =
        prepare_workspace(&first, None, selected(&first), &AtomicBool::new(false)).unwrap();
    let RetainedInputs::Extension { inputs, .. } = prepared.conditioning else {
        unreachable!()
    };
    assert_eq!(inputs.context_frames().len(), 9);
    assert!(inputs.opposite().is_some());
    assert!(inputs.signatures().object().byte_length() > 0);
}

fn declared(allocated: &Allocated) -> NativeCandidateManifest {
    let plan = allocated.inputs.plan();
    let bytes = b"declared fixture";
    let hash = conditioning::sha256(bytes).unwrap();
    NativeCandidateManifest {
        native: workspace_artifact("outputs/native.mp4", bytes, &hash).unwrap(),
        provenance: workspace_artifact("outputs/provenance.json", bytes, &hash).unwrap(),
        video: VideoSpec::new(
            FrameDuration::new(i64::from(native_frame_count(&plan))).unwrap(),
            plan.native_frame_rate(),
            plan.native_dimensions().width(),
            plan.native_dimensions().height(),
        )
        .unwrap(),
        provider: allocated.provider().clone(),
    }
}

#[test]
fn supervised_extension_cancel_and_host_refusals_are_terminal_before_ready() {
    for mode in ["cancel", "ack", "completion"] {
        let directory = tempfile::tempdir().unwrap();
        let mut store = project_with_context(directory.path(), ExtensionDirection::FromLeft, false);
        let first = allocate_extension(directory.path(), &mut store, ExtensionDirection::FromLeft);
        let before = store.snapshot().unwrap();
        let mut runtime = runtime(directory.path(), "/usr/bin/true");
        runtime.python = python();
        runtime.model_manifest = manifest();
        runtime.worker_script = directory.path().join("cancel-worker.py");
        std::fs::write(&runtime.worker_script, CANCEL_WORKER).unwrap();
        std::fs::write(directory.path().join("mode"), mode).unwrap();
        let mut bad = declared(&first);
        bad.video = VideoSpec::new(
            FrameDuration::new(32).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            768,
            320,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("bad-candidate.json"),
            serde_json::to_vec(&bad).unwrap(),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut cancellation_recorded = false;
        let mut completion_recorded = false;
        let run = run_worker(
            &first,
            &runtime,
            |progress| {
                if mode == "cancel" && progress == AttemptProgress::Stage(WorkerStage::Preflight) {
                    cancelled.store(true, Ordering::Release);
                }
            },
            |entry| {
                if mode == "ack" && matches!(&entry, AttemptRecord::Worker(_)) {
                    return Err("test writer refused stage acknowledgement".into());
                }
                cancellation_recorded |= entry == AttemptRecord::CancelRequested;
                completion_recorded |= matches!(&entry, AttemptRecord::Worker(message)
                    if matches!(message.as_ref(), WorkerMessage::CompletedExtension { .. }));
                record(&mut store, &first, &entry).map_err(|error| error.to_string())
            },
            &cancelled,
        );
        let log = run.worker_log.clone();
        let result = finish(&mut store, &first, run).unwrap();
        if mode != "cancel" {
            assert_eq!(result.state, JobState::Failed, "{log}");
            let Some(JobFailure::Host(failure)) = result.failure else {
                panic!("writer failure")
            };
            let (code, detail) = if mode == "ack" {
                (HostFailureCode::Io, "acknowledgement")
            } else {
                (
                    HostFailureCode::OutputValidationFailed,
                    "differs from the request plan",
                )
            };
            assert_eq!(failure.code, code, "{failure:?}; {log}");
            assert!(
                failure.detail.as_str().contains(detail),
                "{failure:?}; {log}"
            );
        } else {
            assert_eq!(result.state, JobState::Cancelled, "{log}");
            assert!(cancellation_recorded);
        }
        assert!(result.receipt.is_none());
        assert!(
            !completion_recorded,
            "a refused candidate cannot be acknowledged to the store"
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let cancellation_receipt = directory.path().join("received-cancel");
        if mode == "completion" {
            // Completed is delivered only after clean teardown, so this host
            // rejection cannot and need not cancel an already reaped worker.
            assert!(!cancellation_receipt.exists());
        } else {
            assert_eq!(std::fs::read_to_string(cancellation_receipt).unwrap(), "3");
        }
        let pid: i32 = std::fs::read_to_string(directory.path().join("worker.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).unwrap()),
            Err(rustix::io::Errno::SRCH)
        );
    }
}

fn python() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("python3"))
        .find(|path| path.is_file())
        .expect("Python 3")
        .canonicalize()
        .unwrap()
}

#[test]
fn extension_completion_requires_v3_operation_native_contract_and_selected_provider() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = project_with_context(directory.path(), ExtensionDirection::FromLeft, false);
    let first = allocate_extension(directory.path(), &mut store, ExtensionDirection::FromLeft);
    let candidate = declared(&first);
    let completed = |candidate: NativeCandidateManifest| WorkerMessage::CompletedExtension {
        protocol: ProtocolVersion::V3,
        identity: first.identity.clone(),
        candidate,
    };
    assert!(completion_matches(
        &first,
        &completed(candidate.clone()),
        &candidate
    ));
    let bridge = WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: first.identity.clone(),
        candidate: candidate.clone(),
    };
    assert!(!completion_matches(&first, &bridge, &candidate));
    let wrong_protocol = WorkerMessage::CompletedExtension {
        protocol: ProtocolVersion::V2,
        identity: first.identity.clone(),
        candidate: candidate.clone(),
    };
    assert!(!completion_matches(&first, &wrong_protocol, &candidate));
    let mut bad = candidate.clone();
    bad.video = VideoSpec::new(
        FrameDuration::new(32).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        768,
        320,
    )
    .unwrap();
    assert!(!completion_matches(&first, &completed(bad.clone()), &bad));
    let mut bad = candidate;
    bad.provider.seed += 1;
    assert!(!completion_matches(&first, &completed(bad.clone()), &bad));
}

/// The scripted process returns independently encoded footage, then removes
/// its input files. Qualification must use the host's prelaunch retained copy.
fn scripted_runtime(
    directory: &Path,
    allocated: &Allocated,
    worker: &synthetic::SyntheticWorker,
) -> BridgeRuntime {
    std::fs::create_dir(directory.join("outputs")).unwrap();
    let declaration = synthetic::synthesize(
        allocated,
        worker,
        directory,
        &AtomicBool::new(false),
        synthetic::SyntheticMode::Blend,
    )
    .unwrap();
    std::fs::write(
        directory.join("candidate.json"),
        serde_json::to_vec(&declaration).unwrap(),
    )
    .unwrap();
    std::fs::write(directory.join("worker.py"), SCRIPTED_WORKER).unwrap();
    BridgeRuntime {
        python: python(),
        runtime_source: directory.to_owned(),
        model_cache: directory.to_owned(),
        model_manifest: manifest(),
        worker_script: directory.join("worker.py"),
        ffmpeg: worker.ffmpeg.clone(),
        ffprobe: PathBuf::from("/usr/bin/true"),
        media_worker: worker.media_worker.clone(),
        landmark_worker: worker.landmark_worker.clone(),
    }
}

#[test]
fn supervised_extension_both_directions_publish_all_inputs_then_accept_and_reopen() {
    let Some(worker) = synthetic_tools() else {
        eprintln!("skipped: needs ffmpeg and built media/tracking workers");
        return;
    };
    for (direction, opposite) in [
        (ExtensionDirection::FromLeft, false),
        (ExtensionDirection::FromRight, true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut store = project_with_context(directory.path(), direction, opposite);
        store.set_generation_context_resolver(std::sync::Arc::new(
            crate::generation_context::BoundaryContextResolver::default(),
        ));
        let first = allocate_extension(directory.path(), &mut store, direction);
        let before = store.snapshot().unwrap();
        let runtime = scripted_runtime(directory.path(), &first, &worker);
        let mut stages = Vec::new();
        let run = run_worker(
            &first,
            &runtime,
            |progress| stages.push(progress),
            |entry| record(&mut store, &first, &entry).map_err(|error| error.to_string()),
            &AtomicBool::new(false),
        );
        let worker_log = run.worker_log.clone();
        let finished = finish(&mut store, &first, run).unwrap();
        assert_eq!(
            finished.state,
            JobState::Ready,
            "{:?} {worker_log}",
            finished.failure
        );
        assert!(stages.contains(&AttemptProgress::Qualifying));
        assert_eq!(
            store.snapshot().unwrap(),
            before,
            "Ready cannot accept itself"
        );
        let receipt = finished.receipt.unwrap();
        assert_eq!(receipt.plan(), &first.inputs.plan());
        let inputs = receipt.admission().unwrap().inputs();
        assert_eq!(inputs.context().unwrap().len(), 9);
        assert_eq!(inputs.opposite().is_some(), opposite);
        assert!(inputs.signatures().is_some());
        for object in inputs.objects() {
            store
                .generated_read_handle()
                .snapshot(
                    object,
                    deadpan_store::generated_media::GeneratedReadLimits::new(
                        object.byte_length(),
                        Duration::from_secs(30),
                    )
                    .unwrap(),
                    &AtomicBool::new(false),
                )
                .unwrap();
        }
        store.validate_full().unwrap();
        crate::generation::acceptance::accept(
            &mut store,
            &first.request.request_id,
            RevisionId::new("accepted-extension").unwrap(),
        )
        .unwrap();
        let accepted = store.snapshot().unwrap();
        let deadpan_core::NodeKind::Hold { recipe } = &accepted.nodes()[&hold_id()].kind else {
            unreachable!()
        };
        assert!(matches!(recipe.video, HoldVideo::Generated { .. }));
        assert_eq!(recipe.duration.frames(), 30);
        store
            .undo(
                accepted.revision_id(),
                RevisionId::new("undo-extension").unwrap(),
            )
            .unwrap();
        let undone = store.snapshot().unwrap();
        let deadpan_core::NodeKind::Hold { recipe } = &undone.nodes()[&hold_id()].kind else {
            unreachable!()
        };
        assert_eq!(recipe.video, HoldVideo::Background);
        store
            .redo(
                undone.revision_id(),
                RevisionId::new("redo-extension").unwrap(),
            )
            .unwrap();
        drop(store);
        let reopened = ProjectStore::open(
            &directory.path().join("test.deadpan"),
            deadpan_store::AccessMode::ReadOnly,
        )
        .unwrap();
        reopened.validate_full().unwrap();
        assert_eq!(reopened.head_revision().unwrap().as_str(), "redo-extension");
    }
}

const SCRIPTED_WORKER: &str = r#"
import argparse, json, shutil, struct, sys
from pathlib import Path
parser = argparse.ArgumentParser()
parser.add_argument('--runtime-config', required=True)
args = parser.parse_args()
runtime = json.loads(Path(args.runtime_config).read_text())
source = Path(runtime['runtime_source'])
def exact(count):
    value = bytearray()
    while len(value) < count:
        chunk = sys.stdin.buffer.read(count - len(value))
        if not chunk: raise RuntimeError('incomplete host frame')
        value.extend(chunk)
    return bytes(value)
length = struct.unpack('>I', exact(4))[0]
assert 0 < length <= 256 * 1024
request = json.loads(exact(length))
assert request['operation'] == 'generate_extension' and request['protocol'] == 3
def emit(message):
    data = json.dumps(message).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(data)) + data)
    sys.stdout.buffer.flush()
for stage in ['preflight', 'model_loading', 'inference', 'encoding']:
    emit({'event': 'stage', 'protocol': 3, 'identity': request['identity'], 'stage': stage})
candidate = json.loads((source / 'candidate.json').read_text())
assert candidate['provider'] == request['provider']
for name in ['native.mp4', 'provenance.json']:
    shutil.copyfile(source / 'outputs' / name, Path('outputs') / name)
shutil.rmtree('inputs')
emit({'event': 'completed_extension', 'protocol': 3, 'identity': request['identity'], 'candidate': candidate})
"#;

const CANCEL_WORKER: &str = r#"
import argparse, json, os, signal, struct, sys
from pathlib import Path
parser = argparse.ArgumentParser()
parser.add_argument('--runtime-config', required=True)
args = parser.parse_args()
runtime = json.loads(Path(args.runtime_config).read_text())
source = Path(runtime['runtime_source'])
(source / 'worker.pid').write_text(str(os.getpid()))
signal.alarm(10)
def exact(count):
    value = bytearray()
    while len(value) < count:
        chunk = sys.stdin.buffer.read(count - len(value))
        if not chunk: raise RuntimeError('incomplete host frame')
        value.extend(chunk)
    return bytes(value)
def receive():
    length = struct.unpack('>I', exact(4))[0]
    assert 0 < length <= 256 * 1024
    return json.loads(exact(length))
def emit(message):
    data = json.dumps(message).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(data)) + data)
    sys.stdout.buffer.flush()
request = receive()
assert request['operation'] == 'generate_extension' and request['protocol'] == 3
emit({'event': 'stage', 'protocol': 3, 'identity': request['identity'], 'stage': 'preflight'})
if (source / 'mode').read_text() == 'completion':
    for stage in ['model_loading', 'inference', 'encoding']:
        emit({'event': 'stage', 'protocol': 3, 'identity': request['identity'], 'stage': stage})
    candidate = json.loads((source / 'bad-candidate.json').read_text())
    emit({'event': 'completed_extension', 'protocol': 3, 'identity': request['identity'], 'candidate': candidate})
    sys.exit(0)
cancel = receive()
assert cancel['operation'] == 'cancel' and cancel['protocol'] == 3
assert cancel['identity'] == request['identity']
assert cancel['cancellation_token'] == request['cancellation_token']
(source / 'received-cancel').write_text(str(cancel['protocol']))
emit({'event': 'cancelled', 'protocol': 3, 'identity': request['identity']})
"#;
