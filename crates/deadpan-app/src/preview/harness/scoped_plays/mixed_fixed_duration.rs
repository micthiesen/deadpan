//! Native mixed Default/Play duration editing beneath one fixed Retime.

use super::nested_fixed_duration::fixture;
use super::*;
use deadpan_core::{
    Command, CommandRequest, FrameDuration, HoldAudio, HoldVideo, NodeId, NodeKind, ProjectFrame,
    RepeatEditBranch, RevisionId, SourceTimestamp,
};
use egui::Key;
use std::{num::NonZeroU32, path::Path};

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No Mixed fixed-window duration workspace")?;
    let previous = workspace.path.clone();
    let baseline = workspace.document.clone();
    let original = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|e| e.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("No actual Original picture")?;
    let index = workspace
        .sources
        .get(asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No measured Original index")?;
    let measured = picture
        .picture
        .select_source_frame(index)
        .map_err(|e| e.to_string())?;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let result = (|| {
        for outer_default in [true, false] {
            let live = scratch.path().join(if outer_default {
                "mixed-default-play.deadpan"
            } else {
                "mixed-play-default.deadpan"
            });
            fixture::create(&previous, &live, freeze.clone())?;
            super::shared_hold_duration::open(d, &live, original)?;
            fixture::assert_fixture(d)?;
            controls(d, outer_default)?;
            recording_and_delayed(d, outer_default)?;
            let committed = document(d)?.clone();
            reopen_check(d, &live, original, &committed, outer_default)?;
            if outer_default {
                d.app().service.submit(ProjectRequest::Close)?;
                d.wait_for("Close completed mixed direction", |app| {
                    app.workspace.is_none() && !app.service.is_busy()
                })?;
            }
        }
        Ok(())
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for(
            "Close the private Mixed fixed-window duration writer",
            |app| app.workspace.is_none() && !app.service.is_busy(),
        )?;
        super::shared_hold_duration::open(d, &previous, original)?;
        if edit_cursor > 0 {
            let mut keys = edit_cursor
                .to_string()
                .chars()
                .map(|digit| {
                    Key::from_name(&digit.to_string()).ok_or("Invalid Edit cursor digit".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            keys.push(Key::L);
            d.chord(&keys)?;
        }
        super::shared_hold_duration::settled(d)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Mixed fixed-window duration replay restores the original project and cursor",
            super::shared_hold_duration::equivalent(document(d)?, &baseline)
                && d.app().source_cursor == original
                && d.app().sequence_cursor == edit_cursor
                && d.app().scoped.is_none(),
            json!({"project":"restored","Original":original,"inspector":"closed"}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(format!("{error}; Mixed fixed-window cleanup: {cleanup}"))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn controls(d: &mut Driver<'_>, outer_default: bool) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d, outer_default)?;
    let baseline = document(d)?.clone();
    let prior = super::shared_hold_duration::target(d)?;
    let displayed_picture = d.app().presentation.displayed_source_frame();
    let repeat = fixture::id(if outer_default { "inner" } else { "outer" })?;
    let stage = fixture::id("stage")?;
    let held = fixture::id("held")?;
    let fixed = baseline.nodes()[&stage].clone();
    let expected_pool = if outer_default { 5 } else { 6 };
    let instance = prior
        .presentation
        .as_ref()
        .ok_or("No concrete nested Hold")?;
    let needs = baseline
        .scoped_hold_duration_requirements(
            &prior.target,
            FrameDuration::new(4).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    let preflight_request = CommandRequest {
        project_id: baseline.project_id().clone(),
        expected_revision: baseline.revision_id().clone(),
        new_revision: RevisionId::new("ui-mixed-duration-preflight").map_err(|e| e.to_string())?,
        command: Command::SetScopedHoldDuration {
            target: prior.target.clone(),
            duration: FrameDuration::new(4).map_err(|e| e.to_string())?,
            identities: deadpan_core::OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| {
                        NodeId::new(format!("ui-mixed-duration-preflight-{index}"))
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<_, _>>()?,
                marks: vec![],
            },
        },
    };
    let preflight =
        deadpan_core::prepare_scoped_hold_duration(&baseline, &preflight_request, Some(instance))
            .map_err(|e| e.to_string())?;
    let preflight_proof = preflight
        .isolation()
        .validate(&baseline, &preflight_request, &preflight.document)
        .map_err(|e| e.to_string())?;
    d.check(
        "Independent mixed preflight requires one explicit Play clone and no marks",
        needs.nodes == expected_pool
            && needs.marks == 0
            && preflight_proof.record().steps().len() == 1
            && preflight.fixed_visibility().is_some_and(|visible| {
                visible.frames.start() == ProjectFrame(22)
                    && visible.frames.end() == ProjectFrame(25)
            })
            && preflight_proof
                .map_instance_forward(instance)
                .map_err(|e| e.to_string())?
                == preflight
                    .presentation
                    .clone()
                    .ok_or("No preflight representative")?,
        json!({"preflight_only":true,"nodes":expected_pool,"marks":0,"isolation_steps":1}),
        d.snapshot(),
    )?;
    d.check(
        "Mixed fixed-window duration captures the explicit mixed choices through one fixed Edit Retime",
        prior.target.repeats.len() == 2
            && branch_choices(&prior.target) == [!outer_default, outer_default]
            && prior.also.is_empty() && !prior.has_single_play_authority()
            && prior.presentation.is_some()
            && needs.nodes == expected_pool && needs.marks == 0
            && matches!(&fixed.kind, NodeKind::Retime { purpose: deadpan_core::RetimePurpose::Edit, .. })
            && d.app().sequence_length() == 42 && d.app().sequence_cursor == 22
            && matches!(&baseline.nodes()[&held].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 2 && matches!(&recipe.video, HoldVideo::Freeze { .. })
                    && matches!(&recipe.audio, HoldAudio::RoomTone { .. })),
        json!({"outer_default":outer_default,"fixed_Retime":true,"Edit":22,"output_frames":42}),
        d.snapshot(),
    )?;
    let scope_hint = scenarios::text_paint_visibility(
        d,
        "Changes follow each level's All plays or This play choice.",
    );
    d.check(
        "The visible inspector explanation follows both mixed scope choices",
        !scope_hint.is_empty()
            && scope_hint.iter().all(|text| text["fully_visible"] == true)
            && scenarios::text_paint_visibility(d, "All plays changes the shared definition.")
                .is_empty(),
        json!({"outer_default":outer_default,"explanation":"fully painted"}),
        json!({"paint":scope_hint}),
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Enter prefills the exact selected two-frame Hold without a write",
        d.app().command_open && d.app().command == "hold-duration 2f" && d.revision() == revision,
        json!({"prefill":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    super::shared_hold_duration::submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let grown = document(d)?.clone();
    let after = super::shared_hold_duration::target(d)?;
    verify_affected_uses(d, &after.target.node, outer_default, 4)?;
    d.check(
        "Growing one play creates only its stable Repeat override and retains fixed timing",
        grown.nodes()[&stage] == fixed
            && baseline
                .nodes()
                .iter()
                .all(|(id, node)| grown.nodes().get(id) == Some(node))
            && grown.nodes().len() == baseline.nodes().len() + expected_pool
            && grown
                .overrides()
                .values()
                .map(|entries| entries.len())
                .sum::<usize>()
                == 1
            && grown.nodes()[&fixture::id("inner")?] == baseline.nodes()[&fixture::id("inner")?]
            && grown.nodes()[&held] == baseline.nodes()[&held]
            && grown.nodes().contains_key(&after.target.node)
            && after.target.node != held
            && matches!(&grown.nodes()[&after.target.node].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 4)
            && grown
                .overrides()
                .get(&repeat)
                .is_some_and(|entries| entries.len() == 1)
            && grown
                .duration()
                .is_ok_and(|duration| duration.frames() == 42)
            && after.target.repeats[0] == prior.target.repeats[0]
            && after.target.repeats[1].branch == prior.target.repeats[1].branch
            && (after.target.repeats[1].repeat != prior.target.repeats[1].repeat) == !outer_default
            && after.presentation.is_some()
            && after.presentation.as_ref() == Some(&selected_instance(&grown)?)
            && d.app().sequence_cursor == 22
            && d.app().presentation.displayed_source_frame() == displayed_picture,
        json!({"this_play_frames":4,"overrides":1,"fixed_Retime":true,"Edit":22}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.command("hold-duration 4f")?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Repeating the selected duration is a no-op",
        document(d)? == &grown
            && d.revision() == revision
            && super::shared_hold_duration::target(d)?.target == after.target,
        json!({"history":"unchanged","frames":4}),
        d.snapshot(),
    )?;
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let before_redo = document(d)?.clone();
    d.check(
        "Undo removes the isolated play and restores its exact default recipe",
        super::shared_hold_duration::equivalent(document(d)?, &baseline)
            && d.app().scoped.is_none(),
        json!({"overrides":0,"frames":2}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Redo restores only the selected play edit",
        super::shared_hold_duration::equivalent(document(d)?, &grown)
            && document(d)?.nodes()[&stage] == fixed
            && document(d)?
                .overrides()
                .get(&repeat)
                .is_some_and(|entries| entries.len() == 1)
            && document(d)?.nodes().contains_key(&after.target.node)
            && matches!(&document(d)?.nodes()[&after.target.node].kind, NodeKind::Hold { recipe }
            if recipe.duration.frames() == 4)
            && !super::shared_hold_duration::equivalent(document(d)?, &before_redo),
        json!({"overrides":1,"fixed_Retime":true}),
        d.snapshot(),
    )?;
    choose(d, outer_default)?;
    let owned = super::shared_hold_duration::target(d)?;
    let needs = document(d)?
        .scoped_hold_duration_requirements(
            &owned.target,
            FrameDuration::new(3).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    d.check(
        "Already-owned mixed duration needs no additional identities",
        needs.nodes == 0 && needs.marks == 0,
        json!({"nodes":0,"marks":0}),
        d.snapshot(),
    )?;
    edit(d, "hold-duration 3f")?;
    let shortened = document(d)?.clone();
    verify_affected_uses(d, &after.target.node, outer_default, 3)?;
    d.check(
        "A second edit updates the owned Hold without adding another override",
        shortened.nodes().len() == grown.nodes().len()
            && shortened
                .overrides()
                .get(&repeat)
                .is_some_and(|entries| entries.len() == 1)
            && shortened.nodes().contains_key(&after.target.node)
            && matches!(&shortened.nodes()[&after.target.node].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 3),
        json!({"this_play_frames":3,"overrides":1,"node_count":"stable"}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let immediately_before_dot = document(d)?.clone();
    choose(d, outer_default)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let actual = serde_json::to_value(document(d)?).map_err(|e| e.to_string())?;
    let mut expected = serde_json::to_value(&shortened).map_err(|e| e.to_string())?;
    expected["revision_id"] = actual["revision_id"].clone();
    let diffs: Vec<_> = actual
        .as_object()
        .unwrap()
        .keys()
        .chain(expected.as_object().unwrap().keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|key| actual.get(*key) != expected.get(*key))
        .cloned()
        .collect();
    d.check(
        "Dot repeats the selected duration against the reselected stable play",
        diffs.iter().all(|field| field == "audio_bindings")
            && super::fixed_retime_duration::same_audio_clock_intent(
                document(d)?.audio_bindings(),
                shortened.audio_bindings(),
            )
            && document(d)?.nodes()[&stage] == fixed
            && super::shared_hold_duration::target(d)?.target.node == after.target.node
            && super::shared_hold_duration::target(d)?.target.repeats == after.target.repeats,
        json!({"dot":"hold-duration 3f","fields":diffs,"fixed_Retime":true}),
        d.snapshot(),
    )?;
    d.capture(if outer_default {
        "Mixed fixed Retime All plays then This play duration history"
    } else {
        "Mixed fixed Retime This play then All plays duration history"
    })?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Undo after dot restores the immediately preceding four-frame state",
        super::shared_hold_duration::equivalent(document(d)?, &immediately_before_dot),
        json!({"this_play_frames":4}),
        d.snapshot(),
    )
}

fn recording_and_delayed(d: &mut Driver<'_>, outer_default: bool) -> Result<(), String> {
    use deadpan_core::{PauseLength, SemanticInstruction};
    choose(d, outer_default)?;
    let baseline = document(d)?.clone();
    let stage = fixture::id("stage")?;
    let revision = d.revision();
    d.chord(&[Key::Q, Key::W])?;
    d.command("hold-duration 3f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.key(Key::Q)?;
    super::shared_hold_duration::settled(d)?;
    let saved = d
        .app()
        .copied
        .entries()
        .find_map(|(name, value)| match value {
            crate::preview::copied::Content::Macro(program) if name == 'w' => {
                Some(program.instructions().to_vec())
            }
            _ => None,
        });
    let recorded = document(d)?.clone();
    let intent = SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
    };
    d.check(
        "Saved Mixed fixed-window duration contains only semantic length intent",
        saved == Some(vec![intent]) && document(d)?.nodes()[&stage] == baseline.nodes()[&stage],
        json!({"saved":"SetHoldDuration 3f","fixed_Retime":true}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    choose(d, outer_default)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Run resolves Mixed fixed-window duration intent at the current stable play",
        same_content(document(d)?, &recorded)?
            && document(d)?.nodes()[&stage] == baseline.nodes()[&stage]
            && document(d)?
                .overrides()
                .values()
                .map(|entries| entries.len())
                .sum::<usize>()
                == 1,
        json!({"run":"w","fixed_Retime":true,"overrides":1}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    choose(d, outer_default)?;
    let prior = super::shared_hold_duration::target(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    let pending = (|| {
        let revision = d.revision();
        d.command("hold-duration 3f")?;
        d.wait_for(
            "Mixed fixed-window duration is durable before newer navigation",
            |app| !app.service.is_busy(),
        )?;
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = super::shared_hold_duration::target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check(
            "Delayed receipt cannot reclaim a newer play navigation",
            current.target == prior.target && current.navigation != prior.navigation,
            json!({"target":"same stable play","navigation":"new generation"}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        super::shared_hold_duration::settled(d)?;
        let repeat = fixture::id(if outer_default { "inner" } else { "outer" })?;
        d.check("Delayed duration persists without resetting the cursor or Repeat clock",
            d.app().sequence_cursor == cursor && d.app().sequence_length() == 42
                && same_content(document(d)?, &recorded)?
                && document(d)?.nodes()[&stage] == baseline.nodes()[&stage]
                && document(d)?.overrides().get(&repeat).is_some_and(|entries| entries.len() == 1)
                && matches!(&document(d)?.nodes()[&prior.target.node].kind, NodeKind::Hold { recipe }
                    if recipe.duration.frames() == 3)
                && d.app().scoped.is_none(),
            json!({"Edit":cursor,"output_frames":42,"this_play_frames":3,"fixed_Retime":true}), d.snapshot())
    })();
    d.app_mut().feedback.hold_project_updates = false;
    pending
}

fn reopen_check(
    d: &mut Driver<'_>,
    live: &Path,
    original: u64,
    expected: &ProjectDocument,
    outer_default: bool,
) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close Mixed fixed-window project before reopening", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    super::shared_hold_duration::open(d, live, original)?;
    choose(d, outer_default)?;
    let repeat = fixture::id(if outer_default { "inner" } else { "outer" })?;
    let stage = fixture::id("stage")?;
    let held = fixture::id("held")?;
    d.check("Reopened project retains the selected-play override, real audio and fixed Retime",
        document(d)? == expected
            && document(d)?.overrides().values().map(|entries| entries.len()).sum::<usize>() == 1
            && document(d)?.overrides().get(&repeat).is_some_and(|entries| entries.len() == 1)
            && matches!(&document(d)?.nodes()[&stage].kind, NodeKind::Retime {
                duration, mapping, purpose: deadpan_core::RetimePurpose::Edit, ..
            } if duration.frames() == 6 && mapping.start() == ProjectFrame(0)
                && mapping.end() == ProjectFrame(8))
            && super::shared_hold_duration::target(d)?.target.node != held
            && matches!(&document(d)?.nodes()[&super::shared_hold_duration::target(d)?.target.node].kind,
                NodeKind::Hold { recipe } if recipe.duration.frames() == 3
                    && matches!(&recipe.audio, HoldAudio::RoomTone { .. })),
        json!({"overrides":1,"fixed_Retime":true,"audio":"RoomTone"}), d.snapshot())
}

fn choose(d: &mut Driver<'_>, outer_default: bool) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num2, Key::L, Key::Enter])?;
    d.command(if outer_default {
        "scope all"
    } else {
        "scope play 2"
    })?;
    d.key(Key::Enter)?; // Outer branch -> actual Inner Repeat.
    d.command(if outer_default {
        "scope play 2"
    } else {
        "scope all"
    })?;
    d.key(Key::Enter)?; // Inner selected play -> fixed Retime.
    d.key(Key::Enter)?; // Fixed Retime -> ordinary Body Sequence.
    d.key(Key::J)?;
    super::shared_hold_duration::settled(d)?;
    scroll_top(d)?;
    let target = super::shared_hold_duration::target(d)?;
    if !matches!(&document(d)?.nodes()[&target.target.node].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Freeze { .. }))
        || branch_choices(&target.target) != [!outer_default, outer_default]
        || target.presentation.as_ref() != Some(&selected_instance(document(d)?)?)
    {
        return Err("Mixed fixed-window navigation did not select the real Freeze Hold".into());
    }
    Ok(())
}

fn branch_choices(target: &deadpan_core::ScopedNodeTarget) -> Vec<bool> {
    target
        .repeats
        .iter()
        .map(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
        .collect()
}

/// Literal 3x2 stage windows and center sampling of input [2, 2+length).
/// The plan supplies observations only; it never supplies the expected uses.
fn verify_affected_uses(
    d: &mut Driver<'_>,
    mapped: &NodeId,
    outer_default: bool,
    length: i64,
) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No mixed fixed workspace")?;
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for frame in 0..42_i64 {
        if workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?
            .framing
            .iter()
            .any(|layer| &layer.instance.node == mapped)
        {
            observed.push(frame);
        }
    }
    for family in 0..3 {
        for play in 0..2 {
            if if outer_default {
                play == 1
            } else {
                family == 1
            } {
                for local in 0..6 {
                    let center = (2 * local + 1) * 2;
                    if center >= 6 && center < 3 * (2 + length) {
                        expected.push(3 + family * 12 + play * 6 + local);
                    }
                }
            }
        }
    }
    d.check("Mixed fixed duration changes precisely the intended three or two live uses",
        observed == expected,
        json!({"outer_default":outer_default,"affected_uses":if outer_default {3}else{2},"sampled_frames":expected}),
        json!({"sampled_frames":observed}))
}

/// Resolve the two committed stable Play identities and each actual owned branch.
/// Play/Default remaps Inner, while Default/Play owns the selected Stage.
fn selected_instance(doc: &ProjectDocument) -> Result<deadpan_core::InstancePath, String> {
    let outer = fixture::id("outer")?;
    let branch = |repeat: &NodeId| -> Result<(NodeId, deadpan_core::RepeatInstance), String> {
        let NodeKind::Repeat {
            child, iterations, ..
        } = &doc.nodes()[repeat].kind
        else {
            return Err("Expected committed nested Repeat".into());
        };
        let iteration = iterations.at(1).ok_or("Missing second stable play")?;
        let root = doc
            .overrides()
            .get(repeat)
            .and_then(|entries| entries.get(&iteration))
            .unwrap_or(child)
            .clone();
        Ok((
            root,
            deadpan_core::RepeatInstance {
                node: repeat.clone(),
                iteration,
            },
        ))
    };
    let (inner, outer_play) = branch(&outer)?;
    let (stage, inner_play) = branch(&inner)?;
    let NodeKind::Retime { child: body, .. } = &doc.nodes()[&stage].kind else {
        return Err("Selected inner branch lost fixed Retime".into());
    };
    let NodeKind::Sequence { children } = &doc.nodes()[body].kind else {
        return Err("Selected fixed Retime lost ordinary body".into());
    };
    let node = children.get(1).ok_or("Selected body lost Hold")?.clone();
    Ok(deadpan_core::InstancePath {
        node,
        repeats: vec![outer_play, inner_play],
    })
}

fn same_content(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual_json = serde_json::to_value(actual).map_err(|e| e.to_string())?;
    let mut expected_json = serde_json::to_value(expected).map_err(|e| e.to_string())?;
    actual_json["revision_id"] = expected_json["revision_id"].clone();
    // Permit only a bijection of immutable allocation names with identical layout,
    // placement, phase, reanchors and sound clocks, as checked by the shared oracle.
    if !super::fixed_retime_duration::same_audio_clock_intent(
        actual.audio_bindings(),
        expected.audio_bindings(),
    ) {
        return Ok(false);
    }
    expected_json["audio_bindings"] = actual_json["audio_bindings"].clone();
    Ok(actual_json == expected_json)
}

fn scroll_top(d: &mut Driver<'_>) -> Result<(), String> {
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore Mixed fixed-window duration controls",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 10_000.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Mixed fixed-window inspector scroll settles", false)?;
    }
    Ok(())
}

fn edit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(text)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)
}
