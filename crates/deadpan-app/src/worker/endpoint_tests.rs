use super::*;
use crate::worker::project_tests::Fixture;

fn identity(workspace: &Workspace, change: u64, start: u64, end: u64) -> EndpointIdentity {
    EndpointIdentity {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        draft: 30,
        change,
        source: EndpointSourceId::Original {
            asset: workspace.sources.keys().next().unwrap().clone(),
            qualification: workspace
                .sources
                .values()
                .next()
                .unwrap()
                .receipt
                .id()
                .clone(),
            in_frame: SourceFrameId(start),
            out_frame: SourceFrameId(end),
        },
    }
}

fn await_endpoints(worker: &EndpointWorker) -> EndpointReply {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(reply) = worker.take_reply() {
            return reply;
        }
        assert!(Instant::now() < deadline, "endpoint response deadline");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn vfr_endpoints_use_exact_in_and_last_included_ordinals_and_canonical_pixels() {
    let fixture = Fixture::source("vfr.mp4");
    let workspace = fixture.workspace(81);
    let asset = workspace.sources.keys().next().unwrap();
    let index = workspace.sources[asset].video_index.as_ref().unwrap();
    assert!(
        index
            .frames()
            .windows(3)
            .any(|frames| { frames[1].pts - frames[0].pts != frames[2].pts - frames[1].pts }),
        "fixture must actually be VFR"
    );
    let count = index.frames().len() as u64;
    let worker = EndpointWorker::new(egui::Context::default()).unwrap();
    let mut reference_decoder = None;
    for (change, start, end) in [(1, 2, 17), (2, count - 2, count), (3, 9, 10)] {
        let identity = identity(&workspace, change, start, end);
        worker.submit(identity.clone(), EndpointInput::Original(workspace.clone()));
        let reply = await_endpoints(&worker);
        assert_eq!(reply.identity, identity);
        let pictures = reply.pictures.unwrap();
        for (picture, ordinal) in [(&pictures.first, start), (&pictures.last, end - 1)] {
            assert_eq!(picture.id, SourceFrameId(ordinal));
            assert_eq!(
                picture.frame.as_ref().unwrap().metadata().pts.ticks,
                index.frames()[ordinal as usize].pts
            );
            let expected = project_picture(
                &workspace,
                &workspace.document,
                &workspace.plan,
                &ProjectView::Source {
                    asset: asset.clone(),
                    frame: SourceFrameId(ordinal),
                },
                &AtomicBool::new(false),
                &mut reference_decoder,
            )
            .unwrap();
            assert_eq!(
                picture.frame.as_ref().unwrap().metadata(),
                expected.frame.as_ref().unwrap().metadata()
            );
            assert_eq!(
                picture.frame.as_ref().unwrap().bytes(),
                expected.frame.as_ref().unwrap().bytes()
            );
            assert!(picture.canvas.is_none());
            assert!(picture.framing.is_empty());
        }
    }
    worker.shutdown();
}

#[test]
fn endpoint_replacement_cancels_active_work_and_rejects_stale_success_and_failure() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(82);
    let first_identity = identity(&workspace, 1, 2, 17);
    let mut mailbox = EndpointMailbox::default();
    assert!(mailbox.submit(
        first_identity.clone(),
        EndpointInput::Original(workspace.clone())
    ));
    let EndpointWork::Source(active) = mailbox.start_next().unwrap() else {
        panic!("source endpoint request keeps its typed worker slot")
    };
    // Even resubmitting the identical external identity cancels this generation.
    assert!(mailbox.submit(
        first_identity.clone(),
        EndpointInput::Original(workspace.clone())
    ));
    assert!(active.cancelled.load(Ordering::Acquire));
    assert!(!mailbox.publish_source(
        EndpointReply {
            identity: first_identity.clone(),
            pictures: Err("late failure".into())
        },
        &active.cancelled
    ));
    let EndpointWork::Source(active) = mailbox.start_next().unwrap() else {
        panic!("replacement source endpoint request keeps its typed worker slot")
    };
    let mut decoder = None;
    let pictures = endpoint_pictures(&active, &mut decoder, &mut None).unwrap();
    let latest = identity(&workspace, 3, 35, 40);
    assert!(mailbox.submit(
        identity(&workspace, 2, 20, 25),
        EndpointInput::Original(workspace.clone())
    ));
    assert!(mailbox.submit(latest.clone(), EndpointInput::Original(workspace)));
    assert!(!mailbox.publish_source(
        EndpointReply {
            identity: first_identity,
            pictures: Ok(pictures)
        },
        &active.cancelled
    ));
    assert!(mailbox.reply.is_none());
    let EndpointWork::Source(current) = mailbox.start_next().unwrap() else {
        panic!("latest source endpoint request remains pending")
    };
    assert_eq!(current.identity, latest);
    assert!(mailbox.pending.is_none());
    let pictures = endpoint_pictures(&current, &mut decoder, &mut None).unwrap();
    assert!(mailbox.publish_source(
        EndpointReply {
            identity: latest.clone(),
            pictures: Ok(pictures)
        },
        &current.cancelled
    ));
    assert_eq!(mailbox.reply.as_ref().unwrap().identity, latest);
    mailbox.cancel();
    assert!(mailbox.reply.is_none());
    assert!(mailbox.latest.is_none());
}

