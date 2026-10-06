//! Real decoded-media witnesses for the sealed copied and destination paths.

use super::*;
use crate::project::slice::{CopiedViewId, CopyId, test_copied_view, test_media_view};
use deadpan_core::{
    AudioTimingId, CapturedCanvas, CapturedEditSlice, CapturedFit, CapturedFraming, FrameRange,
    Framing, FramingPose, OccurrenceIdentities, SlicePasteIdentities,
};
use deadpan_playback::Snapshot;
use deadpan_store::slice_preview::SliceViewIdentities;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn edit(fixture: &mut Fixture, name: &str, command: Command) {
    let document = fixture.store.snapshot().unwrap();
    fixture
        .store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

fn capture(fixture: &Fixture, selection: FrameRange) -> CapturedEditSlice {
    let document = fixture.store.snapshot().unwrap();
    CapturedEditSlice::capture(
        &document,
        document.root(),
        selection,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn ids(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let count = slice.identity_requirements().unwrap();
    assert_eq!(count.marks, 0);
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..count.nodes)
                .map(|i| node(&format!("{name}-node-{i}")))
                .collect(),
            marks: Vec::new(),
        },
        aliases: (0..count.aliases)
            .map(|i| node(&format!("{name}-alias-{i}")))
            .collect(),
    }
}

fn copied(
    fixture: &Fixture,
    slice: &CapturedEditSlice,
    session: u64,
    name: &str,
) -> Arc<CopiedView> {
    let admitted = fixture
        .store
        .view_edit_slice(
            slice,
            SliceViewIdentities {
                empty_revision: revision(&format!("{name}-empty")),
                view_revision: revision(name),
                root: node(&format!("{name}-root")),
                paste: ids(slice, name),
            },
        )
        .unwrap();
    let id = CopiedViewId {
        copy: CopyId {
            session,
            project: slice.project_id().clone(),
            source_revision: slice.revision_id().clone(),
            request: 1,
            persisted_version: None,
        },
        parent: slice.parent().clone(),
        range: slice.range(),
    };
    test_copied_view(id, test_media_view(session, admitted).unwrap()).unwrap()
}

fn placement(
    fixture: &Fixture,
    base: &Arc<Workspace>,
    slice: &CapturedEditSlice,
    name: &str,
) -> (Arc<Snapshot>, Arc<MediaView>) {
    let command = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::SpliceSlice {
            parent: base.document.root().clone(),
            index: 0,
            slice: slice.clone(),
            identities: ids(slice, name),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    };
    let media = test_media_view(
        base.session,
        fixture.store.preview_edit_slice(&command).unwrap(),
    )
    .unwrap();
    let snapshot = Arc::new(
        Snapshot::proposed_edit_slice(&base.playback_snapshot(), media.admitted().clone(), 9, 1)
            .unwrap(),
    );
    (snapshot, media)
}

fn work(work: Work, serial: u64) -> Request {
    Request {
        ticket: Ticket {
            source: 71,
            request: serial,
            transport: None,
        },
        work,
        cancelled: Arc::new(AtomicBool::new(false)),
        arrived: Instant::now(),
    }
}

fn copied_request(view: &Arc<CopiedView>, frame: i64, serial: u64) -> Request {
    work(
        Work::Copied {
            view: view.clone(),
            frame: ProjectFrame(frame),
        },
        serial,
    )
}

fn endpoint_id(base: &Workspace, view: &CopiedView, change: u64) -> EndpointIdentity {
    EndpointIdentity {
        session: base.session,
        project: base.document.project_id().clone(),
        revision: base.document.revision_id().clone(),
        draft: 9,
        change,
        source: EndpointSourceId::Copied(view.id().clone()),
    }
}

