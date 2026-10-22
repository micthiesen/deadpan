//! Native selected-play editing of two distinct owned gaps and a dormant final gap.

use super::mixed_hold_duration::{equivalent, open, recipe, scroll_top, settled, submit, target};
use super::shared_hold_duration::fixture;
use super::*;
use deadpan_core::{
    AudioTimingId, BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe,
    HoldVideo, IterationId, IterationOrder, PlayOverride, PlayOverrides, RepeatEditBranch,
    RevisionId, SourceAudio, SourceSpan, SourceTimestamp, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use std::{collections::BTreeMap, path::Path};

fn id(name: &str) -> Result<NodeId, String> {
    fixture::id(name)
}

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d.app().workspace.as_ref().ok_or("No owned-gap workspace")?;
    let previous = workspace.path.clone();
    let baseline = workspace.document.clone();
    let original = d.app().source_cursor;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|e| e.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("No Original picture")?;
    let source = workspace
        .sources
        .get(asset)
        .and_then(|s| s.video_index.as_ref())
        .ok_or("No measured Original index")?;
    let measured = picture
        .picture
        .select_source_frame(source)
        .map_err(|e| e.to_string())?;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: SourceTimestamp {
            ticks: measured.pts,
            time_base: source.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let live = scratch.path().join("owned-gap-duration.deadpan");
    create_fixture(&previous, &live, freeze)?;
    let result = (|| {
        open(d, &live, original)?;
        let before = document(d)?.clone();
        resize(d, 960.0, 640.0)?;
        outer_play_gap_case(d, &before)?;
        navigate_selected_gap_pair(d, "1,2", false)?;
        let entry = target(d)?;
        d.check(
            "Selected owned gaps retain distinct live branches and the true primary picture",
            entry.also.len() == 1
                && entry.target.repeats.last().is_some_and(|step| {
                    step.branch
                        == (RepeatEditBranch::Play {
                            iteration: iteration(&before, "inner", 0).unwrap(),
                        })
                })
                && entry.target.node == id("gap-zero")?
                && entry.also[0].node == id("gap-one")?
                && entry.presentation.is_some(),
            json!({"selected_gap_branches":2,"final_gap":"not selected"}),
            d.snapshot(),
        )?;
        d.capture("Two live owned gap Holds selected together at minimum size")?;
        let cursor = d.app().sequence_cursor;
        let revision = d.revision();
        d.key(Key::Enter)?;
        if d.app().command != "hold-duration 3f" || d.revision() != revision {
            return Err(
                "Owned-gap duration did not prefill the primary three-frame definition".into(),
            );
        }
        submit(d, "hold-duration 6f")?;
        d.changed(&revision)?;
        settled(d)?;
        let after = document(d)?.clone();
        let mapped = target(d)?;
        d.check("One native transaction grows both selected owned gaps and preserves the final definition",
            mapped.also.len() == 1
                && recipe(&after, &mapped.target.node)?.duration.frames() == 6
                && recipe(&after, &mapped.also[0].node)?.duration.frames() == 6
                && recipe(&after, &id("gap-final")?)?.duration.frames() == 5
                && d.app().sequence_cursor == cursor
                && d.app().sequence_length() == 60,
            json!({"selected_gaps_frames":6,"final_dormant_gap_frames":5,"cursor":cursor,"output_frames":60}), d.snapshot())?;

        // Reselect after the receipt: browsing invalidates the mapped counterpart
        // set, so this explicit scope establishes fresh authority for plays 2/3.
        d.command("scope plays 2,3")?;
        d.key(Key::J)?;
        settled(d)?;
        let final_pair = target(d)?;
        d.check(
            "Play two remains live while its selected counterpart is the final dormant gap",
            final_pair.also.len() == 1
                && final_pair.target.node == id("gap-one")?
                && final_pair.also[0].node == id("gap-final")?
                && final_pair.presentation.is_some(),
            json!({"primary":"live play 2 gap","counterpart":"dormant final play 3 gap"}),
            d.snapshot(),
        )?;
        let final_cursor = d.app().sequence_cursor;
        let revision = d.revision();
        d.key(Key::Enter)?;
        if d.app().command != "hold-duration 6f" {
            return Err(
                "Second owned-gap duration did not prefill the live six-frame definition".into(),
            );
        }
        submit(d, "hold-duration 7f")?;
        d.changed(&revision)?;
        settled(d)?;
        let latest = document(d)?.clone();
        let mapped = target(d)?;
        d.check("The dormant final gap stays an explicit duration target beside a live primary",
            mapped.also.len() == 1
                && recipe(&latest, &mapped.target.node)?.duration.frames() == 7
                && recipe(&latest, &mapped.also[0].node)?.duration.frames() == 7
                && d.app().sequence_cursor == final_cursor
                && i64::try_from(final_cursor).ok() == Some(final_pair.cursor.0)
                && d.app().sequence_length() == 62,
            json!({"live_and_dormant_definitions_frames":7,"cursor":final_cursor,"output_frames":62}), d.snapshot())?;
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
        settled(d)?;
        if !equivalent(document(d)?, &after) {
            return Err("Undo did not restore the first selected-gap transaction".into());
        }
        let revision = d.revision();
        d.key_modified(Key::R, egui::Modifiers::CTRL)?;
        d.changed(&revision)?;
        settled(d)?;
        if !equivalent(document(d)?, &latest) {
            return Err("Redo did not restore both live/dormant gap definitions".into());
        }
        Ok(())
    })();
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private owned-gap writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        d.app()
            .service
            .submit(ProjectRequest::Open(previous.clone()))?;
        d.wait_for("Restore the main project after owned-gap replay", |app| {
            app.workspace.as_ref().is_some_and(|w| w.path == previous) && !app.service.is_busy()
        })?;
        if !equivalent(document(d)?, &baseline) {
            return Err("Main project changed during owned-gap replay".into());
        }
        Ok(())
    })();
    match (result, cleanup) {
        (Err(a), Err(b)) => Err(format!("{a}; owned-gap cleanup: {b}")),
        (Err(a), _) | (_, Err(a)) => Err(a),
        _ => Ok(()),
    }
}

