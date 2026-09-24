use std::collections::BTreeMap;

use deadpan_core::{
    BeatNode, Command, CommandRequest, EndpointPolicy, ExactRatio, FrameDuration, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, ProjectDocument, ProjectId, RevisionId, SourceVideoMapping,
    Subtree,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_plan::RenderPlan;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::OriginalOwnership;
use deadpan_store::source_registration::{
    SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration,
};

use super::*;

#[test]
fn index_validation_observes_cancellation_between_bounded_chunks() {
    let make = |last_keyframe| {
        SourceFrameIndex::new(
            asset(),
            deadpan_core::SourceTimeBase::new(1, 30).unwrap(),
            (0..4096)
                .map(|i| deadpan_core::IndexedSourceFrame {
                    identity: SourceFrameId(i as u64),
                    pts: i,
                    reported_duration: Some(1),
                    keyframe: i == 0 || (i == 4095 && last_keyframe),
                    seek_from: Some(SourceFrameId(0)),
                    decode_timestamp: Some(i),
                })
                .collect(),
            4096,
            deadpan_core::TerminalProvenance::Explicit,
        )
        .unwrap()
    };
    let left = make(false);
    let mut polls = 0;
    let error = same_index_mapping(&left, &left, || {
        polls += 1;
        polls == 3
    })
    .unwrap_err();
    assert!(error.contains("cancelled"));
    assert_eq!(polls, 3);
    assert!(same_index_mapping(&left, &left, || false).unwrap());
    assert!(!same_index_mapping(&left, &make(true), || false).unwrap());
}

struct Fixture {
    scratch: tempfile::TempDir,
    store: ProjectStore,
}

impl Fixture {
    fn empty() -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let document = ProjectDocument::new_automatic(
            ProjectId::new("worker-project").unwrap(),
            revision("initial"),
            node("root"),
        )
        .unwrap();
        let store =
            ProjectStore::create(&scratch.path().join("preview.deadpan"), &document).unwrap();
        Self { scratch, store }
    }

    fn source(name: &str) -> Self {
        let mut fixture = Self::empty();
        let cancelled = AtomicBool::new(false);
        let limits = OriginalMediaLimits::default();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures")
            .join(name)
            .canonicalize()
            .unwrap();
        let original = fixture
            .store
            .retain_original(&path, OriginalOwnership::Managed, limits, &cancelled)
            .unwrap()
            .record;
        let mut snapshot = fixture
            .store
            .snapshot_original(original.object().content(), limits, &cancelled)
            .unwrap();
        let input = VerifiedSourceInput::copy_verified(
            &mut snapshot,
            SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
            2_000_000,
            Duration::from_secs(10),
            &cancelled,
        )
        .unwrap();
        let video = SourceSession::open_input(
            input.clone(),
            asset(),
            SourceSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let audio = video.info().audio_streams.first().map(|stream| {
            AudioSession::open_input(
                input,
                stream.stream_index,
                AudioSessionLimits::default(),
                &cancelled,
            )
            .unwrap()
        });
        let decoded =
            DecodedSourceQualification::from_sessions(Some(&video), audio.as_ref()).unwrap();
        fixture
            .store
            .register_source(
                &SourceRegistration {
                    expected_revision: revision("initial"),
                    new_revision: revision("registered"),
                    original: original.object().content().clone(),
                    new_asset_id: asset(),
                    label: name.into(),
                    insertion: Some(SourceInsertionRequest {
                        parent: node("root"),
                        index: 0,
                        node: node("source"),
                        label: "Full source".into(),
                        purpose: SourceInsertionPurpose::Primary,
                    }),
                },
                &decoded,
                None,
                limits,
                &cancelled,
            )
            .unwrap();
        fixture
    }

    fn workspace(&self, session: u64) -> Arc<Workspace> {
        let document = Arc::new(self.store.snapshot().unwrap());
        let sources = document
            .assets()
            .iter()
            .map(|(asset, record)| {
                let receipt = self
                    .store
                    .registered_source(document.revision_id(), asset)
                    .unwrap();
                let original = self
                    .store
                    .original_record(receipt.original().content())
                    .unwrap()
                    .unwrap();
                let video_index = Some(
                    self.store
                        .source_video_index(document.revision_id(), asset)
                        .unwrap(),
                );
                (
                    asset.clone(),
                    Arc::new(RegisteredSource {
                        asset: asset.clone(),
                        label: record.label.clone(),
                        receipt: Arc::new(receipt),
                        original,
                        video_index,
                    }),
                )
            })
            .collect();
        Arc::new(Workspace {
            session,
            path: self.scratch.path().join("preview.deadpan"),
            plan: Arc::new(RenderPlan::compile(&document).unwrap()),
            document,
            sources,
            originals: self.store.original_import_handle().unwrap(),
            can_undo: false,
            can_redo: false,
            single_source: None,
            original_duration: None,
        })
    }

    fn append_hold(&mut self, name: &str, index: usize, video: HoldVideo) {
        let document = self.store.snapshot().unwrap();
        self.store
            .commit(&CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision(name),
                command: Command::Insert {
                    parent: node("root"),
                    index,
                    subtree: Subtree {
                        root: node(name),
                        nodes: BTreeMap::from([(
                            node(name),
                            BeatNode::hold(
                                name,
                                HoldRecipe {
                                    duration: FrameDuration::new(3).unwrap(),
                                    video,
                                    audio: HoldAudio::Silence,
                                },
                            ),
                        )]),
                        overrides: BTreeMap::new(),
                    },
                },
            })
            .unwrap();
    }
}