fn endpoints(worker: &EndpointWorker) -> EndpointReply {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(reply) = worker.take_reply() {
            return reply;
        }
        assert!(
            Instant::now() < deadline,
            "copied endpoint response deadline"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn copied_endpoints_keep_owned_clocks_pixels_and_exclude_both_ancestor_views() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    for (name, owner, scale) in [
        ("source-camera", "source", 2),
        ("old-parent-camera", "root", 3),
    ] {
        edit(
            &mut fixture,
            name,
            Command::SetFraming {
                node: node(owner),
                framing: Some(
                    Framing::static_pose(FramingPose {
                        scale: ExactRatio::integer(scale),
                        ..Default::default()
                    })
                    .unwrap(),
                ),
            },
        );
    }
    let slice = capture(&fixture, range(20, 25));
    let view = copied(&fixture, &slice, 71, "copied-source");
    edit(
        &mut fixture,
        "destination-camera",
        Command::SetFraming {
            node: node("root"),
            framing: Some(
                Framing::static_pose(FramingPose {
                    scale: ExactRatio::integer(4),
                    ..Default::default()
                })
                .unwrap(),
            ),
        },
    );
    let base = fixture.workspace(71);
    let worker = EndpointWorker::new(egui::Context::default()).unwrap();
    let identity = endpoint_id(&base, &view, 1);
    worker.submit(identity.clone(), EndpointInput::Copied(view.clone()));
    let reply = endpoints(&worker);
    assert_eq!(reply.identity, identity);
    let pair = reply.pictures.unwrap();
    let mut decoder = None;
    let mut plan = None;
    for (endpoint, local, ordinal) in [(&pair.first, 0, 20), (&pair.last, 4, 24)] {
        assert_eq!(endpoint.id, SourceFrameId(ordinal));
        assert_eq!(endpoint.canvas, Some((320, 180)));
        assert_eq!(
            endpoint.frame.as_ref().unwrap().metadata().pts.ticks,
            ordinal as i64 * 1001
        );
        let owner = endpoint
            .framing
            .iter()
            .find(|layer| layer.pose.is_some())
            .unwrap();
        assert_eq!(owner.pose.unwrap().scale, ExactRatio::integer(2));
        assert_eq!(owner.duration.frames(), 120);
        assert_eq!(
            owner.local_position,
            ExactRatio::new(i128::from(2 * ordinal + 1), 2).unwrap()
        );
        assert_eq!(
            endpoint
                .framing
                .iter()
                .filter(|layer| layer.pose.is_some())
                .count(),
            1
        );
        let original =
            perform(&request(&base, source(ordinal), 1), &mut decoder, &mut plan).unwrap();
        assert_eq!(
            endpoint.frame.as_ref().unwrap().metadata(),
            original.frame.as_ref().unwrap().metadata()
        );
        assert_eq!(
            endpoint.frame.as_ref().unwrap().bytes(),
            original.frame.as_ref().unwrap().bytes()
        );
        let main = perform(&copied_request(&view, local, 2), &mut decoder, &mut plan).unwrap();
        assert_eq!(main.framing, endpoint.framing);
        assert_eq!(
            main.frame.as_ref().unwrap().bytes(),
            endpoint.frame.as_ref().unwrap().bytes()
        );
    }
    let (snapshot, media) = placement(&fixture, &base, &slice, "destination-proposal");
    let proposed = perform(
        &work(
            Work::EditedProposed {
                base: base.clone(),
                snapshot,
                media,
                frame: ProjectFrame(0),
            },
            3,
        ),
        &mut decoder,
        &mut plan,
    )
    .unwrap();
    assert_eq!(proposed.id, SourceFrameId(20));
    assert_eq!(
        proposed
            .framing
            .iter()
            .filter_map(|layer| layer.pose.map(|pose| pose.scale))
            .collect::<Vec<_>>(),
        vec![ExactRatio::integer(2), ExactRatio::integer(4)]
    );
    let mut wrong = identity;
    let EndpointSourceId::Copied(id) = &mut wrong.source else {
        unreachable!()
    };
    id.copy.request += 1;
    worker.submit(wrong.clone(), EndpointInput::Copied(view));
    let rejected = endpoints(&worker);
    assert_eq!(rejected.identity, wrong);
    assert!(rejected.pictures.is_err());
    worker.shutdown();
}

#[test]
fn copied_freeze_context_background_and_identical_images_keep_distinct_edit_clocks() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let initial = fixture.workspace(71);
    fixture.append_hold(
        "freeze",
        1,
        HoldVideo::Freeze {
            asset: asset(),
            timestamp: SourceTimestamp {
                ticks: 20 * 1001,
                time_base: initial.sources[&asset()]
                    .video_index
                    .as_ref()
                    .unwrap()
                    .time_base(),
            },
        },
    );
    fixture.append_hold("black", 2, HoldVideo::Background);
    let context = CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 102,
            height: 62,
            fit: CapturedFit::Fit,
            layers: vec![Some(FramingPose {
                scale: ExactRatio::integer(2),
                ..Default::default()
            })],
        },
    )
    .unwrap();
    edit(
        &mut fixture,
        "frozen-geometry",
        Command::SetHoldPictureContext {
            node: node("freeze"),
            context: Some(context.clone()),
        },
    );
    let slice = capture(&fixture, range(120, 126));
    let view = copied(&fixture, &slice, 71, "freeze-source");
    let base = fixture.workspace(71);
    let worker = EndpointWorker::new(egui::Context::default()).unwrap();
    worker.submit(
        endpoint_id(&base, &view, 1),
        EndpointInput::Copied(view.clone()),
    );
    let pair = endpoints(&worker).pictures.unwrap();
    assert_eq!(pair.first.id, SourceFrameId(20));
    assert_eq!(pair.first.picture_context.as_deref(), Some(&context));
    assert!(
        pair.last.frame.is_none(),
        "authored Background must finish the Out endpoint"
    );
    assert_eq!(pair.last.canvas, Some((320, 180)));
    let mut decoder = None;
    let mut plan = None;
    let first = perform(&copied_request(&view, 0, 1), &mut decoder, &mut plan).unwrap();
    let second = perform(&copied_request(&view, 1, 2), &mut decoder, &mut plan).unwrap();
    assert_eq!(
        first.frame.as_ref().unwrap().bytes(),
        second.frame.as_ref().unwrap().bytes()
    );
    assert_eq!(
        first.framing[0].local_position,
        ExactRatio::new(1, 2).unwrap()
    );
    assert_eq!(
        second.framing[0].local_position,
        ExactRatio::new(3, 2).unwrap()
    );
    assert_eq!(second.picture_context.as_deref(), Some(&context));
    let cancelled = copied_request(&view, 1, 3);
    cancelled.cancelled.store(true, Ordering::Release);
    assert!(perform(&cancelled, &mut decoder, &mut plan).is_err());
    assert!(perform(&copied_request(&view, 6, 4), &mut decoder, &mut plan).is_err());
    worker.shutdown();
}

