use super::*;
use deadpan_playback::{ContentIdentity, Snapshot};

fn splice_document(
    fixture: &Fixture,
    base: &Workspace,
    name: &str,
    ordinals: std::ops::Range<u64>,
) -> Arc<ProjectDocument> {
    let receipt = &base.sources[&asset()].receipt;
    let timing = deadpan_media::source_import_timing::derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        ordinals,
        base.document.presentation_basis().frame_rate,
    )
    .unwrap();
    let request = CommandRequest {
        project_id: base.document.project_id().clone(),
        expected_revision: base.document.revision_id().clone(),
        new_revision: revision(name),
        command: Command::SpliceSource {
            parent: node("root"),
            index: 1,
            source: timing.source_node(asset()),
            id: node(name),
            label: "Proposed slice".into(),
            timing: deadpan_core::AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    };
    Arc::new(
        fixture
            .store
            .preview(&request)
            .unwrap()
            .forward
            .apply(&base.document)
            .unwrap(),
    )
}

fn proposed_request(
    base: &Arc<Workspace>,
    snapshot: &Arc<Snapshot>,
    frame: i64,
    serial: u64,
) -> Request {
    let mut request = request(base, sequence(frame), serial);
    request.work = Work::Proposed {
        base: base.clone(),
        snapshot: snapshot.clone(),
        view: sequence(frame),
    };
    request
}

#[test]
fn proposed_picture_uses_exact_slice_joins_without_stored_revision_and_retains_decoder() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    fixture.append_hold("prefix", 0, HoldVideo::Background);
    let base = fixture.workspace(61);
    let document = splice_document(&fixture, &base, "slice-one", 20..25);
    let snapshot = Arc::new(Snapshot::proposed(&base.playback_snapshot(), document, 9, 1).unwrap());
    let changed_document = splice_document(&fixture, &base, "slice-two", 40..45);
    let changed =
        Arc::new(Snapshot::proposed(&base.playback_snapshot(), changed_document, 9, 2).unwrap());
    assert!(
        fixture
            .store
            .snapshot_at(snapshot.document.revision_id())
            .is_err()
    );
    let mut retained = None;
    let mut plan = None;
    let before = perform(&request(&base, sequence(3), 1), &mut retained, &mut plan).unwrap();
    assert_eq!(before.id, SourceFrameId(0));
    let original_bytes = before.frame.unwrap().bytes().to_vec();
    // Closed original handles make an accidental decoder reopen fail. Proposed
    // reads must reuse the same verified source independently of its revision.
    drop(fixture.store);
    for (serial, frame, expected) in [
        (2, 2, None),
        (3, 3, Some(20)),
        (4, 7, Some(24)),
        (5, 8, Some(0)),
    ] {
        let picture = perform(
            &proposed_request(&base, &snapshot, frame, serial),
            &mut retained,
            &mut plan,
        )
        .unwrap();
        assert_eq!(picture.frame.as_ref().map(|_| picture.id.0), expected);
        if expected == Some(0) {
            assert_eq!(picture.frame.unwrap().bytes(), original_bytes);
        }
    }
    let changed_picture = perform(
        &proposed_request(&base, &changed, 3, 6),
        &mut retained,
        &mut plan,
    )
    .unwrap();
    assert_eq!(
        changed_picture.id,
        SourceFrameId(40),
        "a new proposal cannot reuse the old compiled mapping"
    );
    let before_again = perform(&request(&base, sequence(3), 7), &mut retained, &mut plan).unwrap();
    assert_eq!(before_again.id, SourceFrameId(0));
    let proposal_again = perform(
        &proposed_request(&base, &snapshot, 3, 8),
        &mut retained,
        &mut plan,
    )
    .unwrap();
    assert_eq!(proposal_again.id, SourceFrameId(20));
    retained = None;
    assert!(
        perform(
            &proposed_request(&base, &snapshot, 3, 9),
            &mut retained,
            &mut plan
        )
        .is_err()
    );
}

#[test]
fn proposed_picture_rejects_public_field_tampering_and_equal_but_foreign_base() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    fixture.append_hold("prefix", 0, HoldVideo::Background);
    let base = fixture.workspace(62);
    let document = splice_document(&fixture, &base, "slice", 20..25);
    let make = || Snapshot::proposed(&base.playback_snapshot(), document.clone(), 10, 1).unwrap();
    let rejects = |snapshot: Snapshot, workspace: &Arc<Workspace>| {
        let result = perform(
            &proposed_request(workspace, &Arc::new(snapshot), 3, 1),
            &mut None,
            &mut None,
        );
        assert!(
            result.is_err(),
            "invalid proposal must fail before media admission"
        );
    };
    let mut changed = make();
    changed.content = ContentIdentity::Proposed {
        base_revision: base.document.revision_id().clone(),
        draft: 10,
        change: 2,
    };
    rejects(changed, &base);
    let mut changed = make();
    changed.session += 1;
    rejects(changed, &base);
    let mut changed = make();
    changed.document = Arc::new((*document).clone());
    rejects(changed, &base);
    let mut changed = make();
    Arc::make_mut(&mut changed.sources).clear();
    rejects(changed, &base);
    let mut forged = base.playback_snapshot();
    forged.document = document.clone();
    forged.content = make().content;
    rejects(forged, &base);
    let equal_base = fixture.workspace(base.session);
    rejects(make(), &equal_base);
    rejects(make(), &fixture.workspace(base.session + 1));
    let mut wire = serde_json::to_value(document.as_ref()).unwrap();
    wire["assets"][asset().as_str()]["label"] = serde_json::json!("changed asset contract");
    assert!(
        Snapshot::proposed(
            &base.playback_snapshot(),
            Arc::new(ProjectDocument::from_json(&wire.to_string()).unwrap()),
            10,
            1
        )
        .is_err()
    );
}

#[test]
fn proposed_decode_and_presentation_reject_stale_changes_and_name_the_proposed_edit() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    fixture.append_hold("prefix", 0, HoldVideo::Background);
    let base = fixture.workspace(63);
    let first = Arc::new(
        Snapshot::proposed(
            &base.playback_snapshot(),
            splice_document(&fixture, &base, "first-slice", 20..25),
            11,
            1,
        )
        .unwrap(),
    );
    let next = Arc::new(
        Snapshot::proposed(
            &base.playback_snapshot(),
            splice_document(&fixture, &base, "next-slice", 40..45),
            11,
            2,
        )
        .unwrap(),
    );
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    let mut presentation = crate::presentation::Presentation::default();
    let old = proposed_request(&base, &first, 3, 1);
    presentation.request(old.ticket, &old.work);
    worker.submit(old.ticket, old.work);
    let late = await_reply(&worker);
    let current = proposed_request(&base, &next, 3, 2);
    presentation.request(current.ticket, &current.work);
    assert!(presentation.receive(late).is_none());
    worker.submit(current.ticket, current.work);
    assert!(presentation.receive(await_reply(&worker)).unwrap().is_ok());
    assert_eq!(presentation.displayed_label(), None);
    assert_eq!(presentation.picture().unwrap().id, SourceFrameId(40));
    presentation.presented();
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing proposed edit frame 4")
    );
    assert_eq!(
        presentation.stable_sequence_ticket(
            base.session,
            next.document.revision_id(),
            ProjectFrame(3)
        ),
        None,
        "proposal cannot become a committed Camera target"
    );
    worker.shutdown();
}