fn asset() -> AssetId {
    AssetId::new("camera").unwrap()
}

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn source(frame: u64) -> ProjectView {
    ProjectView::Source {
        asset: asset(),
        frame: SourceFrameId(frame),
    }
}

fn sequence(frame: i64) -> ProjectView {
    ProjectView::Sequence {
        frame: ProjectFrame(frame),
    }
}

fn request(workspace: &Arc<Workspace>, view: ProjectView, serial: u64) -> Request {
    Request {
        ticket: Ticket {
            transport: None,
            source: workspace.session,
            request: serial,
        },
        work: Work::Project {
            workspace: workspace.clone(),
            view,
        },
        cancelled: Arc::new(AtomicBool::new(false)),
    }
}

fn await_reply(worker: &PreviewWorker) -> Reply {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(reply) = worker.take_reply() {
            return reply;
        }
        assert!(
            Instant::now() < deadline,
            "project preview response deadline"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn actual_decoded_freeze_keeps_sequence_position_and_rejects_old_revision_reply() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let before = fixture.workspace(1);
    let time_base = before.sources[&asset()]
        .video_index
        .as_ref()
        .unwrap()
        .time_base();
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    let mut presentation = crate::presentation::Presentation::default();
    let first = request(&before, sequence(20), 1);
    presentation.request(first.ticket, &first.work);
    worker.submit(first.ticket, first.work);
    let old_reply = await_reply(&worker);

    fixture.append_hold(
        "freeze",
        1,
        HoldVideo::Freeze {
            asset: asset(),
            timestamp: SourceTimestamp {
                ticks: 20 * 1001,
                time_base,
            },
        },
    );
    let after = fixture.workspace(1);
    assert_ne!(before.document.revision_id(), after.document.revision_id());
    presentation.clear();
    let next = request(&after, sequence(121), 2);
    presentation.request(next.ticket, &next.work);
    assert!(presentation.receive(old_reply).is_none());
    assert!(presentation.loading());
    assert!(!presentation.has_displayed());
    worker.submit(next.ticket, next.work);
    assert!(presentation.receive(await_reply(&worker)).unwrap().is_ok());
    let frozen_bytes = presentation
        .picture()
        .unwrap()
        .frame
        .as_ref()
        .unwrap()
        .bytes()
        .to_vec();
    assert_eq!(presentation.picture().unwrap().id, SourceFrameId(20));
    assert_eq!(presentation.displayed_label(), None);
    presentation.presented();
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing sequence frame 122")
    );
    assert_eq!(
        presentation.displayed_source_frame(),
        Some(SourceFrameId(20))
    );

    let next = request(&after, sequence(122), 3);
    presentation.request(next.ticket, &next.work);
    worker.submit(next.ticket, next.work);
    assert!(presentation.receive(await_reply(&worker)).unwrap().is_ok());
    assert_eq!(
        presentation
            .picture()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .bytes(),
        frozen_bytes
    );
    assert!(
        presentation.needs_render(),
        "identical decoded pixels still occupy a new sequence frame"
    );
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing sequence frame 122")
    );
    presentation.presented();
    assert_eq!(
        presentation.displayed_label().as_deref(),
        Some("Showing sequence frame 123")
    );
    worker.shutdown();
}

