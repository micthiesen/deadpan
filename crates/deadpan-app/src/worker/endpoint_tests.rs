use super::*;
use crate::worker::project_tests::Fixture;

fn identity(workspace: &Workspace, change: u64, start: u64, end: u64) -> EndpointIdentity {
    EndpointIdentity {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        draft: 30,
        change,
        asset: workspace.sources.keys().next().unwrap().clone(),
        in_frame: SourceFrameId(start),
        out_frame: SourceFrameId(end),
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
        worker.submit(identity.clone(), workspace.clone());
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
    assert!(mailbox.submit(first_identity.clone(), workspace.clone()));
    let active = mailbox.start_next().unwrap();
    // Even resubmitting the identical external identity cancels this generation.
    assert!(mailbox.submit(first_identity.clone(), workspace.clone()));
    assert!(active.cancelled.load(Ordering::Acquire));
    assert!(!mailbox.publish(
        EndpointReply {
            identity: first_identity.clone(),
            pictures: Err("late failure".into())
        },
        &active.cancelled
    ));
    let active = mailbox.start_next().unwrap();
    let mut decoder = None;
    let pictures = endpoint_pictures(&active, &mut decoder).unwrap();
    let latest = identity(&workspace, 3, 35, 40);
    assert!(mailbox.submit(identity(&workspace, 2, 20, 25), workspace.clone()));
    assert!(mailbox.submit(latest.clone(), workspace));
    assert!(!mailbox.publish(
        EndpointReply {
            identity: first_identity,
            pictures: Ok(pictures)
        },
        &active.cancelled
    ));
    assert!(mailbox.reply.is_none());
    let current = mailbox.start_next().unwrap();
    assert_eq!(current.identity, latest);
    assert!(mailbox.pending.is_none());
    let pictures = endpoint_pictures(&current, &mut decoder).unwrap();
    assert!(mailbox.publish(
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
            workspace.clone(),
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
        endpoints.submit(invalid.clone(), workspace.clone());
        let reply = await_endpoints(&endpoints);
        assert_eq!(reply.identity, invalid);
        assert!(reply.pictures.is_err());
    }
    main.shutdown();
    endpoints.shutdown();
}
