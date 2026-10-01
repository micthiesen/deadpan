use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use deadpan_core::{
    AssetRecord, BeatNode, CapturedCanvas, CapturedFit, Command, CommandRequest, ExactRatio,
    FrameDuration, Framing, FramingPose, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind,
    PitchPolicy, PresentationBasis, SourceFrameIndex, SourceTimeBase, SourceTimestamp,
    SplitIdentities, Subtree, TerminalProvenance,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::original_media::OriginalOwnership;
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
}
fn original_limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn initial(color_policy: ColorPolicy) -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("picture-worker").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy,
        },
        node("root"),
    )
    .unwrap()
}

struct Fixture {
    _scratch: tempfile::TempDir,
    path: PathBuf,
    source_path: PathBuf,
    store: ProjectStore,
}

impl Fixture {
    fn source(name: &str) -> Result<Self> {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("picture.deadpan");
        let source_path = scratch.path().join(name);
        fs::copy(fixture_path(name), &source_path)?;
        let mut store = ProjectStore::create(&path, &initial(ColorPolicy::SdrRec709))?;
        let original = store
            .retain_original(
                &source_path,
                OriginalOwnership::Linked { bookmark: None },
                original_limits(),
                &active(),
            )?
            .record;
        let mut snapshot =
            store.snapshot_original(original.object().content(), original_limits(), &active())?;
        let input = VerifiedSourceInput::copy_verified(
            &mut snapshot,
            SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
            2_000_000,
            Duration::from_secs(10),
            &active(),
        )?;
        let video = SourceSession::open_input(
            input.clone(),
            asset(),
            SourceSessionLimits::default(),
            &active(),
        )?;
        let audio = video
            .info()
            .audio_streams
            .first()
            .map(|stream| {
                AudioSession::open_input(
                    input,
                    stream.stream_index,
                    AudioSessionLimits::default(),
                    &active(),
                )
            })
            .transpose()?;
        let decoded = DecodedSourceQualification::from_sessions(Some(&video), audio.as_ref())?;
        store.register_source(
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
                    label: "Original".into(),
                    purpose: Default::default(),
                }),
            },
            &decoded,
            None,
            original_limits(),
            &active(),
        )?;
        Ok(Self {
            _scratch: scratch,
            path,
            source_path,
            store,
        })
    }

    fn edit(&mut self, name: &str, command: Command) -> Result {
        let document = self.store.snapshot()?;
        self.store.commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })?;
        Ok(())
    }

    fn open(&self, range: Option<FrameRange>) -> Result<ProjectPictureSession> {
        Ok(ProjectPictureSession::open_revision(
            &self.path,
            self.store.snapshot()?.revision_id(),
            range,
            &active(),
        )?)
    }

    fn hold(&mut self, name: &str, recipe: HoldRecipe) -> Result {
        let document = self.store.snapshot()?;
        let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
            panic!("root Sequence")
        };
        self.edit(
            name,
            Command::Insert {
                parent: node("root"),
                index: children.len(),
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), BeatNode::hold(name, recipe))]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        )
    }
}

fn decoded(picture: &PreparedProjectPicture) -> (&SourceFrameId, &Rgba8Frame) {
    match &picture.picture {
        PreparedPicture::Frame {
            asset: actual_asset,
            id,
            frame,
            ..
        } => {
            assert_eq!(actual_asset, &asset());
            (id, frame)
        }
        PreparedPicture::Background => panic!("expected decoded picture"),
        PreparedPicture::Generated { .. } => panic!("expected Original picture"),
    }
}

