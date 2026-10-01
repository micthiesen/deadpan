//! Durability of an already rebased Source and its retained clocks. This does
//! not qualify a public Trim operation, source decoding, or rendered PCM.

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

fn fixture() -> Result<(ProjectDocument, FrozenAudioLayout)> {
    let rate = FrameRate::new(30_000, 1001)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000)?,
        },
        SourceTimestamp {
            ticks: 6406,
            time_base: SourceTimeBase::new(1, 48_000)?,
        },
    )?;
    let asset = AssetId::new("audio")?;
    let mut source = SourceNode {
        edit_window: None,
        duration: FrameDuration::new(4)?,
        video: SourceVideo::Blank,
        video_mapping: SourceVideoMapping::FitBeat,
        audio: Some(SourceAudio {
            asset: asset.clone(),
            span,
        }),
        audio_mapping: SourceAudioMapping::natural_rate(span, rate)?,
        audio_offset: AudioSample(7),
        link: LinkRelation::Independent,
    };
    let source_node = |source| BeatNode {
        label: "Retained source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source { source },
    };
    let empty = ProjectDocument::new(
        ProjectId::new("source-origin-history")?,
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("source")]),
        ),
        (
            node("lead"),
            BeatNode::hold(
                "Lead",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(1)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
        (node("source"), source_node(source.clone())),
    ]))?;
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset,
        AssetRecord {
            label: "Synthetic audio metadata".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))?;
    let original = ProjectDocument::from_json(&wire.to_string())?;
    let frozen = FrozenAudioLayout::capture(&original)?;
    let timing = AudioTimingId {
        allocation: revision("captured-before-prefix"),
        ordinal: 0,
    };
    let captured = capture_unbound_audio_bindings(&original, timing.clone())?;
    let mut binding = captured.bindings()[&node("source")].clone();
    binding.resume = Some(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ExactRatio::new(64 * 5, 8008)?,
            terms: vec![AudioPhaseTerm {
                placement: binding.lattice.clone(),
                from_local: ExactRatio::ZERO,
                to_local: ExactRatio::ONE,
            }],
        },
    });
    binding.reanchors.push(AudioReanchorStep {
        anchor: Default::default(),
        placement: binding.lattice.clone(),
        window: Some(ExactFrameRange::new(
            ExactRatio::ONE,
            ExactRatio::integer(5),
        )?),
    });
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing,
            layout: frozen.clone(),
        }],
        BTreeMap::from([(node("source"), binding.rebase_local(ExactRatio::ONE)?)]),
    )?;

    // Existing material moves one physical frame later. The unity Partition
    // keeps the visible duration and the captured layout in their old clocks.
    let extent = source.audio_mapping.duration_frames(source.duration)?;
    source.duration = FrameDuration::new(5)?;
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ONE,
        frames: extent,
        selection: ExactFrameRange::new(ExactRatio::ONE, ExactRatio::ONE.checked_add(extent)?)?,
    };
    wire["nodes"]["source"] = serde_json::to_value(source_node(source))?;
    wire["nodes"]["window"] = serde_json::to_value(BeatNode {
        label: "Retained physical window".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: node("source"),
            duration: FrameDuration::new(4)?,
            mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(5))?,
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    })?;
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Root",
        vec![node("lead"), node("window")],
    ))?;
    wire["audio_bindings"] = serde_json::to_value(bindings)?;
    Ok((ProjectDocument::from_json(&wire.to_string())?, frozen))
}

fn assert_rebased(document: &ProjectDocument, frozen: &FrozenAudioLayout) -> Result {
    document.validate()?;
    let bindings = document.audio_bindings();
    assert_eq!(bindings.bindings().len(), 1);
    assert_eq!(bindings.timings().len(), 1);
    let retained = bindings.timings().values().next().unwrap();
    assert_eq!(retained, frozen);
    assert_eq!(serde_json::to_vec(retained)?, serde_json::to_vec(frozen)?);
    let binding = &bindings.bindings()[&node("source")];
    assert_eq!(
        binding.lattice.reference_local_offset,
        ExactRatio::integer(-1)
    );
    let resume = binding.resume.as_ref().unwrap();
    assert_eq!(resume.local_boundary, ExactRatio::ONE);
    assert_eq!(resume.phase.constant, ExactRatio::new(64 * 5, 8008)?);
    assert_eq!(resume.phase.terms.len(), 1);
    let term = &resume.phase.terms[0];
    assert_eq!(
        term.placement.reference_local_offset,
        ExactRatio::integer(-1)
    );
    assert_eq!(term.from_local, ExactRatio::ONE);
    assert_eq!(term.to_local, ExactRatio::integer(2));
    assert_eq!(binding.reanchors.len(), 1);
    assert_eq!(
        binding.reanchors[0].placement.reference_local_offset,
        ExactRatio::integer(-1)
    );
    assert_eq!(
        binding.reanchors[0].window,
        Some(ExactFrameRange::new(
            ExactRatio::ONE,
            ExactRatio::integer(5)
        )?)
    );
    Ok(())
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    actual.validate()?;
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn rebased_source_offsets_and_frozen_layout_survive_durable_undo_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("source-origin.deadpan");
    let (initial, frozen) = fixture()?;
    let store = ProjectStore::create(&path, &initial)?;
    assert_eq!(store.snapshot()?, initial);
    assert_rebased(&store.snapshot()?, &frozen)?;
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, initial);
    assert_rebased(&store.snapshot()?, &frozen)?;
    let saved = store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("deleted"),
        command: Command::Delete {
            node: node("window"),
        },
    })?;
    let deleted = store.snapshot()?;
    assert_eq!(deleted.revision_id(), &revision("deleted"));
    assert!(!deleted.nodes().contains_key(&node("source")));
    assert!(deleted.audio_bindings().is_empty());
    deleted.validate()?;
    let forward = saved.edit.forward.audio_bindings.as_ref().unwrap();
    let inverse = saved.edit.inverse.audio_bindings.as_ref().unwrap();
    assert_eq!(forward.before.as_ref(), Some(initial.audio_bindings()));
    assert_eq!(forward.after.as_ref(), Some(deleted.audio_bindings()));
    assert_eq!(inverse.before, forward.after);
    assert_eq!(inverse.after, forward.before);
    store.validate()?;
    drop(store);

    // Inspect the serialized transaction, not only the in-memory commit result.
    let connection = Connection::open(path.join("project.sqlite"))?;
    let encoded: String = connection.query_row(
        "SELECT edit FROM history WHERE revision_id='deleted'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&encoded)?,
        saved.edit
    );
    drop(connection);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, deleted);
    store.undo(&revision("deleted"), revision("undone"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.revision_id(), &revision("undone"));
    assert_ne!(undone.revision_id(), initial.revision_id());
    assert_authored(&undone, &initial)?;
    assert_rebased(&undone, &frozen)?;
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    assert_rebased(&store.snapshot()?, &frozen)?;
    store.redo(&revision("undone"), revision("redone"))?;
    let redone = store.snapshot()?;
    assert_eq!(redone.revision_id(), &revision("redone"));
    assert_ne!(redone.revision_id(), deleted.revision_id());
    assert_authored(&redone, &deleted)?;
    assert!(redone.audio_bindings().is_empty());
    store.validate()?;
    drop(store);

    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, redone);
    assert_eq!(store.snapshot_at(initial.revision_id())?, initial);
    assert_eq!(store.snapshot_at(undone.revision_id())?, undone);
    assert_rebased(&store.snapshot_at(undone.revision_id())?, &frozen)?;
    store.validate()?;
    Ok(())
}
