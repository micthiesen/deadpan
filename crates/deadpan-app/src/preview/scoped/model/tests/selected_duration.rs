//! Qualified selected-play duration entry with a zero-net temporal control.

use super::*;
use crate::{
    library::ProjectLibrary,
    project::{ImportStage, ProjectRequest, ProjectService, ProjectUpdate, RoomToneSelection},
};
use deadpan_core::SourceVideo;
use deadpan_store::{
    AccessMode, ProjectStore, original_media::OriginalOwnership, single_source::SingleSourceState,
};
use std::time::{Duration, Instant};

mod service;

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

#[test]
fn native_selected_hold_duration_offers_entry_after_zero_net_compound_control() {
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
                .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
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
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let initial = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        initial.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    let qualified = initial
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let prepared = command(
        &service,
        ProjectRequest::PrepareRoomTone {
            expected_session: initial.session,
            expected_revision: initial.document.revision_id().clone(),
            ticket: 71,
            selection: RoomToneSelection::Original {
                asset: qualified.asset.clone(),
                qualification: qualified.receipt.id().clone(),
                ordinals: 1..4,
            },
        },
    );
    assert!(prepared.error.is_none(), "{:?}", prepared.error);
    assert!(
        prepared.room_tone_error.is_none(),
        "{:?}",
        prepared.room_tone_error
    );
    let audio = prepared.room_tone.unwrap().source;
    let NodeKind::Sequence { children } = &initial.document.nodes()[initial.document.root()].kind
    else {
        panic!("ordinary initial root");
    };
    assert_eq!(children.len(), 1);
    let original = children[0].clone();
    let NodeKind::Source { source } = &initial.document.nodes()[&original].kind else {
        panic!("full Original source");
    };
    let SourceVideo::Stream { asset, span } = &source.video else {
        panic!("qualified Original video");
    };
    assert!(
        source.audio.is_some(),
        "fixture must carry real qualified audio"
    );
    let audible_hold = |name: &str, length: i64| {
        (
            node(name),
            BeatNode::hold(
                name,
                HoldRecipe {
                    duration: frames(length),
                    video: HoldVideo::Freeze {
                        asset: asset.clone(),
                        timestamp: span.start(),
                    },
                    picture_context: None,
                    audio: HoldAudio::RoomTone {
                        source: audio.clone(),
                    },
                },
            ),
        )
    };
    let mut nodes = vec![
        repeat("shared-outer", "shared-inner", 3, None),
        repeat("shared-inner", "shared-body", 2, None),
        sequence(
            "shared-body",
            &["shared-before", "shared-pause", "shared-after"],
        ),
        audible_hold("shared-before", 2),
        audible_hold("shared-pause", 2),
        audible_hold("shared-after", 2),
        sequence(
            "inner-owned",
            &["inner-before", "inner-pause", "inner-after"],
        ),
        audible_hold("inner-before", 2),
        audible_hold("inner-pause", 4),
        audible_hold("inner-after", 2),
    ];
    let owned_names = ["outer-owned-zero", "outer-owned-one", "outer-owned-two"];
    let mut outer_overrides = vec![];
    for (ordinal, name) in owned_names.iter().enumerate() {
        if ordinal == 2 {
            let before = format!("{name}-before");
            let pause = format!("{name}-pause");
            let after = format!("{name}-after");
            nodes.push(sequence(name, &[&before, &pause, &after]));
            nodes.push(audible_hold(&before, 2));
            nodes.push(audible_hold(&pause, 7));
            nodes.push(audible_hold(&after, 2));
            outer_overrides.push((u32::try_from(ordinal).unwrap(), *name));
        }
    }
    let subtree = Subtree {
        root: node("shared-outer"),
        nodes: nodes.into_iter().collect(),
        overrides: [
            overrides("shared-inner", &[(1, "inner-owned")]),
            overrides("shared-outer", &outer_overrides),
        ]
        .into(),
        gap_overrides: BTreeMap::new(),
    };
    let lead = audible_hold("root-lead", 3);
    let tail = audible_hold("root-tail", 3);
    let path = initial.path.clone();
    let parent = initial.document.root().clone();
    command(&service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Delete { node: original },
        "shared-delete-baseline",
    );
    for (index, subtree) in [
        Subtree {
            root: lead.0.clone(),
            nodes: [lead].into(),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
        subtree,
        Subtree {
            root: tail.0.clone(),
            nodes: [tail].into(),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        seed_command(
            &mut store,
            Command::Insert {
                parent: parent.clone(),
                index,
                subtree,
            },
            &format!("shared-insert-{index}"),
        );
    }
    drop(store);
    let opened = command(&service, ProjectRequest::Open(path));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.as_ref().unwrap();
    assert!(matches!(
        workspace.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    workspace.document.validate().unwrap();
    assert!(workspace.document.sounds().is_empty());
    assert!(workspace.document.beat_sounds().is_empty());
    assert!(
        workspace
            .document
            .nodes()
            .values()
            .all(|beat| !matches!(beat.kind, NodeKind::Retime { .. }))
    );
    let offered = selected_duration_witness(workspace);
    // Every qualified selection and public temporal control passed. The old
    // model hides only this entry because the selected set has further plays.
    assert_eq!(
        offered.as_deref(),
        Some("hold-duration 2f"),
        "a valid several-play Hold set must offer its primary duration",
    );
    service::exercise(&service, &opened);
    command(&service, ProjectRequest::Close);
}

fn selected_duration_witness(workspace: &Workspace) -> Option<String> {
    use deadpan_core::{
        Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, Command, CommandRequest, LeafEdit,
        OccurrenceIdentities, RepeatEditBranch, ResolvedStep, ResolvedTransaction,
        apply_with_result, derive_scoped_isolation, prepare_scoped_hold_duration,
    };
    let before = &*workspace.document;
    before.validate().unwrap();
    assert_eq!(workspace.plan.duration().frames(), 45);
    assert!(before.sounds().is_empty() && before.beat_sounds().is_empty());
    assert!(
        before
            .nodes()
            .values()
            .all(|n| !matches!(n.kind, NodeKind::Retime { .. }))
    );
    let cursor = ProjectFrame(19);
    let mut state = State::new(
        workspace,
        SequenceScope::default(),
        node("shared-outer"),
        cursor,
    )
    .unwrap();
    assert!(state.enter(workspace).unwrap()); // outer Default
    state.switch_plays(workspace, &[1, 2]).unwrap();
    assert!(state.enter(workspace).unwrap());
    state.select(workspace, 1).unwrap();
    let captured = state.target(workspace, cursor).unwrap();
    captured.validate(workspace).unwrap();
    assert_eq!(captured.target.node, node("shared-pause"));
    assert_eq!(captured.also.len(), 1);
    assert_eq!(captured.also[0].node, node("inner-pause"));
    assert_eq!(captured.target.repeats.len(), 2);
    assert_eq!(captured.target.repeats[0].branch, RepeatEditBranch::Default);
    assert_eq!(captured.also[0].repeats[0], captured.target.repeats[0]);
    let NodeKind::Repeat {
        iterations: outer_plays,
        ..
    } = &before.nodes()[&node("shared-outer")].kind
    else {
        panic!("outer Repeat");
    };
    let NodeKind::Repeat {
        iterations: inner_plays,
        ..
    } = &before.nodes()[&node("shared-inner")].kind
    else {
        panic!("inner Repeat");
    };
    assert_eq!(outer_plays.len(), 3);
    assert_eq!(inner_plays.len(), 2);
    assert_eq!(
        captured.target.repeats[1].branch,
        RepeatEditBranch::Play {
            iteration: inner_plays.at(0).unwrap()
        }
    );
    assert_eq!(
        captured.also[0].repeats[1].branch,
        RepeatEditBranch::Play {
            iteration: inner_plays.at(1).unwrap()
        }
    );
    assert_ne!(captured.target.repeats[1], captured.also[0].repeats[1]);
    for t in std::iter::once(&captured.target).chain(&captured.also) {
        t.validate(before).unwrap();
        assert!(matches!(t.repeats[1].branch, RepeatEditBranch::Play { .. }));
    }
    let shown = captured.presentation.as_ref().unwrap();
    assert_eq!(shown.repeats[0].iteration, outer_plays.at(1).unwrap());
    assert!(captured.target.matches_instance(before, shown).unwrap());
    assert_eq!(
        state.presentation(workspace).unwrap().unwrap().frames,
        19..21
    );
    assert!(
        workspace
            .plan
            .picture(cursor)
            .unwrap()
            .framing
            .iter()
            .any(|layer| &layer.instance == shown)
    );

    // Derive other presentation from explicit stable Play and the captured
    // outer representative, independently of any sampled root cursor.
    let mut other_shown = shown.clone();
    other_shown.node = captured.also[0].node.clone();
    other_shown.repeats[1].iteration = match &captured.also[0].repeats[1].branch {
        RepeatEditBranch::Play { iteration } => iteration.clone(),
        RepeatEditBranch::Default => panic!("selected Play required"),
    };
    assert!(
        captured.also[0]
            .matches_instance(before, &other_shown)
            .unwrap()
    );
    let start = |document: &ProjectDocument, instance: &InstancePath| {
        let boundary = AnchorIndex::new(document)
            .unwrap()
            .resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: instance.clone(),
                        position: ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            })
            .unwrap();
        assert_eq!(boundary.exact_frame.denominator(), 1);
        boundary.frame
    };
    assert_eq!(start(before, shown), ProjectFrame(19));
    assert_eq!(start(before, &other_shown), ProjectFrame(25));

    let make_request = |doc: &ProjectDocument,
                        target: ScopedNodeTarget,
                        allocation: &str,
                        expected_nodes: usize| {
        let needs = doc
            .scoped_hold_duration_requirements(&target, frames(3))
            .unwrap();
        assert_eq!((needs.nodes, needs.marks), (expected_nodes, 0));
        CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: revision(allocation),
            command: Command::SetScopedHoldDuration {
                target,
                duration: frames(3),
                identities: OccurrenceIdentities {
                    nodes: (0..needs.nodes)
                        .map(|n| node(&format!("{allocation}-node-{n}")))
                        .collect(),
                    marks: vec![],
                },
            },
        }
    };
    let first = make_request(before, captured.target.clone(), "selected-first", 4);
    let p1 = prepare_scoped_hold_duration(before, &first, Some(shown)).unwrap();
    assert_eq!(p1.document.duration().unwrap().frames(), 47);
    assert_eq!(p1.boundary.as_ref().unwrap().frame, ProjectFrame(20));
    let other = p1.map_target(before, &captured.also[0]).unwrap();
    let other_shown = p1.map_instance(before, &other_shown).unwrap();
    assert_eq!(other.node, node("inner-pause"));
    let second = make_request(&p1.document, other, "selected-second", 0);
    let p2 = prepare_scoped_hold_duration(&p1.document, &second, Some(&other_shown)).unwrap();
    assert_eq!(p2.document.duration().unwrap().frames(), 45);
    assert_eq!(p2.boundary.as_ref().unwrap().frame, ProjectFrame(26));
    for owner in [
        "outer-owned-two",
        "outer-owned-two-before",
        "outer-owned-two-pause",
        "outer-owned-two-after",
        "root-tail",
    ] {
        assert_eq!(
            p2.document.nodes()[&node(owner)],
            before.nodes()[&node(owner)]
        );
    }
    let requested = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("selected-final"),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                vec![
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(first.new_revision, first.command).unwrap(),
                    },
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(second.new_revision, second.command).unwrap(),
                    },
                ],
            )
            .unwrap(),
        },
    };
    let (transaction, after) = apply_with_result(before, &requested).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    assert!(!transaction.changed_ids.is_empty());
    assert_eq!(after.nodes(), p2.document.nodes());
    assert_eq!(after.audio_bindings(), p2.document.audio_bindings());
    let proof = derive_scoped_isolation(before, &requested, &after).unwrap();
    let primary_final = proof.map_forward(&captured.target).unwrap();
    let other_final = proof.map_forward(&captured.also[0]).unwrap();
    let primary_shown = proof.map_instance_forward(shown).unwrap();
    assert!(
        primary_final
            .matches_instance(&after, &primary_shown)
            .unwrap()
    );
    assert_eq!(start(&after, &primary_shown), ProjectFrame(19));
    assert_eq!(other_final, p2.target);
    assert_eq!(
        start(&after, p2.presentation.as_ref().unwrap()),
        ProjectFrame(26)
    );
    for target in [&primary_final, &other_final] {
        assert!(matches!(&after.nodes()[&target.node].kind,
            NodeKind::Hold { recipe } if recipe.duration == frames(3)));
    }
    assert!(matches!(&after.nodes()[&node("shared-pause")].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(2)));
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );

    // No published service mutation occurred during the independent control.
    assert_eq!(*workspace.document, *before);
    // Generic one-play framing and scalar mixed authority remain restricted.
    assert!(captured.semantic_framing(workspace).is_err());
    assert!(captured.semantic_mixed_hold_duration(workspace).is_err());

    assert!(state.hold_duration_available()); // expected OLD positive prefix
    state.hold_parameter(workspace)
}
