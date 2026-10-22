//! Unsupported clocks and sound policies reject through the actual command UI.
use super::*;
use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, FrameRange, HoldAudio, IterationOrder,
    PitchPolicy, ProjectId, RetimePurpose, RevisionId, Subtree,
};
use deadpan_store::ProjectStore;
use std::collections::BTreeMap;

const PROJECT: &str = "ui-scoped-duration-refusals";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let previous = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No duration workspace")?
        .path
        .clone();
    let original = d.app().source_cursor;
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = scratch.path().join("duration-refusals.deadpan");
    fixture(&path)?;
    let sound_path = scratch.path().join("duration-sounds.deadpan");
    sound_fixture(&previous, &sound_path)?;
    let result = exercise(d, &path).and_then(|_| sound_case(d, &sound_path));
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private duration refusal writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        d.app()
            .service
            .submit(ProjectRequest::Open(previous.clone()))?;
        d.wait_for("Restore the qualified Hold-duration workspace", |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path == previous)
                && !app.service.is_busy()
        })?;
        d.command("source")?;
        d.chord(&[Key::G, Key::G])?;
        if original > 0 {
            let mut keys = original
                .to_string()
                .chars()
                .map(|digit| {
                    Key::from_name(&digit.to_string())
                        .ok_or("Invalid Original frame digit".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            keys.push(Key::L);
            d.chord(&keys)?;
        }
        d.command("sequence")?;
        d.chord(&[Key::G, Key::G])?;
        settled(d)
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; refusal fixture cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn exercise(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    d.app()
        .service
        .submit(ProjectRequest::Open(path.to_path_buf()))?;
    d.wait_for("Open actual unsupported duration definitions", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.project_id().as_str() == PROJECT)
            && !app.service.is_busy()
    })?;
    d.command("sequence")?;
    super::super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Enter, Key::Enter])?;
    let all = target(d)?;
    d.check(
        "All-plays duration fixture selects a real nested Hold definition",
        all.target.repeats.len() == 2
            && all
                .target
                .repeats
                .iter()
                .all(|step| matches!(step.branch, deadpan_core::RepeatEditBranch::Default)),
        json!("two explicit All plays choices"),
        d.snapshot(),
    )?;
    let before = document(d)?.clone();
    d.key(Key::Enter)?;
    d.check(
        "All-plays duration now offers the authored Hold length without writing",
        d.app().command_open && d.app().command == "hold-duration 4f" && document(d)? == &before,
        json!({"command":"hold-duration 4f","document":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::Enter])?;
    d.command("scope play 1")?;
    d.key(Key::Enter)?;
    let mixed = target(d)?;
    d.check(
        "Mixed duration fixture retains distinct outer Play and inner Default authority",
        mixed.target.repeats.len() == 2
            && matches!(
                mixed.target.repeats[0].branch,
                deadpan_core::RepeatEditBranch::Play { .. }
            )
            && matches!(
                mixed.target.repeats[1].branch,
                deadpan_core::RepeatEditBranch::Default
            ),
        json!("This play / All plays"),
        d.snapshot(),
    )?;
    let before = document(d)?.clone();
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Mixed duration now offers the authored Hold length without isolation or writes",
        d.app().command_open
            && d.app().command == "hold-duration 4f"
            && d.revision() == revision
            && document(d)? == &before,
        json!({"command":"hold-duration 4f","document":"unchanged","history":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::J, Key::Enter])?;
    d.command("scope plays 1,2")?;
    d.check(
        "Several-play duration fixture captures both concrete Hold targets",
        target(d)?.also.len() == 1,
        json!("two selected Holds"),
        d.snapshot(),
    )?;
    let before = document(d)?.clone();
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Several-play duration offers the authored Hold length without isolation or writes",
        d.app().command_open
            && d.app().command == "hold-duration 4f"
            && d.revision() == revision
            && document(d)? == &before,
        json!({"command":"hold-duration 4f","document":"unchanged","history":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::J, Key::J, Key::Enter])?;
    d.command("scope play 1")?;
    d.key(Key::Enter)?;
    let retimed = target(d)?;
    d.check(
        "This-play duration fixture selects a Hold under a real Retime clock",
        retimed.target.repeats.len() == 1
            && retimed.has_single_play_authority()
            && retimed.target.node.as_str() == "retimed-hold"
            && matches!(
                document(d)?.nodes()[&NodeId::new("speed").map_err(|e| e.to_string())?].kind,
                NodeKind::Retime { .. }
            )
            && matches!(
                document(d)?.nodes()[&retimed.root].kind,
                NodeKind::Repeat { .. }
            ),
        json!("This play with Retime ancestor"),
        d.snapshot(),
    )?;
    refuse(d, "This-play retimed Hold duration")?;
    Ok(())
}