#[test]
fn registered_source_and_sequence_decode_the_same_original_frame() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    assert_eq!(workspace.plan.duration().frames(), 120);
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    let first = request(&workspace, source(20), 1);
    worker.submit(first.ticket, first.work);
    let first = await_reply(&worker).picture.unwrap();
    assert_eq!(first.id, SourceFrameId(20));
    assert_eq!(first.canvas, None);
    assert_eq!(
        first.frame.as_ref().unwrap().metadata().pts.ticks,
        20 * 1001
    );
    for serial in 2..=20 {
        let next = request(&workspace, sequence(serial as i64), serial);
        worker.submit(next.ticket, next.work);
    }
    let last = await_reply(&worker);
    assert_eq!(last.ticket.request, 20);
    let last = last.picture.unwrap();
    assert_eq!(last.id, SourceFrameId(20));
    assert_eq!(last.canvas, Some((320, 180)));
    assert_eq!(last.frame.unwrap().bytes(), first.frame.unwrap().bytes());
    worker.clear();
    let after_clear = request(&workspace, source(42), 21);
    worker.submit(after_clear.ticket, after_clear.work);
    assert_eq!(await_reply(&worker).picture.unwrap().id, SourceFrameId(42));
    worker.shutdown();
}

#[test]
fn sequence_samples_exact_mapping_and_endpoint_policy_after_a_revision_change() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let before = fixture.workspace(1);
    let mut retained = None;
    let original = perform(&request(&before, sequence(20), 1), &mut retained).unwrap();
    assert_eq!(original.id, SourceFrameId(20));
    fixture
        .store
        .commit(&CommandRequest {
            project_id: before.document.project_id().clone(),
            expected_revision: before.document.revision_id().clone(),
            new_revision: revision("picture-rate"),
            command: Command::SetSourceVideoMapping {
                node: node("source"),
                mapping: SourceVideoMapping::Duration {
                    frames: ExactRatio::integer(60),
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
            },
        })
        .unwrap();
    let after = fixture.workspace(1);
    let faster = perform(&request(&after, sequence(20), 2), &mut retained).unwrap();
    assert_eq!(faster.id, SourceFrameId(41));
    assert_eq!(faster.frame.unwrap().metadata().pts.ticks, 41 * 1001);
    let endpoint = perform(&request(&after, sequence(119), 3), &mut retained).unwrap();
    assert_eq!(endpoint.id, SourceFrameId(119));
}

