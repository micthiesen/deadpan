//! Native This-play duration editing beneath one fixed Edit Retime.

use super::*;
use deadpan_core::{
    AudioTimingId, BeatNode, Command, CommandRequest, FrameDuration, FrameRange, HoldAudio,
    HoldRecipe, HoldVideo, IterationOrder, NodeId, NodeKind, PitchPolicy, ProjectFrame,
    RepeatEditBranch, RevisionId, SourceTimestamp, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use egui::Key;
use std::{collections::BTreeMap, num::NonZeroU32, path::Path};

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No This-play duration workspace")?;
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
    let live = scratch.path().join("this-play-fixed-duration.deadpan");
    fixture::create(&previous, &live, freeze)?;
    let result = (|| {
        super::shared_hold_duration::open(d, &live, original)?;
        fixture::assert_fixture(d)?;
        controls(d)?;
        recording_and_delayed(d)?;
        reopen_check(d, &live, original)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private This-play duration writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        super::shared_hold_duration::open(d, &previous, original)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "This-play duration replay restores the original project and cursor",
            super::shared_hold_duration::equivalent(document(d)?, &baseline)
                && d.app().source_cursor == original
                && d.app().scoped.is_none(),
            json!({"project":"restored","Original":original,"inspector":"closed"}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; This-play cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn controls(d: &mut Driver<'_>) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d)?;
    let baseline = document(d)?.clone();
    let prior = super::shared_hold_duration::target(d)?;
    let displayed_picture = d.app().presentation.displayed_source_frame();
    let repeat = fixture::id("outer")?;
    let stage = fixture::id("stage")?;
    let held = fixture::id("held")?;
    let fixed = baseline.nodes()[&stage].clone();
    d.check(
        "This-play duration captures the second stable play through one fixed Edit Retime",
        prior.target.repeats.len() == 1
            && matches!(&prior.target.repeats[0].branch,
                RepeatEditBranch::Play { iteration } if iteration.ordinal == 1)
            && prior.also.is_empty() && prior.has_single_play_authority()
            && prior.presentation.is_some()
            && matches!(&fixed.kind, NodeKind::Retime { purpose: deadpan_core::RetimePurpose::Edit, .. })
            && d.app().sequence_length() == 24 && d.app().sequence_cursor == 10
            && matches!(&baseline.nodes()[&held].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 2 && matches!(&recipe.video, HoldVideo::Freeze { .. })
                    && matches!(&recipe.audio, HoldAudio::RoomTone { .. })),
        json!({"scope":"This play 2/3","fixed_Retime":true,"Edit":10,"output_frames":24}),
        d.snapshot(),
    )?;
    let scope_hint =
        scenarios::text_paint_visibility(d, "This play changes only the selected play.");
    d.check(
        "The visible inspector explanation follows This play scope",
        !scope_hint.is_empty()
            && scope_hint.iter().all(|text| text["fully_visible"] == true)
            && scenarios::text_paint_visibility(d, "All plays changes the shared definition.")
                .is_empty(),
        json!({"scope":"This play","explanation":"fully painted"}),
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
    d.capture("This play fixed Retime duration prefill")?;
    super::shared_hold_duration::submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let grown = document(d)?.clone();
    let after = super::shared_hold_duration::target(d)?;
    d.check(
        "Growing one play creates only its stable Repeat override and retains fixed timing",
        grown.nodes()[&stage] == fixed
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
                .is_ok_and(|duration| duration.frames() == 24)
            && after.target.repeats == prior.target.repeats
            && after.presentation.is_some()
            && d.app().sequence_cursor == 10
            && d.app().presentation.displayed_source_frame() == displayed_picture,
        json!({"this_play_frames":4,"overrides":1,"fixed_Retime":true,"Edit":10}),
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
        document(d)?.nodes()[&stage] == fixed
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
    choose(d)?;
    edit(d, "hold-duration 3f")?;
    let shortened = document(d)?.clone();
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
    choose(d)?;
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
    d.capture("This play fixed Retime duration history")?;
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

fn recording_and_delayed(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_core::{PauseLength, SemanticInstruction};
    choose(d)?;
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
    let intent = SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
    };
    d.check(
        "Saved This-play duration contains only semantic length intent",
        saved == Some(vec![intent]) && document(d)?.nodes()[&stage] == baseline.nodes()[&stage],
        json!({"saved":"SetHoldDuration 3f","fixed_Retime":true}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    choose(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Run resolves This-play duration intent at the current stable play",
        document(d)?.nodes()[&stage] == baseline.nodes()[&stage]
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
    choose(d)?;
    let prior = super::shared_hold_duration::target(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    let pending = (|| {
        let revision = d.revision();
        d.command("hold-duration 3f")?;
        d.wait_for(
            "This-play duration is durable before newer navigation",
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
        let repeat = fixture::id("outer")?;
        d.check("Delayed duration persists without resetting the cursor or Repeat clock",
            d.app().sequence_cursor == cursor && d.app().sequence_length() == 24
                && document(d)?.nodes()[&stage] == baseline.nodes()[&stage]
                && document(d)?.overrides().get(&repeat).is_some_and(|entries| entries.len() == 1)
                && matches!(&document(d)?.nodes()[&prior.target.node].kind, NodeKind::Hold { recipe }
                    if recipe.duration.frames() == 3)
                && d.app().scoped.is_none(),
            json!({"Edit":cursor,"output_frames":24,"this_play_frames":3,"fixed_Retime":true}), d.snapshot())
    })();
    d.app_mut().feedback.hold_project_updates = false;
    pending
}

fn reopen_check(d: &mut Driver<'_>, live: &Path, original: u64) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close This-play project before reopening", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    super::shared_hold_duration::open(d, live, original)?;
    choose(d)?;
    let repeat = fixture::id("outer")?;
    let stage = fixture::id("stage")?;
    let held = fixture::id("held")?;
    d.check("Reopened project retains the selected-play override, real audio and fixed Retime",
        document(d)?.overrides().get(&repeat).is_some_and(|entries| entries.len() == 1)
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

fn choose(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L, Key::Enter])?;
    d.command("scope play 2")?;
    d.key(Key::Enter)?;
    d.key(Key::Enter)?;
    d.key(Key::J)?;
    super::shared_hold_duration::settled(d)?;
    scroll_top(d)?;
    let target = super::shared_hold_duration::target(d)?;
    if !matches!(&document(d)?.nodes()[&target.target.node].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Freeze { .. }))
        || target.target.repeats.len() != 1
    {
        return Err("This-play navigation did not select the real Freeze Hold".into());
    }
    Ok(())
}

fn scroll_top(d: &mut Driver<'_>) -> Result<(), String> {
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore This-play duration controls",
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
        d.step("This-play inspector scroll settles", false)?;
    }
    Ok(())
}

fn edit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(text)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)
}

mod fixture {
    use super::*;

    pub(super) fn id(name: &str) -> Result<NodeId, String> {
        NodeId::new(format!("ui-this-play-fixed-duration-{name}")).map_err(|e| e.to_string())
    }
    fn rev(name: &str) -> Result<RevisionId, String> {
        RevisionId::new(format!("ui-this-play-fixed-duration-{name}")).map_err(|e| e.to_string())
    }
    fn commit(store: &mut ProjectStore, command: Command, name: &str) -> Result<(), String> {
        let before = store.snapshot().map_err(|e| e.to_string())?;
        store
            .commit(&CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: rev(name)?,
                command,
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn hold(
        label: &str,
        frames: i64,
        template: &HoldRecipe,
        video: HoldVideo,
    ) -> Result<BeatNode, String> {
        let mut recipe = template.clone();
        recipe.duration = FrameDuration::new(frames).map_err(|e| e.to_string())?;
        recipe.video = video;
        Ok(BeatNode::hold(label, recipe))
    }
    pub(super) fn create(source: &Path, path: &Path, freeze: HoldVideo) -> Result<(), String> {
        super::super::shared_hold_duration::fixture::create(source, path, false, freeze.clone())?;
        let mut store =
            ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
        let before = store.snapshot().map_err(|e| e.to_string())?;
        let template = before
            .nodes()
            .values()
            .find_map(|node| match &node.kind {
                NodeKind::Hold { recipe }
                    if matches!(&recipe.audio, HoldAudio::RoomTone { .. }) =>
                {
                    Some(recipe.clone())
                }
                _ => None,
            })
            .ok_or("Fixture has no real RoomTone recipe")?;
        let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
            return Err("Qualified Original project has no root Sequence".into());
        };
        commit(
            &mut store,
            Command::DeleteChildren {
                parent: before.root().clone(),
                first: children.first().ok_or("Empty source edit")?.clone(),
                last: children.last().ok_or("Empty source edit")?.clone(),
                timing: AudioTimingId {
                    allocation: rev("clear")?,
                    ordinal: 0,
                },
            },
            "clear",
        )?;
        let mut repeat = BeatNode::sequence("Outer repeat", vec![]);
        repeat.kind = NodeKind::Repeat {
            child: id("stage")?,
            iterations: IterationOrder::new(rev("outer-order")?, 3).map_err(|e| e.to_string())?,
            gap: None,
            escalation: None,
        };
        let mut stage = BeatNode::sequence("Fixed stage", vec![]);
        stage.kind = NodeKind::Retime {
            child: id("body")?,
            duration: FrameDuration::new(6).map_err(|e| e.to_string())?,
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(8))
                .map_err(|e| e.to_string())?,
            pitch: PitchPolicy::FollowSpeed,
            purpose: deadpan_core::RetimePurpose::Edit,
        };
        let nodes = BTreeMap::from([
            (id("outer")?, repeat),
            (id("stage")?, stage),
            (
                id("body")?,
                BeatNode::sequence(
                    "Retime body",
                    vec![id("before")?, id("held")?, id("suffix")?],
                ),
            ),
            (
                id("before")?,
                hold("Before", 2, &template, HoldVideo::Background)?,
            ),
            (id("held")?, hold("Selected Freeze", 2, &template, freeze)?),
            (
                id("suffix")?,
                hold("Suffix", 6, &template, HoldVideo::Background)?,
            ),
        ]);
        let subtree = Subtree {
            root: id("outer")?,
            nodes,
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        };
        let lead = Subtree {
            root: id("lead")?,
            nodes: BTreeMap::from([(
                id("lead")?,
                hold("Lead", 3, &template, HoldVideo::Background)?,
            )]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        };
        let tail = Subtree {
            root: id("tail")?,
            nodes: BTreeMap::from([(
                id("tail")?,
                hold("Tail", 3, &template, HoldVideo::Background)?,
            )]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        };
        for (index, (name, child)) in [("lead", lead), ("outer", subtree), ("tail", tail)]
            .into_iter()
            .enumerate()
        {
            commit(
                &mut store,
                Command::Insert {
                    parent: before.root().clone(),
                    index,
                    subtree: child,
                },
                &format!("insert-{name}"),
            )?;
        }
        let after = store.snapshot().map_err(|e| e.to_string())?;
        after.validate().map_err(|e| e.to_string())?;
        if after.duration().map_err(|e| e.to_string())?.frames() != 24 {
            return Err(
                "This-play fixed-Retime fixture did not retain its 24-frame duration".into(),
            );
        }
        Ok(())
    }
    pub(super) fn assert_fixture(d: &Driver<'_>) -> Result<(), String> {
        let doc = super::document(d)?;
        let NodeKind::Sequence { children } = &doc.nodes()[doc.root()].kind else {
            return Err("Fixture root is not a Sequence".into());
        };
        if children != &[id("lead")?, id("outer")?, id("tail")?] {
            return Err("Fixture does not keep only the expected Sequence ancestry".into());
        }
        let outer = doc
            .nodes()
            .get(&id("outer")?)
            .ok_or("Missing outer Repeat")?;
        let NodeKind::Repeat {
            child, iterations, ..
        } = &outer.kind
        else {
            return Err("Fixture outer node is not Repeat".into());
        };
        let NodeKind::Retime {
            child: body,
            duration,
            mapping,
            purpose,
            ..
        } = &doc.nodes()[child].kind
        else {
            return Err("Repeat child is not fixed Retime".into());
        };
        let NodeKind::Sequence {
            children: body_children,
        } = &doc.nodes()[body].kind
        else {
            return Err("Retime child is not an ordinary Sequence".into());
        };
        for name in ["before", "held", "suffix", "lead", "tail"] {
            if !matches!(&doc.nodes()[&id(name)?].kind, NodeKind::Hold { recipe }
                if matches!(&recipe.audio, HoldAudio::RoomTone { .. }))
            {
                return Err("Fixture Hold lacks the qualified RoomTone recipe".into());
            }
        }
        if body_children != &[id("before")?, id("held")?, id("suffix")?]
            || iterations.len() != 3
            || duration.frames() != 6
            || mapping.start() != ProjectFrame(0)
            || mapping.end() != ProjectFrame(8)
            || *purpose != deadpan_core::RetimePurpose::Edit
            || doc.nodes().len() != 9
            || doc
                .nodes()
                .values()
                .filter(|node| matches!(&node.kind, NodeKind::Repeat { .. }))
                .count()
                != 1
            || doc
                .nodes()
                .values()
                .filter(|node| matches!(&node.kind, NodeKind::Retime { .. }))
                .count()
                != 1
            || !matches!(&doc.nodes()[&id("held")?].kind, NodeKind::Hold { recipe }
                if matches!(&recipe.video, HoldVideo::Freeze { .. }))
        {
            return Err(
                "Fixture does not have exactly one outer Repeat and fixed Edit Retime".into(),
            );
        }
        Ok(())
    }
}