#[test]
fn historical_source_decodes_after_undo_and_sealed_views_revoke_even_with_warm_media() {
    // A fixed presentation basis survives undoing first registration. Automatic
    // basis adoption is intentionally undone along with that registration.
    let scratch = tempfile::tempdir().unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("historical-pictures").unwrap(),
        revision("initial"),
        deadpan_core::PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: deadpan_core::FrameRate::new(30000, 1001).unwrap(),
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let store = ProjectStore::create(&scratch.path().join("preview.deadpan"), &initial).unwrap();
    let mut fixture = Fixture::with_source(Fixture { scratch, store }, "cfr-bframes.mp4");
    let slice = capture(&fixture, range(20, 25));
    fixture
        .store
        .undo(&revision("registered"), revision("undo-registration"))
        .unwrap();
    let base = fixture.workspace(71);
    assert!(base.sources.is_empty());
    let view = copied(&fixture, &slice, 71, "historical-copy");
    let (snapshot, media) = placement(&fixture, &base, &slice, "historical-placement");
    assert_eq!(snapshot.document.assets().len(), 1);
    let make = |snapshot: Arc<Snapshot>, media: Arc<MediaView>| {
        work(
            Work::EditedProposed {
                base: base.clone(),
                snapshot,
                media,
                frame: ProjectFrame(0),
            },
            2,
        )
    };
    let mut decoder = None;
    let mut plan = None;
    assert_eq!(
        perform(&copied_request(&view, 0, 1), &mut decoder, &mut plan)
            .unwrap()
            .id,
        SourceFrameId(20)
    );
    assert_eq!(
        perform(
            &make(snapshot.clone(), media.clone()),
            &mut decoder,
            &mut plan
        )
        .unwrap()
        .id,
        SourceFrameId(20)
    );
    let mut changed =
        Snapshot::proposed_edit_slice(&base.playback_snapshot(), media.admitted().clone(), 9, 1)
            .unwrap();
    Arc::make_mut(&mut changed.sources).clear();
    assert!(
        perform(
            &make(Arc::new(changed), media.clone()),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    let (other, other_media) = placement(&fixture, &base, &slice, "other-placement");
    assert!(
        perform(
            &make(snapshot.clone(), other_media.clone()),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    assert!(perform(&make(other, media.clone()), &mut decoder, &mut plan).is_err());
    assert!(
        perform(
            &work(
                Work::Proposed {
                    base: base.clone(),
                    snapshot: snapshot.clone(),
                    view: sequence(0)
                },
                3
            ),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    assert!(
        perform(
            &make(snapshot.clone(), media.clone()),
            &mut decoder,
            &mut plan
        )
        .is_ok()
    );
    let path = base.path.clone();
    drop(fixture.store);
    assert!(
        perform(
            &make(snapshot.clone(), media.clone()),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    assert!(perform(&copied_request(&view, 0, 4), &mut decoder, &mut plan).is_err());
    let reopened = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite).unwrap();
    assert!(perform(&make(snapshot, media), &mut decoder, &mut plan).is_err());
    assert!(perform(&copied_request(&view, 0, 5), &mut decoder, &mut plan).is_err());
    reopened.validate().unwrap();
}

#[test]
fn full_receipt_contracts_are_checked_once_per_view_not_per_warm_picture() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let base = fixture.workspace(71);
    assert!(
        !base.sources[&asset()]
            .receipt
            .snapshot()
            .audio()
            .unwrap()
            .frames()
            .is_empty(),
        "the regression fixture must include a measured audio index"
    );
    let slice = capture(&fixture, range(20, 25));
    let view = copied(&fixture, &slice, 71, "admission-once");
    let next = copied(&fixture, &slice, 71, "admission-next");
    let (snapshot, media) = placement(&fixture, &base, &slice, "admission-proposed");
    let checks = || slice_view::CATALOG_CONTRACT_CHECKS.with(std::cell::Cell::get);
    slice_view::CATALOG_CONTRACT_CHECKS.with(|checked| checked.set(0));
    let mut decoder = None;
    let mut plan = None;
    for local in 0..5 {
        let picture = perform(&copied_request(&view, local, 1), &mut decoder, &mut plan).unwrap();
        assert_eq!(picture.id, SourceFrameId(20 + local as u64));
        assert_eq!(
            checks(),
            1,
            "warm seeks must not reconstruct the receipt's full audio extent"
        );
    }
    assert!(perform(&copied_request(&next, 0, 2), &mut decoder, &mut plan).is_ok());
    assert_eq!(checks(), 2, "a different immutable view must be admitted");
    for local in 0..5 {
        let picture = perform(
            &work(
                Work::EditedProposed {
                    base: base.clone(),
                    snapshot: snapshot.clone(),
                    media: media.clone(),
                    frame: ProjectFrame(local),
                },
                3,
            ),
            &mut decoder,
            &mut plan,
        )
        .unwrap();
        assert_eq!(picture.id, SourceFrameId(20 + local as u64));
        assert_eq!(
            checks(),
            3,
            "edited proposal seeks share one complete contract admission"
        );
    }
}

#[test]
fn admitted_move_uses_existing_edited_picture_path_and_keeps_seals_and_revocation() {
    use deadpan_core::{MoveRangeDestination, SplitIdentities};

    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let base = fixture.workspace(71);
    let destination = MoveRangeDestination::Interior {
        parent: node("root"),
        target: node("source"),
        at: FrameDuration::new(60).unwrap(),
    };
    let query = base
        .document
        .range_move(&node("root"), range(20, 30), &destination)
        .unwrap();
    assert_eq!(query.required_ids, 7);
    let command = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision("move-picture"),
        command: Command::MoveRange {
            source_revision: base.document.revision_id().clone(),
            source_parent: node("root"),
            range: range(20, 30),
            destination,
            identities: SplitIdentities {
                nodes: (0..query.required_ids)
                    .map(|index| node(&format!("move-cut-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("move-picture"),
                ordinal: 0,
            },
        },
    };
    let media = test_media_view(
        base.session,
        fixture.store.preview_edit_slice(&command).unwrap(),
    )
    .unwrap();
    let snapshot = Arc::new(
        Snapshot::proposed_edit_slice(&base.playback_snapshot(), media.admitted().clone(), 12, 1)
            .unwrap(),
    );
    assert_eq!(snapshot.validate_edit_slice_view(media.admitted()), Ok(()));
    assert!(snapshot.validate_original_proposal().is_err());
    let make = |base: Arc<Workspace>, snapshot: Arc<Snapshot>, media: Arc<MediaView>, frame| {
        work(
            Work::EditedProposed {
                base,
                snapshot,
                media,
                frame: ProjectFrame(frame),
            },
            1,
        )
    };
    let mut decoder = None;
    let mut plan = None;
    // Explicit ordinals at the removal and both insertion edges, in cold then
    // shuffled warm order. This is the ordinary edited path, with no Move decoder.
    for (frame, ordinal) in [(59, 29), (20, 30), (50, 20), (19, 19), (60, 60), (49, 59)] {
        let actual = perform(
            &make(base.clone(), snapshot.clone(), media.clone(), frame),
            &mut decoder,
            &mut plan,
        )
        .unwrap();
        assert_eq!(actual.id, SourceFrameId(ordinal));
        assert_eq!(
            actual.frame.as_ref().unwrap().metadata().pts.ticks,
            ordinal as i64 * 1001
        );
        let original =
            perform(&request(&base, source(ordinal), 2), &mut decoder, &mut plan).unwrap();
        assert_eq!(
            actual.frame.as_ref().unwrap().metadata(),
            original.frame.as_ref().unwrap().metadata()
        );
        assert_eq!(
            actual.frame.as_ref().unwrap().bytes(),
            original.frame.as_ref().unwrap().bytes()
        );
    }
    assert!(
        perform(
            &work(
                Work::Proposed {
                    base: base.clone(),
                    snapshot: snapshot.clone(),
                    view: sequence(50)
                },
                3
            ),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    let alternative = test_media_view(
        base.session,
        fixture.store.preview_edit_slice(&command).unwrap(),
    )
    .unwrap();
    assert!(
        perform(
            &make(base.clone(), snapshot.clone(), alternative, 50),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    let mut changed =
        Snapshot::proposed_edit_slice(&base.playback_snapshot(), media.admitted().clone(), 12, 1)
            .unwrap();
    changed.document = Arc::new((*changed.document).clone());
    assert!(
        perform(
            &make(base.clone(), Arc::new(changed), media.clone(), 50),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    let cancelled = make(base.clone(), snapshot.clone(), media.clone(), 50);
    cancelled.cancelled.store(true, Ordering::Release);
    assert!(perform(&cancelled, &mut decoder, &mut plan).is_err());

    edit(
        &mut fixture,
        "later-edit",
        Command::Rename {
            node: node("source"),
            label: "Later source".into(),
        },
    );
    let current = fixture.workspace(71);
    let mut historical = command.clone();
    historical.expected_revision = current.document.revision_id().clone();
    historical.new_revision = revision("historical-move-refused");
    assert!(
        fixture.store.preview_edit_slice(&historical).is_err(),
        "new expected revision must not authorize old Move source"
    );
    assert!(
        perform(
            &make(current, snapshot.clone(), media.clone(), 50),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    assert!(
        perform(
            &make(base.clone(), snapshot.clone(), media.clone(), 50),
            &mut decoder,
            &mut plan
        )
        .is_ok()
    );
    let path = base.path.clone();
    drop(fixture.store);
    assert!(
        perform(
            &make(base.clone(), snapshot.clone(), media.clone(), 50),
            &mut decoder,
            &mut plan
        )
        .is_err()
    );
    assert!(
        perform(
            &make(base.clone(), snapshot.clone(), media.clone(), 50),
            &mut None,
            &mut None
        )
        .is_err()
    );
    // Closing revokes temporary views; the existing committed private decoder
    // contract still permits warm inspection of the already-admitted Original.
    assert!(perform(&request(&base, source(20), 4), &mut decoder, &mut plan).is_ok());
    let reopened = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite).unwrap();
    assert!(perform(&make(base, snapshot, media, 50), &mut decoder, &mut plan).is_err());
    reopened.validate().unwrap();
}