#[test]
fn freeze_background_and_empty_sequence_follow_the_plan() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let time_base = workspace.sources[&asset()]
        .video_index
        .as_ref()
        .unwrap()
        .time_base();
    fixture.append_hold(
        "freeze",
        1,
        HoldVideo::Freeze {
            asset: asset(),
            timestamp: SourceTimestamp {
                ticks: 20 * 1001,
                time_base,
            },
        },
    );
    fixture.append_hold("background", 2, HoldVideo::Background);
    let workspace = fixture.workspace(1);
    let mut retained = None;
    let frozen = perform(&request(&workspace, sequence(121), 1), &mut retained).unwrap();
    assert_eq!(frozen.id, SourceFrameId(20));
    assert_eq!(
        frozen.frame.as_ref().unwrap().metadata().pts.ticks,
        20 * 1001
    );
    let background = perform(&request(&workspace, sequence(123), 2), &mut retained).unwrap();
    assert!(background.frame.is_none());
    assert_eq!(background.canvas, Some((320, 180)));
    assert!(perform(&request(&workspace, sequence(126), 3), &mut retained).is_err());
    assert!(perform(&request(&workspace, source(120), 4), &mut retained).is_err());
    assert_eq!(
        perform(&request(&workspace, source(20), 5), &mut retained)
            .unwrap()
            .id,
        SourceFrameId(20)
    );
    let empty = Fixture::empty();
    let workspace = empty.workspace(2);
    let blank = perform(&request(&workspace, sequence(0), 4), &mut retained).unwrap();
    assert!(blank.frame.is_none());
    assert_eq!(blank.canvas, Some((1920, 1080)));
    assert!(perform(&request(&workspace, sequence(-1), 5), &mut retained).is_err());
}

#[test]
fn cancelled_open_does_not_leave_a_later_project_request_without_a_source() {
    let fixture = Fixture::source("cfr-bframes.mp4");
    let workspace = fixture.workspace(1);
    let mut mailbox = Mailbox::default();
    let first = request(&workspace, source(0), 1);
    mailbox.submit(first.ticket, first.work);
    let first = mailbox.start_next().unwrap();
    let replacement = request(&workspace, sequence(37), 2);
    mailbox.submit(replacement.ticket, replacement.work);
    let mut retained = None;
    let result = perform(&first, &mut retained);
    assert!(result.is_err());
    assert!(!mailbox.publish(Reply {
        ticket: first.ticket,
        picture: result
    }));
    let latest = mailbox.start_next().unwrap();
    let picture = perform(&latest, &mut retained).unwrap();
    assert_eq!(picture.id, SourceFrameId(37));
    assert_eq!(picture.frame.unwrap().metadata().pts.ticks, 37 * 1001);
}

#[test]
fn revision_receipt_index_and_session_identity_are_checked() {
    let mut fixture = Fixture::source("cfr-bframes.mp4");
    let old = fixture.workspace(1);
    fixture.append_hold("background", 1, HoldVideo::Background);
    let mut current = fixture.workspace(1);
    let current_mut = Arc::get_mut(&mut current).unwrap();
    current_mut.plan = old.plan.clone();
    let mut retained = None;
    assert!(
        perform(&request(&current, sequence(0), 1), &mut retained)
            .err()
            .unwrap()
            .contains("another project revision")
    );

    let other_fixture = Fixture::source("offset-bframes.mp4");
    let other = other_fixture.workspace(1);
    let mut inconsistent = fixture.workspace(1);
    Arc::get_mut(&mut inconsistent).unwrap().sources = other.sources.clone();
    assert!(
        perform(&request(&inconsistent, source(0), 2), &mut retained)
            .err()
            .unwrap()
            .contains("selected project revision")
    );

    let mut inconsistent = fixture.workspace(1);
    let registered = Arc::get_mut(
        Arc::get_mut(&mut inconsistent)
            .unwrap()
            .sources
            .get_mut(&asset())
            .unwrap(),
    )
    .unwrap();
    registered.video_index = other.sources[&asset()].video_index.clone();
    assert!(
        perform(&request(&inconsistent, source(0), 3), &mut retained)
            .err()
            .unwrap()
            .contains("immutable receipt")
    );

    let first = perform(&request(&old, source(0), 4), &mut retained).unwrap();
    let replacement = perform(&request(&other, source(0), 5), &mut retained).unwrap();
    assert_ne!(
        first.frame.unwrap().metadata().pts.ticks,
        replacement.frame.unwrap().metadata().pts.ticks
    );
    let new_session = other_fixture.workspace(2);
    drop(other_fixture.store);
    assert!(perform(&request(&new_session, source(0), 6), &mut retained).is_err());
    // Reopening was attempted despite identical alias and receipt. The old
    // private decoder cannot cross a project-service session boundary.
    assert!(retained.is_none());
}
