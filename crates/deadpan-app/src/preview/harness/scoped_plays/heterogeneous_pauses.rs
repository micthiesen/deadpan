//! Actual keyboard continuation when queried owners resolve different plays.
use super::*;
use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    IterationId, IterationOrder, PlayOverride, PlayOverrides, ProjectId, RepeatEditBranch,
    RevisionId, ScopedNodeTarget, Subtree,
};
use deadpan_store::ProjectStore;
use std::collections::BTreeMap;

const PROJECT: &str = "ui-heterogeneous-pauses";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let previous = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No preceding replay workspace")?
        .path
        .clone();
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let package = scratch.path().join("heterogeneous-pauses.deadpan");
    fixture(&package)?;
    let result = exercise(d, &package);
    // The private package outlives its actual service writer, including a
    // failed assertion. Never delete a package still open by the service.
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the heterogeneous pause fixture", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        d.app()
            .service
            .submit(ProjectRequest::Open(previous.clone()))?;
        d.wait_for("Restore the preceding scoped replay workspace", |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path == previous)
                && !app.service.is_busy()
        })?;
        resize(d, 1280.0, 820.0)
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; fixture cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn exercise(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    d.app()
        .service
        .submit(ProjectRequest::Open(path.to_path_buf()))?;
    d.wait_for("Open the independent queried-owner pause fixture", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.project_id().as_str() == PROJECT)
            && !app.service.is_busy()
    })?;
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G])?;
    d.key(Key::Enter)?;
    d.command("scope plays 1,2")?;
    d.chord(&[Key::Num3, Key::L])?;
    idle(d)?;
    let before = d.app().scoped_target()?.ok_or("No queried Repeat owner")?;
    d.check(
        "Keyboard selection captures both inner Repeat owners at local frame three",
        before.target.node.as_str() == "inner"
            && before.target.repeats.len() == 1
            && before.also.len() == 1
            && before.also[0].node.as_str() == "owned-inner"
            && before.cursor == ProjectFrame(3)
            && document(d)?
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                == 12,
        json!({"owner":"inner Repeat","outer_plays":[1,2],"Edit":3,"frames":12}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.command("hold 3f video=black")?;
    d.changed(&revision)?;
    idle(d)?;
    let captured = d
        .app()
        .scoped_target()?
        .ok_or("Pause lost its captured target set")?;
    let after = document(d)?;
    let scopes = d
        .app()
        .scoped
        .as_ref()
        .ok_or("Pause lost its scoped inspector")?
        .scope_label(
            d.app()
                .workspace
                .as_ref()
                .ok_or("Missing pause workspace")?,
        )?;
    let [other] = captured.also.as_slice() else {
        return Err("Pause lost its second captured Hold".into());
    };
    let black_hold = |target: &ScopedNodeTarget| {
        matches!(after.nodes()[&target.node].kind,
        NodeKind::Hold { ref recipe } if recipe.duration.frames() == 3 && recipe.video == HoldVideo::Background && recipe.audio == HoldAudio::Silence)
    };
    d.check(
        "The saved pauses retain their independently inferred inner plays and truthful scope",
        after.duration().map_err(|error| error.to_string())?.frames() == 18
            && inner_play(after, &captured.target)? == 0 && inner_play(after, other)? == 1
            && black_hold(&captured.target) && black_hold(other)
            && captured.cursor == ProjectFrame(3)
            && scopes == "2 captured pauses · shown: outer · play 1/2 › inner · play 1/2",
        json!({"frames":18,"captured_pauses":2,"inferred_inner_plays":[1,2],"Edit":3,"scope":scopes}), d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    painted(
        d,
        "2 captured pauses · shown: outer · play 1/2 › inner · play 1/2",
    )?;
    painted(d, "Insert pause")?;
    scenarios::footer_anchored(
        d,
        "Independently captured pause scopes keep the compact footer",
    )?;
    d.capture(
        "Two captured pauses show the displayed play without claiming a shared inner choice",
    )?;
    Ok(())
}

fn inner_play(document: &ProjectDocument, target: &ScopedNodeTarget) -> Result<u32, String> {
    let step = target
        .repeats
        .get(1)
        .ok_or("Missing inferred inner Repeat")?;
    let RepeatEditBranch::Play { iteration } = &step.branch else {
        return Err("Inferred inner play is not concrete".into());
    };
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&step.repeat].kind else {
        return Err("Inferred owner is not a Repeat".into());
    };
    iterations
        .position(iteration)
        .ok_or("Inferred play retired".into())
}

fn fixture(path: &Path) -> Result<(), String> {
    let node = |name: &str| NodeId::new(name).map_err(|error| error.to_string());
    let revision = |name: &str| RevisionId::new(name).map_err(|error| error.to_string());
    let document = ProjectDocument::new_automatic(
        ProjectId::new(PROJECT).map_err(|error| error.to_string())?,
        revision("initial")?,
        node("root")?,
    )
    .map_err(|error| error.to_string())?;
    let repeat = |name: &str, child: &str, allocation: &str| -> Result<BeatNode, String> {
        let mut beat = BeatNode::sequence(name, Vec::new());
        beat.kind = NodeKind::Repeat {
            child: node(child)?,
            iterations: IterationOrder::new(revision(allocation)?, 2)
                .map_err(|error| error.to_string())?,
            gap: None,
            escalation: None,
        };
        Ok(beat)
    };
    let hold = |duration| -> Result<HoldRecipe, String> {
        Ok(HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(duration).map_err(|error| error.to_string())?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        })
    };
    let mut store = ProjectStore::create(path, &document).map_err(|error| error.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("fixture")?,
            command: Command::Insert {
                parent: node("root")?,
                index: 0,
                subtree: Subtree {
                    root: node("outer")?,
                    nodes: BTreeMap::from([
                        (node("outer")?, repeat("outer", "inner", "outer")?),
                        (node("inner")?, repeat("inner", "leaf", "inner")?),
                        (node("leaf")?, BeatNode::hold("leaf", hold(4)?)),
                        (
                            node("owned-inner")?,
                            repeat("owned-inner", "owned-leaf", "inner")?,
                        ),
                        (node("owned-leaf")?, BeatNode::hold("owned-leaf", hold(2)?)),
                    ]),
                    overrides: BTreeMap::from([(
                        node("outer")?,
                        PlayOverrides::try_from(vec![PlayOverride {
                            iteration: IterationId {
                                allocation: revision("outer")?,
                                ordinal: 1,
                            },
                            root: node("owned-inner")?,
                        }])
                        .map_err(|error| error.to_string())?,
                    )]),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}
