//! Retained physical sample clocks are authored state across durable resize history.
use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use std::collections::BTreeMap;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Silent pause",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}

fn fixture() -> Result<ProjectDocument> {
    let rate = FrameRate::new(30_000, 1001)?;
    let empty = ProjectDocument::new(
        ProjectId::new("ordinary-duration-clocks")?,
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let asset = AssetId::new("audio")?;
    let audio = SourceAudio {
        asset: asset.clone(),
        span: span(3100, 23_100),
    };
    let mut speech = BeatNode::sequence("Unbound Original sound", Vec::new());
    speech.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(2),
            edit_window: None,
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate)?,
            audio: Some(audio),
            link: LinkRelation::Independent,
            audio_offset: AudioSample(0),
        },
    };
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("pause"), node("speech")]),
        ),
        (node("lead"), hold(3)),
        (node("pause"), hold(2)),
        (node("speech"), speech),
    ]))?;
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset,
        AssetRecord {
            label: "Deterministic metadata fixture; no decoded media".into(),
            content_hash: "ab".repeat(32),
            video: None,
            audio: Some(span(0, 44_117)),
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    )]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn request(document: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

fn resize(duration: i64) -> Command {
    Command::SetHoldDuration {
        node: node("pause"),
        duration: frames(duration),
    }
}

#[test]
fn ordinary_hold_duration_clocks_survive_reopen_undo_redo_and_atomic_late_refusal() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("clocks.deadpan");
    let initial = fixture()?;
    assert!(initial.audio_bindings().bindings().is_empty());
    let mut store = ProjectStore::create(&package, &initial)?;
    store.commit(&request(&initial, "noop", resize(2)))?;
    let before = store.snapshot()?;
    assert_eq!(before.audio_bindings(), initial.audio_bindings());
    store.commit(&request(&before, "resize", resize(1)))?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 6);
    assert!(
        after
            .audio_bindings()
            .bindings()
            .contains_key(&node("speech")),
        "the surviving Source needs its old physical sample clock"
    );
    assert!(!after.audio_bindings().timings().is_empty());
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.audio_bindings(), before.audio_bindings());
    store.redo(undone.revision_id(), revision("redo"))?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), after.nodes());
    assert_eq!(redone.audio_bindings(), after.audio_bindings());
    let late = Command::Compound {
        transaction: ResolvedTransaction::new(
            0,
            BTreeMap::new(),
            vec![
                ResolvedStep::Edit {
                    edit: LeafEdit::new(revision("late-resize"), resize(4))?,
                },
                ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        revision("late-missing"),
                        Command::Rename {
                            node: node("missing"),
                            label: "Cannot resolve".into(),
                        },
                    )?,
                },
            ],
        )?,
    };
    assert!(store.commit(&request(&redone, "late-outer", late)).is_err());
    assert_eq!(store.snapshot()?, redone);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}

fn fixed_retime_fixture() -> Result<ProjectDocument> {
    let initial = fixture()?;
    let mut nodes = initial.nodes().clone();
    nodes.insert(
        node("root"),
        BeatNode::sequence("Root", vec![node("lead"), node("rate"), node("outside")]),
    );
    nodes.insert(
        node("body"),
        BeatNode::sequence("Fixed input", vec![node("pause"), node("speech")]),
    );
    let mut rate = BeatNode::sequence("Fixed output", Vec::new());
    rate.kind = NodeKind::Retime {
        child: node("body"),
        duration: frames(4),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4))?,
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    nodes.insert(node("rate"), rate);
    nodes.insert(node("outside"), initial.nodes()[&node("speech")].clone());
    let mut wire = serde_json::to_value(initial)?;
    wire["nodes"] = serde_json::to_value(nodes)?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

#[test]
fn fixed_retime_hold_duration_clocks_keep_atomic_history_and_outer_owners() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("fixed-retime.deadpan");
    let before = fixed_retime_fixture()?;
    assert_eq!(before.duration()?.frames(), 9);
    assert!(before.audio_bindings().bindings().is_empty());
    let mut store = ProjectStore::create(&package, &before)?;
    let edit = request(&before, "fixed-grow", resize(3));
    let preview = store.preview(&edit)?;
    assert_eq!(store.snapshot()?, before);
    let committed = store.commit(&edit)?.edit;
    assert_eq!(committed, preview);
    assert_eq!(committed.duration_delta, 0);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 9);
    assert_eq!(after.nodes()[&node("rate")], before.nodes()[&node("rate")]);
    assert_eq!(
        after.nodes()[&node("outside")],
        before.nodes()[&node("outside")]
    );
    assert!(
        after
            .audio_bindings()
            .bindings()
            .contains_key(&node("speech"))
    );
    assert!(
        after
            .audio_bindings()
            .bindings()
            .get(&node("outside"))
            .is_none_or(|binding| binding.reanchors.is_empty()),
        "a fixed Retime must not move the following outer owner"
    );
    assert_eq!(committed.inverse.apply(&after)?, before);
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("fixed-undo"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.audio_bindings(), before.audio_bindings());
    store.redo(undone.revision_id(), revision("fixed-redo"))?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), after.nodes());
    assert_eq!(redone.audio_bindings(), after.audio_bindings());
    let bank = store.registers()?;
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let counts = || -> Result<(i64, i64, i64)> {
        Ok(database.query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps)",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?)
    };
    let baseline = counts()?;
    // The child would become three frames, shorter than the fixed [0,4) window.
    assert!(
        store
            .commit(&request(&redone, "fixed-invalid-window", resize(1)))
            .is_err()
    );
    // Prove the changing first leaf admits before testing the later refusal.
    store.preview(&request(&redone, "fixed-staged-grow", resize(4)))?;
    let late = Command::Compound {
        transaction: ResolvedTransaction::new(
            bank.version,
            BTreeMap::new(),
            vec![
                ResolvedStep::Edit {
                    edit: LeafEdit::new(revision("fixed-staged-grow"), resize(4))?,
                },
                ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        revision("fixed-staged-missing"),
                        Command::Rename {
                            node: node("missing"),
                            label: "Cannot resolve".into(),
                        },
                    )?,
                },
            ],
        )?,
    };
    let error = store
        .commit(&request(&redone, "fixed-late-refusal", late))
        .expect_err("missing second node must refuse the complete transaction");
    assert_eq!(error.code(), "SelectionUnavailable");
    assert_eq!(store.snapshot()?, redone);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts()?, baseline);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