fn outer_play_gap_case(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    navigate_selected_gap_pair(d, "1,2", true)?;
    let entry = target(d)?;
    d.check(
        "Outer Play 2 and inner selected plays address both real owned gaps",
        entry.also.len() == 1
            && entry.target.node == id("gap-zero")?
            && entry.also[0].node == id("gap-one")?
            && entry.target.repeats.len() == 2
            && entry.target.repeats[0].branch
                == (RepeatEditBranch::Play {
                    iteration: iteration(baseline, "outer", 1)?,
                })
            && entry.target.repeats[1].branch
                == (RepeatEditBranch::Play {
                    iteration: iteration(baseline, "inner", 0)?,
                }),
        json!({"outer":"Play 2","inner":"selected plays 1,2","owned_gaps":2}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    if d.app().command != "hold-duration 3f" {
        return Err("Outer-play owned gap did not prefill three frames".into());
    }
    submit(d, "hold-duration 6f")?;
    d.changed(&revision)?;
    settled(d)?;
    let after = document(d)?.clone();
    let mapped = target(d)?;
    d.check("Outer Play isolation resizes both mapped gaps and leaves the other outer play untouched",
        mapped.also.len() == 1
            && recipe(&after, &mapped.target.node)?.duration.frames() == 6
            && recipe(&after, &mapped.also[0].node)?.duration.frames() == 6
            && recipe(baseline, &id("gap-zero")?)?.duration.frames() == 3
            && recipe(baseline, &id("gap-one")?)?.duration.frames() == 4
            && recipe(&after, &id("gap-zero")?)?.duration.frames() == 3
            && recipe(&after, &id("gap-one")?)?.duration.frames() == 4
            && recipe(&after, &id("gap-final")?)?.duration.frames() == 5
            && d.app().sequence_cursor == 31
            && d.app().sequence_length() == 55,
        json!({"mapped_gap_frames":6,"unselected_outer_play":[3,4],"final_gap":5,"Edit":31,"output_frames":55}), d.snapshot())?;
    d.capture("Outer Play 2 isolates two owned gaps under the selected inner plays")?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Undo restores both outer-play gap definitions and the original output",
        equivalent(document(d)?, baseline) && d.app().sequence_length() == 50,
        json!({"history":"one transaction","output_frames":50}),
        d.snapshot(),
    )?;
    Ok(())
}

fn navigate_selected_gap_pair(
    d: &mut Driver<'_>,
    plays: &str,
    outer_play: bool,
) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.chord(&[Key::G, Key::G, Key::Enter])?;
    if outer_play {
        d.command("scope play 2")?;
    }
    d.key(Key::Enter)?;
    d.command(&format!("scope plays {plays}"))?;
    d.key(Key::J)?;
    settled(d)?;
    scroll_top(d)?;
    Ok(())
}

