//! Persistence of already retained effect clocks through ordinary effect edits.
//! This fixture does not author Trim or qualify media/rendered output.

use std::{collections::BTreeMap, error::Error};

use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn fixture() -> Result<ProjectDocument> {
    let framing = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(2, 1))?,
        FramingCurve::Linear,
    )?
    .prepend_owner_frames(duration(3), duration(10))?;
    let gain = AudioTreatments::from_clip_gain(ClipGain::new(
        GainDb::new(3000)?,
        false,
        vec![GainEnvelope::new(
            GainClock::OwnerOutput,
            GainRange::new(ratio(1, 1), ratio(5, 1))?,
            GainDb::new(-6000)?,
            vec![GainSegment::new(
                ratio(5, 1),
                GainDb::new(6000)?,
                GainCurve::Linear,
            )?],
        )?],
        vec![GainRange::new(ratio(2, 1), ratio(3, 1))?],
    )?)
    .with_owner_prefix(duration(3))?;
    let asset = AssetId::new("audio")?;
    let time_base = SourceTimeBase::new(1, 48_000)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )?;
    let initial = ProjectDocument::new(
        ProjectId::new("source-effects-history")?,
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(initial)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("window")]),
        ),
        (
            node("window"),
            BeatNode {
                label: "Original body".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: node("source"),
                    duration: duration(10),
                    mapping: FrameRange::new(ProjectFrame(3), ProjectFrame(13))?,
                    pitch: PitchPolicy::FollowSpeed,
                    purpose: RetimePurpose::Partition,
                },
                cutaways: Vec::new(),
            },
        ),
        (
            node("source"),
            BeatNode {
                label: "Retained effects".into(),
                framing: Some(framing),
                audio_treatments: gain,
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Source {
                    source: SourceNode {
                        edit_window: None,
                        duration: duration(15),
                        video: SourceVideo::Blank,
                        video_mapping: SourceVideoMapping::FitBeat,
                        audio: Some(SourceAudio {
                            asset: asset.clone(),
                            span,
                        }),
                        audio_mapping: SourceAudioMapping::SelectedPlacement {
                            start: ratio(3, 1),
                            frames: ratio(10, 1),
                            selection: ExactFrameRange::new(ratio(3, 1), ratio(13, 1))?,
                        },
                        audio_offset: AudioSample(0),
                        link: LinkRelation::Independent,
                    },
                },
                cutaways: Vec::new(),
            },
        ),
    ]))?;
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset,
        AssetRecord {
            label: "Synthetic metadata".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn assert_effects(document: &ProjectDocument, target_scale: i64, updated_gain: bool) -> Result {
    document.validate()?;
    let source = &document.nodes()[&node("source")];
    let framing = source.framing.as_ref().unwrap();
    assert_eq!(
        framing.clock,
        FramingClock::RetainedOutput {
            offset: ratio(-3, 1),
            duration: ratio(10, 1),
        }
    );
    // These exact values distinguish the retained ten-frame path from the
    // current fifteen-frame physical duration, including both extended ends.
    for (at, expected) in [
        (0, ratio(1, 1)),
        (3, ratio(1, 1)),
        (8, ratio(i128::from(1 + target_scale), 2)),
        (13, ratio(i128::from(target_scale), 1)),
        (15, ratio(i128::from(target_scale), 1)),
    ] {
        assert_eq!(
            framing.evaluate(ratio(at, 1), duration(15))?.scale,
            expected
        );
    }
    let clip = source.audio_treatments.clip_gain().unwrap();
    assert_eq!(
        clip.trim(),
        GainDb::new(if updated_gain { -3000 } else { 3000 })?
    );
    let retained = &clip.envelopes()[0];
    assert_eq!(retained.clock(), GainClock::OwnerOutput);
    assert_eq!(retained.range(), GainRange::new(ratio(4, 1), ratio(8, 1))?);
    assert_eq!(retained.segments()[0].end(), ratio(8, 1));
    assert_eq!(retained.segments()[0].curve(), GainCurve::Linear);
    assert_eq!(
        clip.mute_ranges(),
        &[GainRange::new(ratio(5, 1), ratio(6, 1))?]
    );
    assert_eq!(clip.envelopes().len(), if updated_gain { 2 } else { 1 });
    for (at, initial_db, edited_db, muted) in [
        (2, 3000, -9000, false),
        (3, 3000, -3000, false),
        (4, -3000, -9000, false),
        (5, 0, -6000, true),
        (6, 3000, -3000, false),
        (8, 3000, -3000, false),
    ] {
        let evaluated = source.audio_treatments.evaluate(ratio(at, 1))?;
        assert_eq!(
            evaluated.millidecibels,
            ratio(if updated_gain { edited_db } else { initial_db }, 1)
        );
        assert_eq!(evaluated.muted, muted);
    }
    Ok(())
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn retained_framing_and_translated_gain_survive_edit_history_and_restart() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("source-effects.deadpan");
    let initial = fixture()?;
    let store = ProjectStore::create(&path, &initial)?;
    assert_eq!(store.snapshot()?, initial);
    assert_effects(&store.snapshot()?, 2, false)?;
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, initial);
    assert_effects(&store.snapshot()?, 2, false)?;
    let mut framing = initial.nodes()[&node("source")].framing.clone().unwrap();
    let FramingValue::Envelope { envelope } = &mut framing.value else {
        unreachable!()
    };
    envelope.segments[0].pose.scale = ratio(3, 1);
    let framing_request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("camera"),
        command: Command::SetFraming {
            node: node("source"),
            framing: Some(framing),
        },
    };
    let camera_edit = store.commit(&framing_request)?.edit;
    let camera = store.snapshot()?;
    assert_eq!(camera.revision_id(), &revision("camera"));
    assert_effects(&camera, 3, false)?;
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, camera);
    assert_effects(&store.snapshot()?, 3, false)?;
    let clip = camera.nodes()[&node("source")]
        .audio_treatments
        .clip_gain()
        .unwrap();
    let mut envelopes = clip.envelopes().to_vec();
    envelopes.push(GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ratio(3, 1))?,
        GainDb::new(-6000)?,
        vec![GainSegment::new(
            ratio(3, 1),
            GainDb::new(-6000)?,
            GainCurve::Step,
        )?],
    )?);
    let gain_request = CommandRequest {
        project_id: camera.project_id().clone(),
        expected_revision: camera.revision_id().clone(),
        new_revision: revision("gain"),
        command: Command::SetAudioTreatments {
            node: node("source"),
            treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                GainDb::new(-3000)?,
                false,
                envelopes,
                clip.mute_ranges().to_vec(),
            )?),
        },
    };
    let gain_edit = store.commit(&gain_request)?.edit;
    let gained = store.snapshot()?;
    assert_eq!(gained.revision_id(), &revision("gain"));
    assert_effects(&gained, 3, true)?;
    store.validate()?;
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    let mut statement = connection.prepare("SELECT request, edit FROM history ORDER BY id")?;
    let encoded = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(encoded.len(), 2);
    for ((request, edit), (expected_request, expected_edit, before, after)) in encoded.iter().zip([
        (&framing_request, &camera_edit, &initial, &camera),
        (&gain_request, &gain_edit, &camera, &gained),
    ]) {
        assert_eq!(
            &serde_json::from_str::<CommandRequest>(request)?,
            expected_request
        );
        let retained: EditTransaction = serde_json::from_str(edit)?;
        assert_eq!(&retained, expected_edit);
        assert_eq!(retained.forward.apply(before)?, *after);
        assert_eq!(retained.inverse.apply(after)?, *before);
    }
    drop(statement);
    drop(connection);

    for (from, next, redo, expected, scale, gain) in [
        ("gain", "undo-gain", false, &camera, 3, false),
        ("undo-gain", "undo-camera", false, &initial, 2, false),
        ("undo-camera", "redo-camera", true, &camera, 3, false),
        ("redo-camera", "redo-gain", true, &gained, 3, true),
    ] {
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?.revision_id(), &revision(from));
        if redo {
            store.redo(&revision(from), revision(next))?;
        } else {
            store.undo(&revision(from), revision(next))?;
        }
        let actual = store.snapshot()?;
        assert_eq!(actual.revision_id(), &revision(next));
        assert_ne!(actual.revision_id(), expected.revision_id());
        assert_authored(&actual, expected)?;
        assert_effects(&actual, scale, gain)?;
        store.validate()?;
    }
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?.revision_id(), &revision("redo-gain"));
    for (expected, scale, gain) in [
        (&initial, 2, false),
        (&camera, 3, false),
        (&gained, 3, true),
    ] {
        let historical = store.snapshot_at(expected.revision_id())?;
        assert_eq!(&historical, expected);
        assert_effects(&historical, scale, gain)?;
    }
    let historical_undo = store.snapshot_at(&revision("undo-camera"))?;
    assert_authored(&historical_undo, &initial)?;
    assert_effects(&historical_undo, 2, false)?;
    store.validate()?;
    Ok(())
}