#[test]
fn real_cut_frames_stay_exact_across_live_deletion_undo_and_writer_close() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    let mut original = fixture.open(None)?;
    let expected = (38..43)
        .map(|at| original.prepare(ProjectFrame(at), &active()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    fixture.edit(
        "cut",
        Command::Split {
            node: node("source"),
            at: frames(40),
            identities: SplitIdentities {
                nodes: (0..8).map(|i| node(&format!("part-{i}"))).collect(),
            },
        },
    )?;
    let mut captured = fixture.open(Some(range(38, 43)))?;
    for (at, expected) in (38..43).zip(&expected) {
        let picture = captured.prepare(ProjectFrame(at), &active())?;
        assert_eq!(picture.revision_id, revision("cut"));
        assert_eq!(picture.project_frame, ProjectFrame(at));
        assert_eq!(picture.canvas, [320, 180]);
        assert_eq!(picture.frame_rate, FrameRate::new(30_000, 1001)?);
        assert_eq!(decoded(&picture).0, &SourceFrameId(at as u64));
        assert_eq!(
            decoded(&picture).1.metadata(),
            decoded(expected).1.metadata()
        );
        assert_eq!(decoded(&picture).1.bytes(), decoded(expected).1.bytes());
    }
    fixture.edit(
        "grouped-cut",
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 2,
            id: node("cut-group"),
            label: "Cut pair".into(),
        },
    )?;
    fixture.edit(
        "deleted",
        Command::Delete {
            node: node("cut-group"),
        },
    )?;
    assert!(matches!(
        ProjectPictureSession::open_revision(&fixture.path, &revision("deleted"), None, &active()),
        Err(ProjectPictureError::Range)
    ));
    for (head, undo) in [("deleted", true), ("undone", false)] {
        if undo {
            fixture.store.undo(&revision(head), revision("undone"))?;
        } else {
            fixture.store.redo(&revision(head), revision("redone"))?;
        }
        let picture = captured.prepare(ProjectFrame(40), &active())?;
        assert_eq!(picture.revision_id, revision("cut"));
        assert_eq!(decoded(&picture).1.bytes(), decoded(&expected[2]).1.bytes());
    }
    drop(fixture.store);
    // The read-only session has no writer-scoped import handle to revoke.
    assert_eq!(
        decoded(&captured.prepare(ProjectFrame(40), &active())?)
            .1
            .bytes(),
        decoded(&expected[2]).1.bytes()
    );
    let mut reopened = ProjectPictureSession::open_revision(
        &fixture.path,
        &revision("cut"),
        Some(range(40, 41)),
        &active(),
    )?;
    assert_eq!(
        decoded(&reopened.prepare(ProjectFrame(40), &active())?)
            .1
            .bytes(),
        decoded(&expected[2]).1.bytes()
    );
    assert!(matches!(
        captured.prepare(ProjectFrame(43), &active()),
        Err(ProjectPictureError::FrameOutOfRange { .. })
    ));
    Ok(())
}

#[test]
fn real_retime_repeat_freeze_and_captured_framing_keep_distinct_clocks() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    let index = fixture
        .store
        .source_video_index(&revision("registered"), &asset())?;
    let timestamp = SourceTimestamp {
        ticks: index.frames()[41].pts,
        time_base: index.time_base(),
    };
    fixture.edit(
        "source-framing",
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(FramingPose {
                scale: ExactRatio::integer(2),
                ..Default::default()
            })?),
        },
    )?;
    fixture.edit(
        "retimed",
        Command::WrapRetime {
            node: node("source"),
            id: node("retime"),
            duration: frames(60),
            pitch: PitchPolicy::Preserve,
        },
    )?;
    fixture.edit(
        "repeated",
        Command::WrapRepeat {
            node: node("retime"),
            id: node("repeat"),
            plays: 2,
            gap: Some(HoldRecipe {
                duration: frames(2),
                picture_context: None,
                video: HoldVideo::Freeze {
                    asset: asset(),
                    timestamp,
                },
                audio: HoldAudio::Silence,
            }),
            anchor_policy: Default::default(),
        },
    )?;
    let context = CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 320,
            height: 180,
            fit: CapturedFit::Fit,
            layers: vec![
                Some(FramingPose {
                    scale: ExactRatio::integer(2),
                    ..Default::default()
                }),
                None,
            ],
        },
    )?;
    fixture.hold(
        "freeze",
        HoldRecipe {
            duration: frames(3),
            picture_context: Some(context.clone()),
            video: HoldVideo::Freeze {
                asset: asset(),
                timestamp,
            },
            audio: HoldAudio::Silence,
        },
    )?;
    let mut session = fixture.open(None)?;
    assert_eq!(session.range(), range(0, 125));
    let first = session.prepare(ProjectFrame(20), &active())?;
    let second = session.prepare(ProjectFrame(82), &active())?;
    let gap = session.prepare(ProjectFrame(60), &active())?;
    let freeze = session.prepare(ProjectFrame(122), &active())?;
    for picture in [&first, &second, &gap, &freeze] {
        assert_eq!(decoded(picture).0, &SourceFrameId(41));
        assert_eq!(decoded(picture).1.metadata().pts, timestamp);
        assert_eq!(decoded(picture).1.bytes(), decoded(&first).1.bytes());
    }
    assert_eq!(first.framing[0].local_position, ExactRatio::integer(41));
    assert_eq!(first.framing[0].pose.unwrap().scale, ExactRatio::integer(2));
    assert_ne!(first.framing[0].instance, second.framing[0].instance);
    assert!(gap.gap_after.is_some());
    assert_eq!(gap.render_layers()?.len(), gap.framing.len() + 1);
    assert_eq!(gap.render_layers()?[0], FramingLayer::identity());
    assert_eq!(freeze.picture_context.as_deref(), Some(&context));
    assert!(Arc::ptr_eq(
        freeze.picture_context.as_ref().unwrap(),
        session
            .plan()
            .picture(ProjectFrame(124))?
            .picture_context
            .as_ref()
            .unwrap()
    ));
    Ok(())
}

