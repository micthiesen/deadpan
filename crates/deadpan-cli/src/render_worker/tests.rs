use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use deadpan_store::ProjectStore;

use super::*;

pub(super) fn pictures(path: &Path, label: &str) -> ProjectPictureSession {
    let document = ProjectDocument::new(
        ProjectId::new("render-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 2,
            height: 2,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut store = ProjectStore::create(path, &document).unwrap();
    let id = NodeId::new("background").unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("committed").unwrap(),
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: id.clone(),
                    nodes: BTreeMap::from([(
                        id,
                        BeatNode::hold(
                            label,
                            HoldRecipe {
                                duration: FrameDuration::new(2).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .unwrap();
    ProjectPictureSession::open_revision(
        path,
        &RevisionId::new("committed").unwrap(),
        None,
        &AtomicBool::new(false),
    )
    .unwrap()
}

pub(super) fn identity() -> protocol::RenderIdentity {
    protocol::RenderIdentity {
        request_id: RequestId::new("render-request").unwrap(),
        attempt_id: AttemptId::new("render-attempt").unwrap(),
    }
}

fn runtime() -> RenderWorkerRuntime {
    RenderWorkerRuntime {
        executable: "/missing-render-worker".into(),
        arguments: vec![],
        environment: BTreeMap::new(),
    }
}

fn request(path: &Path) -> RenderPictureRequest {
    RenderPictureRequest {
        package: path.into(),
        revision: RevisionId::new("committed").unwrap(),
        range: None,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-render").unwrap(),
    }
}

#[test]
fn full_document_hash_detects_same_labelled_revision_with_different_authored_state() {
    let scratch = tempfile::tempdir().unwrap();
    let first = pictures(&scratch.path().join("first.deadpan"), "first");
    let second = pictures(&scratch.path().join("second.deadpan"), "second");
    assert_eq!(first.project_id(), second.project_id());
    assert_eq!(first.revision(), second.revision());
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let first_hash = document_sha256(&first, &cancelled, deadline).unwrap();
    assert_eq!(
        first_hash,
        document_sha256(&first, &cancelled, deadline).unwrap()
    );
    assert_ne!(
        first_hash,
        document_sha256(&second, &cancelled, deadline).unwrap()
    );
    assert!(matches!(
        document_sha256(&first, &AtomicBool::new(true), deadline),
        Err(RenderWorkerError::Cancelled)
    ));
    assert!(matches!(
        document_sha256(&first, &cancelled, Instant::now()),
        Err(RenderWorkerError::Deadline)
    ));
}

#[test]
fn cancelled_expired_and_over_budget_preflight_never_launch_the_runtime() {
    let missing = Path::new("/missing-render-package");
    assert!(matches!(
        prepare(
            &runtime(),
            request(missing),
            RenderWorkerLimits::default(),
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(5),
            |_| panic!("no progress before launch")
        ),
        Err(RenderWorkerError::Cancelled)
    ));
    assert!(matches!(
        prepare(
            &runtime(),
            request(missing),
            RenderWorkerLimits::default(),
            &AtomicBool::new(false),
            Instant::now(),
            |_| panic!("no progress before launch")
        ),
        Err(RenderWorkerError::Deadline)
    ));
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("budget.deadpan");
    drop(pictures(&path, "budget"));
    let limits = RenderWorkerLimits {
        maximum_output_bytes: 11,
        ..Default::default()
    };
    assert!(matches!(
        prepare(
            &runtime(),
            request(&path),
            limits,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(5),
            |_| panic!("no progress before launch")
        ),
        Err(RenderWorkerError::Configuration(
            "captured picture range exceeds raw byte budget"
        ))
    ));
}