fn refuse(d: &mut Driver<'_>, name: &str) -> Result<(), String> {
    let before = document(d)?.clone();
    let captured = target(d)?;
    let bank = d.app().copied.bank_version();
    d.command("hold-duration 7f")?;
    settled(d)?;
    d.check(
        &format!("{name} refuses without authored, cursor, target or bank changes"),
        document(d)? == &before
            && target(d)? == captured
            && d.app().copied.bank_version() == bank
            && d.app()
                .error
                .as_ref()
                .is_some_and(|error| !error.is_empty()),
        json!({"document":"unchanged","target":"unchanged","bank":"unchanged","error":"explicit"}),
        d.snapshot(),
    )
}

fn fixture(path: &Path) -> Result<(), String> {
    let node = |name: &str| NodeId::new(name).map_err(|e| e.to_string());
    let revision = |name: &str| RevisionId::new(name).map_err(|e| e.to_string());
    let duration = |value| FrameDuration::new(value).map_err(|e| e.to_string());
    let repeat = |name: &str, child: &str| -> Result<BeatNode, String> {
        let mut beat = BeatNode::sequence(name, Vec::new());
        beat.kind = NodeKind::Repeat {
            child: node(child)?,
            iterations: IterationOrder::new(revision(name)?, 2).map_err(|e| e.to_string())?,
            gap: None,
            escalation: None,
        };
        Ok(beat)
    };
    let recipe = |audio| -> Result<HoldRecipe, String> {
        Ok(HoldRecipe {
            duration: duration(4)?,
            video: HoldVideo::Background,
            audio,
            picture_context: None,
        })
    };
    let mut speed = BeatNode::sequence("Retime", Vec::new());
    speed.kind = NodeKind::Retime {
        child: node("retimed-hold")?,
        duration: duration(8)?,
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).map_err(|e| e.to_string())?,
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    let initial = ProjectDocument::new_automatic(
        ProjectId::new(PROJECT).map_err(|e| e.to_string())?,
        revision("initial")?,
        node("root")?,
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::create(path, &initial).map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: revision("inserted")?,
            command: Command::Insert {
                parent: node("root")?,
                index: 0,
                subtree: Subtree {
                    root: node("group")?,
                    nodes: BTreeMap::from([
                        (
                            node("group")?,
                            BeatNode::sequence(
                                "Fixture",
                                vec![node("outer")?, node("many")?, node("retimed-repeat")?],
                            ),
                        ),
                        (node("outer")?, repeat("outer", "inner")?),
                        (node("inner")?, repeat("inner", "silent")?),
                        (
                            node("silent")?,
                            BeatNode::hold("Silent", recipe(HoldAudio::Silence)?),
                        ),
                        (node("many")?, repeat("many", "many-hold")?),
                        (
                            node("many-hold")?,
                            BeatNode::hold("Many", recipe(HoldAudio::Silence)?),
                        ),
                        (node("speed")?, speed),
                        (node("retimed-repeat")?, repeat("retimed-repeat", "speed")?),
                        (
                            node("retimed-hold")?,
                            BeatNode::hold("Retimed", recipe(HoldAudio::Silence)?),
                        ),
                    ]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .map_err(|e| e.to_string())?;
    let inserted = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: inserted.project_id().clone(),
            expected_revision: inserted.revision_id().clone(),
            new_revision: revision("ungrouped")?,
            command: Command::Ungroup {
                node: node("group")?,
            },
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn sound_case(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    let expected_path = path.canonicalize().map_err(|error| error.to_string())?;
    d.app()
        .service
        .submit(ProjectRequest::Open(path.to_path_buf()))?;
    d.wait_for(
        "Open a real qualified project with a root sound interval",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path == expected_path)
                && !app.service.is_busy()
        },
    )?;
    d.command("sequence")?;
    super::super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Enter])?;
    d.command("scope play 1")?;
    settled(d)?;
    d.check(
        "Sound-policy duration fixture has qualified media and a persisted root sound",
        document(d)?.sounds().len() == 1
            && target(d)?.has_single_play_authority()
            && hold(d)?.audio == HoldAudio::Silence
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| !workspace.sources.is_empty()),
        json!({"Hold":"silent","root_sounds":1,"sources":"qualified"}),
        d.snapshot(),
    )?;
    refuse(d, "Root sound-interval Hold duration")
}