#[test]
fn cached_verified_bytes_survive_path_loss_but_cold_or_changed_originals_fail() -> Result {
    let fixture = Fixture::source("offset-bframes.mp4")?;
    let mut retained = fixture.open(None)?;
    let first = retained.prepare(ProjectFrame(20), &active())?;
    let expected_pts = decoded(&first).1.metadata().pts;
    let expected = decoded(&first).1.bytes().to_vec();
    let index = fixture
        .store
        .source_video_index(&revision("registered"), &asset())?;
    let selected = retained
        .plan()
        .picture(ProjectFrame(20))?
        .picture
        .select_source_frame(&index)?;
    assert_eq!(expected_pts.ticks, selected.pts);
    assert_ne!(expected_pts.ticks, 20 * 1001); // original clock was not normalized
    fs::remove_file(&fixture.source_path)?;
    assert_eq!(
        decoded(&retained.prepare(ProjectFrame(20), &active())?)
            .1
            .bytes(),
        expected
    );
    assert!(
        fixture
            .open(None)?
            .prepare(ProjectFrame(20), &active())
            .is_err()
    );
    fs::copy(fixture_path("cfr-bframes.mp4"), &fixture.source_path)?;
    assert!(
        fixture
            .open(None)?
            .prepare(ProjectFrame(20), &active())
            .is_err()
    );
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        retained.prepare(ProjectFrame(20), &cancelled),
        Err(ProjectPictureError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        decoded(&retained.prepare(ProjectFrame(20), &cancelled)?)
            .1
            .bytes(),
        expected
    );
    Ok(())
}

#[test]
fn foreign_receipt_cannot_rebind_the_fixed_revision() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let other = Fixture::source("offset-bframes.mp4")?;
    let mut session = fixture.open(None)?;
    // Fault injection at the host boundary, without forging a writable store.
    // A document's alias alone must not authorize its counterpart in another snapshot.
    session.document = other.store.snapshot()?;
    assert!(matches!(
        session.prepare(ProjectFrame(20), &active()),
        Err(ProjectPictureError::SourceEvidence { .. })
    ));
    assert!(
        ProjectPictureSession::open_revision(&fixture.path, &revision("absent"), None, &active())
            .is_err()
    );
    Ok(())
}

