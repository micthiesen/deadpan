//! Positive native admission controls for an ordinary Hold inside a fixed window.

use super::*;
use crate::{
    library::ProjectLibrary,
    project::{ImportStage, ProjectRequest, ProjectService, ProjectUpdate, scoped::Target},
};
use deadpan_store::{
    AccessMode, ProjectStore, original_media::OriginalOwnership, single_source::SingleSourceState,
};
use std::time::{Duration, Instant};

fn command(service: &ProjectService, request: ProjectRequest) -> ProjectUpdate {
    let lane = crate::project::Lane::of(&request);
    let completed = service.completed_for_check(lane);
    service.submit(request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(update) = service.take_completion(lane, completed) {
            return update;
        }
        assert!(Instant::now() < deadline, "project command timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn seed_command(store: &mut ProjectStore, command: Command, name: &str) {
    let document = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

fn exercise(output: i64, mapping_start: i64, mapping_end: i64, cursor: i64) {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
                .canonicalize()
                .unwrap(),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let initialized = loop {
        if let Some(update) = service.take_update()
            && update.import.as_ref().is_some_and(|status| {
                matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
            })
        {
            assert!(update.error.is_none(), "{:?}", update.error);
            assert_eq!(
                update.import.as_ref().unwrap().stage,
                ImportStage::Complete,
                "Original preparation status: {:?}",
                update.import
            );
            break update;
        }
        assert!(Instant::now() < deadline, "Original preparation timed out");
        std::thread::sleep(Duration::from_millis(2));
    };
    let opened = command(
        &service,
        ProjectRequest::Open(initialized.workspace.as_ref().unwrap().path.clone()),
    );
    let workspace = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        workspace.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    assert_eq!(workspace.plan.duration().frames(), 120);
    let path = workspace.path.clone();
    let parent = workspace.document.root().clone();
    command(&service, ProjectRequest::Close);
    let rate = node("fixed-duration-rate");
    let body = node("fixed-duration-body");
    let selected = node("fixed-duration-hold");
    let lead = node("fixed-duration-lead");
    let tail = node("fixed-duration-tail");
    let mut wrapper = BeatNode::sequence("Fixed window", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: body.clone(),
        duration: FrameDuration::new(output).unwrap(),
        mapping: FrameRange::new(ProjectFrame(mapping_start), ProjectFrame(mapping_end)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent,
            index: 0,
            subtree: Subtree {
                root: rate.clone(),
                nodes: BTreeMap::from([
                    (rate.clone(), wrapper.clone()),
                    (
                        body,
                        BeatNode::sequence(
                            "Interior",
                            vec![lead.clone(), selected.clone(), tail.clone()],
                        ),
                    ),
                    (lead, BeatNode::hold("Lead", recipe(2))),
                    (
                        selected.clone(),
                        BeatNode::hold("Resize this pause", recipe(8)),
                    ),
                    (tail, BeatNode::hold("Suffix", recipe(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "fixed-duration-fixture",
    );
    drop(store);
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let before = opened.workspace.as_ref().unwrap();
    before.document.validate().unwrap();
    assert!(
        before
            .document
            .nodes()
            .values()
            .all(|beat| !matches!(beat.kind, NodeKind::Repeat { .. }))
    );
    let picture = before.plan.picture(ProjectFrame(cursor)).unwrap();
    let presentation = picture
        .framing
        .iter()
        .find(|layer| layer.instance.node == selected)
        .expect("the interior cursor must actually display the selected Hold")
        .instance
        .clone();
    assert!(presentation.repeats.is_empty());
    let target = Target {
        session: before.session,
        navigation: 0,
        project: before.document.project_id().clone(),
        revision: before.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: rate.clone(),
        target: ScopedNodeTarget {
            node: selected.clone(),
            repeats: vec![],
        },
        presentation: Some(presentation),
        cursor: ProjectFrame(cursor),
        also: vec![],
    };
    target.validate(before).unwrap();
    assert!(
        target.semantic_framing(before).is_err(),
        "ordinary duration authority must not broaden framing authority"
    );
    let mut state =
        State::new(before, SequenceScope::default(), rate, ProjectFrame(cursor)).unwrap();
    assert!(state.enter(before).unwrap());
    state.select(before, 1).unwrap();
    assert_eq!(state.selected_target().node, selected);
    let visible = state
        .presentation(before)
        .unwrap()
        .expect("selected Hold has sampled visibility");
    assert!(visible.frames.contains(&u64::try_from(cursor).unwrap()));
    assert_eq!(
        state.hold_parameter(before).as_deref(),
        Some("hold-duration 8f")
    );
    assert!(
        state.hold_duration_available(),
        "a visible ordinary Hold inside a fixed unclocked Edit Retime must offer Change duration"
    );
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_ordinary_fixed_retime_hold_duration_admits_interior_capture() {
    exercise(20, 0, 20, 5);
}

#[test]
fn native_ordinary_fixed_retime_hold_duration_admits_cropped_fractional_visibility() {
    // The Hold's intrinsic start maps to -2/3; visible start is the crop at 0.
    // Its visible end maps to 14/3. Frame 2 is strictly inside that interval.
    exercise(10, 3, 18, 2);
}

#[test]
fn ordinary_duration_availability_visits_only_selected_ancestry_beside_a_million_plays() {
    let mut rate = retime("rate", "body", 0, 20, 20);
    if let NodeKind::Retime { pitch, .. } = &mut rate.1.kind {
        *pitch = PitchPolicy::FollowSpeed;
    }
    let document = fixture(
        "rate",
        vec![
            rate,
            sequence("body", &["lead", "pause", "tail"]),
            hold("lead", 2),
            hold("pause", 8),
            hold("tail", 10),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let document = edited(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("unrelated"),
                nodes: [
                    repeat("unrelated", "unit", 1_000_000, None),
                    hold("unit", 1),
                ]
                .into_iter()
                .collect(),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "million-plays",
    );
    let mut state = state(document, "rate", 5);
    state.enter_in().unwrap();
    state.select_in(1).unwrap();
    assert_eq!(
        presentation(&state).exact,
        ExactRatio::integer(2)..ExactRatio::integer(10)
    );
    for _ in 0..100 {
        assert_eq!(state.ordinary_hold_duration_depth(), Some(3));
        assert!(state.hold_duration_available());
        assert_eq!(
            state.hold_parameter_in().as_deref(),
            Some("hold-duration 8f")
        );
    }
    // Multi-play hints must reject before constructing counterpart targets.
    state
        .levels
        .last_mut()
        .unwrap()
        .also
        .push(deadpan_core::IterationId {
            allocation: revision("unrelated"),
            ordinal: 0,
        });
    assert!(state.hold_parameter_in().is_none());
    state.levels.last_mut().unwrap().also.clear();
    // An immutable revision's precomputed clock ownership is consulted even
    // when the current visible projection still looks like an ordinary Hold.
    Arc::make_mut(&mut state.index)
        .clocked_retimes
        .insert(node("rate"));
    assert!(!state.hold_duration_available());
    assert!(state.hold_parameter_in().is_none());
}