fn sound_fixture(source: &Path, path: &Path) -> Result<(), String> {
    use deadpan_core::{
        AudioEdgePolicy, AudioSample, AudioTreatments, ExactRatio, SoundEvent, SoundId,
        SoundOverflowPolicy, SourceAudioMapping,
    };
    use deadpan_store::AccessMode;
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|error| error.to_string())?;
    let mut store =
        ProjectStore::open(path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let before = store.snapshot().map_err(|error| error.to_string())?;
    let durations = before.durations().map_err(|error| error.to_string())?;
    let (audio, frames) = before
        .nodes()
        .iter()
        .find_map(|(id, node)| match &node.kind {
            NodeKind::Source { source } => source
                .audio
                .clone()
                .map(|audio| (audio, durations[id].frames())),
            _ => None,
        })
        .ok_or("The qualified fixture has no Original audio")?;
    let node = |name: &str| NodeId::new(name).map_err(|error| error.to_string());
    let revision = |name: &str| RevisionId::new(name).map_err(|error| error.to_string());
    let mut repeated = BeatNode::sequence("Sound-policy Repeat", Vec::new());
    repeated.kind = NodeKind::Repeat {
        child: node("sound-policy-hold")?,
        iterations: IterationOrder::new(revision("sound-policy-plays")?, 2)
            .map_err(|error| error.to_string())?,
        gap: None,
        escalation: None,
    };
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("sound-policy-insert")?,
            command: Command::Insert {
                parent: before.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: node("sound-policy-repeat")?,
                    nodes: BTreeMap::from([
                        (node("sound-policy-repeat")?, repeated),
                        (
                            node("sound-policy-hold")?,
                            BeatNode::hold(
                                "Silent sound-policy Hold",
                                HoldRecipe {
                                    duration: FrameDuration::new(4)
                                        .map_err(|error| error.to_string())?,
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                    picture_context: None,
                                },
                            ),
                        ),
                    ]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .map_err(|error| error.to_string())?;
    let inserted = store.snapshot().map_err(|error| error.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: inserted.project_id().clone(),
            expected_revision: inserted.revision_id().clone(),
            new_revision: revision("root-sound-policy")?,
            command: Command::SetSound {
                id: SoundId::new("duration-root-sound").map_err(|error| error.to_string())?,
                event: SoundEvent {
                    owner: inserted.root().clone(),
                    label: "Qualified Original sound interval".into(),
                    source: audio,
                    mapping: SourceAudioMapping::Duration {
                        frames: ExactRatio::integer(frames),
                    },
                    offset: AudioSample(0),
                    treatments: AudioTreatments::default(),
                    start_edge: AudioEdgePolicy::Hard,
                    end_edge: AudioEdgePolicy::Hard,
                    overflow: SoundOverflowPolicy::Reject,
                },
            },
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}