#[test]
fn endpoint_queue_never_replaces_the_main_picture_and_rejects_invalid_contexts() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(83);
    let endpoints = EndpointWorker::new(egui::Context::default()).unwrap();
    let main = PreviewWorker::new(egui::Context::default()).unwrap();
    let ticket = Ticket {
        transport: None,
        source: 83,
        request: 1,
    };
    let main_asset = workspace.sources.keys().next().unwrap().clone();
    main.submit(
        ticket,
        Work::Project {
            workspace: workspace.clone(),
            view: ProjectView::Source {
                asset: main_asset,
                frame: SourceFrameId(100),
            },
        },
    );
    for change in 1..=20 {
        endpoints.submit(
            identity(&workspace, change, change, change + 5),
            EndpointInput::Original(workspace.clone()),
        );
    }
    let endpoint_reply = await_endpoints(&endpoints);
    assert_eq!(endpoint_reply.identity.change, 20);
    let pair = endpoint_reply.pictures.unwrap();
    assert_eq!(pair.first.id, SourceFrameId(20));
    assert_eq!(pair.last.id, SourceFrameId(24));
    let deadline = Instant::now() + Duration::from_secs(15);
    let main_reply = loop {
        if let Some(reply) = main.take_reply() {
            break reply;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(main_reply.ticket, ticket);
    assert_eq!(main_reply.picture.unwrap().id, SourceFrameId(100));
    for invalid in [
        EndpointIdentity {
            session: 84,
            ..identity(&workspace, 21, 0, 2)
        },
        EndpointIdentity {
            revision: RevisionId::new("wrong-revision").unwrap(),
            ..identity(&workspace, 22, 0, 2)
        },
        EndpointIdentity {
            project: ProjectId::new("wrong-project").unwrap(),
            ..identity(&workspace, 23, 0, 2)
        },
        EndpointIdentity {
            draft: 0,
            ..identity(&workspace, 24, 0, 2)
        },
        identity(&workspace, 0, 0, 2),
        identity(&workspace, 25, 2, 2),
        identity(&workspace, 26, 0, 121),
    ] {
        endpoints.submit(invalid.clone(), EndpointInput::Original(workspace.clone()));
        let reply = await_endpoints(&endpoints);
        assert_eq!(reply.identity, invalid);
        assert!(reply.pictures.is_err());
    }
    main.shutdown();
    endpoints.shutdown();
}

fn junction_identity(
    workspace: &Workspace,
    content: deadpan_playback::ContentIdentity,
    proposal_revision: Option<RevisionId>,
    side: JunctionSide,
    inspection: u64,
    boundary: i64,
) -> EditJunctionIdentity {
    let outgoing = (boundary > 0).then(|| ProjectFrame(boundary - 1));
    let incoming =
        (boundary < workspace.plan.duration().frames()).then_some(ProjectFrame(boundary));
    EditJunctionIdentity {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        base_revision: workspace.document.revision_id().clone(),
        draft: 41,
        change: 7,
        content,
        proposal_revision,
        inspection,
        side,
        role: JunctionRole::In,
        boundary: ProjectFrame(boundary),
        outgoing,
        incoming,
    }
}

fn await_junction(worker: &EndpointWorker) -> EditJunctionReply {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(reply) = worker.take_junction_reply() {
            return reply;
        }
        assert!(Instant::now() < deadline, "Edit junction response deadline");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn proposed_background_snapshot(base: &Arc<Workspace>) -> Arc<deadpan_playback::Snapshot> {
    use deadpan_core::{
        BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId,
        Subtree,
    };
    use std::collections::BTreeMap;

    let hold = NodeId::new("junction-proposed-background").unwrap();
    let request = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: RevisionId::new("junction-proposed-revision").unwrap(),
        command: Command::Insert {
            parent: NodeId::new("root").unwrap(),
            index: 0,
            subtree: Subtree {
                root: hold.clone(),
                nodes: BTreeMap::from([(
                    hold.clone(),
                    BeatNode::hold(
                        "Authored black lead",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(3).unwrap(),
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
    let patch = deadpan_core::apply(&base.document, &request)
        .unwrap()
        .forward;
    let document = Arc::new(patch.apply(&base.document).unwrap());
    Arc::new(
        deadpan_playback::Snapshot::proposed(&base.playback_snapshot(), document, 41, 7).unwrap(),
    )
}

#[test]
fn edit_junction_uses_exact_edit_boundaries_and_keeps_exterior_distinct_from_background() {
    use deadpan_playback::ContentIdentity;

    let fixture = Fixture::source("cfr-bframes.mp4");
    let base = fixture.workspace(91);
    let worker = EndpointWorker::new(egui::Context::default()).unwrap();

    let exterior_identity = junction_identity(
        &base,
        ContentIdentity::Committed,
        None,
        JunctionSide::Proposed,
        1,
        0,
    );
    worker.submit_junction(
        exterior_identity.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: None,
        },
    );
    let exterior = await_junction(&worker);
    assert_eq!(exterior.identity, exterior_identity);
    let exterior = exterior.pictures.unwrap();
    assert!(matches!(
        exterior.outgoing,
        EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing)
    ));
    let EditJunctionPicture::Picture(incoming) = exterior.incoming else {
        panic!("boundary zero has the real Edit frame zero on its incoming side")
    };
    assert_eq!(incoming.id, SourceFrameId(0));
    assert!(
        incoming.frame.is_some(),
        "the source frame is decoded media"
    );

    let snapshot = proposed_background_snapshot(&base);
    let proposed_content = snapshot.content.clone();
    let proposal_revision = Some(snapshot.document.revision_id().clone());
    let before_identity = junction_identity(
        &base,
        proposed_content.clone(),
        proposal_revision.clone(),
        JunctionSide::Before,
        2,
        0,
    );
    worker.submit_junction(
        before_identity.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: Some(snapshot.clone()),
        },
    );
    let before = await_junction(&worker);
    assert_eq!(before.identity, before_identity);
    let before = before.pictures.unwrap();
    assert!(matches!(
        before.outgoing,
        EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing)
    ));
    let EditJunctionPicture::Picture(before_zero) = before.incoming else {
        panic!("Before is authenticated by the proposal but renders committed content")
    };
    assert_eq!(before_zero.id, SourceFrameId(0));
    assert!(before_zero.frame.is_some());

    let proposed_identity = junction_identity(
        &base,
        proposed_content,
        proposal_revision,
        JunctionSide::Proposed,
        3,
        3,
    );
    worker.submit_junction(
        proposed_identity.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: Some(snapshot),
        },
    );
    let proposed = await_junction(&worker);
    assert_eq!(proposed.identity, proposed_identity);
    let proposed = proposed.pictures.unwrap();
    let EditJunctionPicture::Picture(outgoing) = proposed.outgoing else {
        panic!("authored Background is a real successful picture, not exterior absence")
    };
    assert!(
        outgoing.frame.is_none(),
        "the proposed lead is authored black"
    );
    assert_eq!(outgoing.canvas, Some(proposed.canvas));
    let EditJunctionPicture::Picture(incoming) = proposed.incoming else {
        panic!("the frame after the proposed lead is still present")
    };
    assert_eq!(incoming.id, SourceFrameId(0));
    worker.shutdown();
}

#[test]
fn edit_junction_rejects_forged_proposal_revision_and_boundary_addresses() {
    use deadpan_playback::ContentIdentity;

    let fixture = Fixture::source("cfr-bframes.mp4");
    let base = fixture.workspace(92);
    let snapshot = proposed_background_snapshot(&base);
    let worker = EndpointWorker::new(egui::Context::default()).unwrap();
    let valid = junction_identity(
        &base,
        snapshot.content.clone(),
        Some(snapshot.document.revision_id().clone()),
        JunctionSide::Before,
        5,
        0,
    );
    let mut forged_revision = valid.clone();
    forged_revision.proposal_revision = Some(RevisionId::new("wrong-proposal").unwrap());
    worker.submit_junction(
        forged_revision.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: Some(snapshot.clone()),
        },
    );
    let reply = await_junction(&worker);
    assert_eq!(reply.identity, forged_revision);
    assert!(reply.pictures.is_err());

    let mut missing_incoming = junction_identity(
        &base,
        ContentIdentity::Committed,
        None,
        JunctionSide::Proposed,
        6,
        0,
    );
    missing_incoming.incoming = None;
    worker.submit_junction(
        missing_incoming.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: None,
        },
    );
    let reply = await_junction(&worker);
    assert_eq!(reply.identity, missing_incoming);
    assert!(reply.pictures.is_err());
    worker.shutdown();
}

#[test]
fn edit_junction_latest_identity_rejects_stale_success_and_failure() {
    use deadpan_playback::ContentIdentity;

    let fixture = Fixture::source("cfr-bframes.mp4");
    let base = fixture.workspace(93);
    let mut mailbox = EndpointMailbox::default();
    let first_identity = junction_identity(
        &base,
        ContentIdentity::Committed,
        None,
        JunctionSide::Proposed,
        1,
        0,
    );
    assert!(mailbox.submit_junction(
        first_identity.clone(),
        EditJunctionInput {
            base: base.clone(),
            snapshot: None,
        },
    ));
    let EndpointWork::EditJunction(active) = mailbox.start_next().unwrap() else {
        panic!("junction request keeps its typed worker slot")
    };
    let newer = junction_identity(
        &base,
        ContentIdentity::Committed,
        None,
        JunctionSide::Proposed,
        2,
        2,
    );
    assert!(mailbox.submit_junction(
        newer.clone(),
        EditJunctionInput {
            base,
            snapshot: None,
        },
    ));
    assert!(active.cancelled.load(Ordering::Acquire));
    assert!(!mailbox.publish_junction(
        EditJunctionReply {
            identity: first_identity.clone(),
            pictures: Err("stale decode failure".into()),
        },
        &active.cancelled,
    ));
    let EndpointWork::EditJunction(current) = mailbox.start_next().unwrap() else {
        panic!("newest request remains pending")
    };
    assert_eq!(current.identity, newer);
    assert!(!mailbox.publish_junction(
        EditJunctionReply {
            identity: first_identity.clone(),
            pictures: Err("stale decode failure".into()),
        },
        &current.cancelled,
    ));
    assert!(!mailbox.publish_junction(
        EditJunctionReply {
            identity: first_identity,
            pictures: Ok(EditJunctionPictures {
                outgoing: EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing),
                incoming: EditJunctionPicture::Exterior(JunctionExterior::NoIncoming),
                canvas: (320, 180),
            }),
        },
        &current.cancelled,
    ));
    assert!(mailbox.junction_reply.is_none());
}