fn create_fixture(source: &Path, path: &Path, freeze: HoldVideo) -> Result<(), String> {
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    let before = store.snapshot().map_err(|e| e.to_string())?;
    let audio = before
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Source { source } => source.audio.clone(),
            _ => None,
        })
        .ok_or("Owned-gap replay needs qualified Original audio")?;
    let start = audio.span.start();
    let room = SourceAudio {
        asset: audio.asset.clone(),
        span: SourceSpan::new(
            start,
            SourceTimestamp {
                ticks: start
                    .ticks
                    .checked_add(4800)
                    .ok_or("Room-tone span overflow")?,
                time_base: start.time_base,
            },
        )
        .map_err(|e| e.to_string())?,
    };
    let hold = |name: &str, frames: i64, video: HoldVideo| -> Result<BeatNode, String> {
        Ok(BeatNode::hold(
            name,
            HoldRecipe {
                duration: FrameDuration::new(frames).map_err(|e| e.to_string())?,
                video,
                picture_context: None,
                audio: HoldAudio::RoomTone {
                    source: room.clone(),
                },
            },
        ))
    };
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        return Err("Original project root is not a Sequence".into());
    };
    let revision =
        |name: &str| RevisionId::new(format!("ui-owned-gap-{name}")).map_err(|e| e.to_string());
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("clear")?,
            command: Command::DeleteChildren {
                parent: before.root().clone(),
                first: children.first().ok_or("Empty Original edit")?.clone(),
                last: children.last().ok_or("Empty Original edit")?.clone(),
                timing: AudioTimingId {
                    allocation: revision("clear")?,
                    ordinal: 0,
                },
            },
        })
        .map_err(|e| e.to_string())?;
    let id = |name: &str| fixture::id(name);
    let outer = id("outer")?;
    let inner = id("inner")?;
    let body = id("body")?;
    let inner_allocation = revision("inner-plays")?;
    let repeat = |child: NodeId,
                  count: u32,
                  allocation: RevisionId,
                  gap: Option<HoldRecipe>|
     -> Result<BeatNode, String> {
        let mut node = BeatNode::sequence("Repeat", vec![]);
        node.kind = NodeKind::Repeat {
            child,
            iterations: IterationOrder::new(allocation, count).map_err(|e| e.to_string())?,
            gap,
            escalation: None,
        };
        Ok(node)
    };
    let default_gap = hold("Default gap", 1, HoldVideo::Background)?.kind;
    let NodeKind::Hold {
        recipe: default_gap,
    } = default_gap
    else {
        unreachable!()
    };
    let nodes = BTreeMap::from([
        (
            outer.clone(),
            repeat(inner.clone(), 2, revision("outer-plays")?, None)?,
        ),
        (
            inner.clone(),
            repeat(body.clone(), 3, inner_allocation.clone(), Some(default_gap))?,
        ),
        (
            body.clone(),
            BeatNode::sequence(
                "Default body",
                vec![id("before")?, id("selected")?, id("after")?],
            ),
        ),
        (id("before")?, hold("Before", 2, HoldVideo::Background)?),
        (id("selected")?, hold("Selected", 2, freeze.clone())?),
        (id("after")?, hold("After", 2, HoldVideo::Background)?),
        (
            id("gap-zero")?,
            hold("Gap after play 1", 3, freeze.clone())?,
        ),
        (id("gap-one")?, hold("Gap after play 2", 4, freeze.clone())?),
        (
            id("gap-final")?,
            hold("Final dormant gap", 5, freeze.clone())?,
        ),
    ]);
    let gap_overrides = PlayOverrides::try_from(
        (0..3)
            .map(|ordinal| {
                Ok(PlayOverride {
                    iteration: IterationId {
                        allocation: inner_allocation.clone(),
                        ordinal,
                    },
                    root: id(
                        ["gap-zero", "gap-one", "gap-final"][usize::try_from(ordinal).unwrap()]
                    )?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    )
    .map_err(|e| e.to_string())?;
    let subtree = Subtree {
        root: outer,
        nodes,
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::from([(inner, gap_overrides)]),
    };
    let current = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: revision("insert")?,
            command: Command::Insert {
                parent: current.root().clone(),
                index: 0,
                subtree,
            },
        })
        .map_err(|e| e.to_string())?;
    store
        .snapshot()
        .map_err(|e| e.to_string())?
        .validate()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn iteration(document: &ProjectDocument, name: &str, position: u32) -> Result<IterationId, String> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&fixture::id(name)?].kind else {
        return Err("Expected Repeat".into());
    };
    iterations
        .at(position)
        .ok_or_else(|| "Missing iteration".into())
}