#[test]
fn blank_and_background_are_explicit_and_ranges_hdr_and_bounds_reject() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("background.deadpan");
    let mut store = ProjectStore::create(&path, &initial(ColorPolicy::SdrRec709))?;
    assert!(matches!(
        ProjectPictureSession::open_revision(&path, &revision("initial"), None, &active()),
        Err(ProjectPictureError::Range)
    ));
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: revision("initial"),
        new_revision: revision("background"),
        command: Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode::hold(
                        "Black",
                        HoldRecipe {
                            duration: frames(3),
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
    })?;
    let mut session =
        ProjectPictureSession::open_revision(&path, &revision("background"), None, &active())?;
    let picture = session.prepare(ProjectFrame(2), &active())?;
    assert!(matches!(picture.picture, PreparedPicture::Background));
    assert_eq!(picture.project_frame, ProjectFrame(2));
    assert_eq!(picture.framing.len(), 2);
    assert!(session.retained.is_none());
    // Canonical geometry is preserved here; codec-even normalization belongs
    // to the later automatic output policy, not preparation of a saved frame.
    let mut odd_wire = serde_json::to_value(store.snapshot()?)?;
    odd_wire["presentation_basis"]["width"] = serde_json::json!(319);
    let odd_document = ProjectDocument::from_json(&odd_wire.to_string())?;
    let odd_path = scratch.path().join("odd.deadpan");
    let _odd_store = ProjectStore::create(&odd_path, &odd_document)?;
    let mut odd = ProjectPictureSession::open_revision(
        &odd_path,
        odd_document.revision_id(),
        None,
        &active(),
    )?;
    let odd_picture = odd.prepare(ProjectFrame(0), &active())?;
    assert_eq!(odd_picture.canvas, [319, 180]);
    assert!(matches!(odd_picture.picture, PreparedPicture::Background));
    for invalid in [range(0, 0), range(2, 2), range(-1, 2), range(0, 4)] {
        assert!(matches!(
            ProjectPictureSession::open_revision(
                &path,
                &revision("background"),
                Some(invalid),
                &active()
            ),
            Err(ProjectPictureError::Range)
        ));
    }
    assert!(matches!(
        ProjectPictureSession::open_revision(
            &path,
            &revision("background"),
            None,
            &AtomicBool::new(true)
        ),
        Err(ProjectPictureError::Cancelled)
    ));
    for policy in [ColorPolicy::HdrRec2020Pq, ColorPolicy::HdrRec2020Hlg] {
        let hdr_path = scratch.path().join(format!("{policy:?}.deadpan"));
        let _store = ProjectStore::create(&hdr_path, &initial(policy))?;
        assert!(matches!(
            ProjectPictureSession::open_revision(&hdr_path, &revision("initial"), None, &active()),
            Err(ProjectPictureError::HdrUnsupported)
        ));
    }
    for (width, height) in [(0, 2), (2, 0), (8193, 2), (8192, 8192)] {
        assert!(matches!(
            validate_raster(width, height),
            Err(ProjectPictureError::Limits(_))
        ));
    }
    // A valid audio-only Source has a Blank picture with its own scope. A
    // Source with neither audio nor video is rejected by core validation.
    let mut audible = Fixture::source("cfr-bframes.mp4")?;
    let document = audible.store.snapshot()?;
    let start = document.duration()?.frames();
    let mut blank = document.nodes()[&node("source")].clone();
    let NodeKind::Source { source } = &mut blank.kind else {
        panic!("fixture Source")
    };
    assert!(source.audio.is_some());
    source.video = deadpan_core::SourceVideo::Blank;
    source.video_mapping = deadpan_core::SourceVideoMapping::FitBeat;
    source.link = deadpan_core::LinkRelation::Independent;
    audible.edit(
        "blank",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("blank"),
                nodes: BTreeMap::from([(node("blank"), blank)]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    let mut session = audible.open(Some(range(start, start + 1)))?;
    let blank = session.prepare(ProjectFrame(start), &active())?;
    assert!(matches!(blank.picture, PreparedPicture::Background));
    assert_eq!(blank.framing[0].instance.node, node("blank"));
    assert!(session.retained.is_none());
    Ok(())
}

#[test]
fn unqualified_source_still_and_accepted_providers_fail_without_fallback() -> Result {
    for kind in ["source", "accepted", "still"] {
        let still = kind == "still";
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("unsupported.deadpan");
        let mut wire = serde_json::to_value(initial(ColorPolicy::SdrRec709))?;
        let record = AssetRecord {
            label: "Legacy".into(),
            content_hash: "a".repeat(64),
            video: if still {
                None
            } else {
                Some(deadpan_core::SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base: SourceTimeBase::new(1, 30)?,
                    },
                    SourceTimestamp {
                        ticks: 3,
                        time_base: SourceTimeBase::new(1, 30)?,
                    },
                )?)
            },
            audio: None,
            still_image: still,
            frame_count: if still { None } else { Some(frames(3)) },
            source_qualification: None,
        };
        let span = record.video;
        wire["assets"] = serde_json::to_value(BTreeMap::from([(asset(), record)]))?;
        let provider = if still || kind == "source" {
            BeatNode {
                label: "Still".into(),
                framing: None,
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                audio_treatments: Default::default(),
                kind: NodeKind::Source {
                    source: deadpan_core::SourceNode {
                        edit_window: None,
                        duration: frames(3),
                        video: if still {
                            deadpan_core::SourceVideo::Still { asset: asset() }
                        } else {
                            deadpan_core::SourceVideo::Stream {
                                asset: asset(),
                                span: span.unwrap(),
                            }
                        },
                        video_mapping: deadpan_core::SourceVideoMapping::FitBeat,
                        audio: None,
                        audio_mapping: deadpan_core::SourceAudioMapping::FitBeat,
                        link: deadpan_core::LinkRelation::Independent,
                        audio_offset: deadpan_core::AudioSample(0),
                    },
                },
            }
        } else {
            BeatNode::hold(
                "Accepted",
                HoldRecipe {
                    duration: frames(3),
                    picture_context: None,
                    video: HoldVideo::Accepted {
                        asset: asset(),
                        frames: range(0, 3),
                    },
                    audio: HoldAudio::Silence,
                },
            )
        };
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (
                node("root"),
                BeatNode::sequence("Root", vec![node("provider")]),
            ),
            (node("provider"), provider),
        ]))?;
        let document = ProjectDocument::from_json(&wire.to_string())?;
        let _store = ProjectStore::create(&path, &document)?;
        let mut session =
            ProjectPictureSession::open_revision(&path, document.revision_id(), None, &active())?;
        let error = session.prepare(ProjectFrame(0), &active()).unwrap_err();
        assert!(if still {
            matches!(error, ProjectPictureError::StillUnsupported(_))
        } else if kind == "source" {
            matches!(error, ProjectPictureError::SourceEvidence { .. })
        } else {
            matches!(error, ProjectPictureError::AcceptedUnsupported(_))
        });
        assert!(session.retained.is_none());
    }
    Ok(())
}

#[test]
fn shared_adapter_preserves_negative_pts_and_checks_decoded_geometry() -> Result {
    let fixture = Fixture::source("rotated90.mp4")?;
    let mut session = fixture.open(None)?;
    let _ = session.prepare(ProjectFrame(0), &active())?;
    let source = &mut session.retained.as_mut().unwrap().source;
    assert_eq!(source.info().rotation_quarter_turns, 3);
    let mut decoded = source.frame(SourceFrameId(0), FRAME_TIMEOUT, &active())?;
    decoded.metadata.pts = -37;
    let frame = source_to_render_frame(decoded.clone(), source.info())?;
    assert_eq!(frame.metadata().pts.ticks, -37);
    assert_eq!(
        frame.metadata().rotation,
        deadpan_render::Rotation::Clockwise270
    );
    decoded.width += 1;
    assert!(matches!(
        source_to_render_frame(decoded, source.info()),
        Err(ProjectPictureError::DecodedDimensions)
    ));
    Ok(())
}

#[test]
fn shared_index_comparison_includes_terminal_evidence_and_cancels_in_chunks() -> Result {
    let index = SourceFrameIndex::new(
        asset(),
        SourceTimeBase::new(1, 30)?,
        (0..4096)
            .map(|i| IndexedSourceFrame {
                identity: SourceFrameId(i as u64),
                pts: i,
                reported_duration: Some(1),
                keyframe: i == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: Some(i),
            })
            .collect(),
        4096,
        TerminalProvenance::Explicit,
    )?;
    let mut polls = 0;
    assert!(matches!(
        same_index_mapping(&index, &index, || {
            polls += 1;
            polls == 3
        }),
        Err(ProjectPictureError::Cancelled)
    ));
    assert_eq!(polls, 3);
    let changed = SourceFrameIndex::new(
        asset(),
        index.time_base(),
        index.frames().to_vec(),
        4097,
        TerminalProvenance::Explicit,
    )?;
    assert!(!same_index_mapping(&index, &changed, || false)?);
    Ok(())
}
