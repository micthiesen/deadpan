//! Large root outline replay with real storage and UI routing. The fixture uses
//! only silent background Holds, so this does not measure media/decode stress.

use std::collections::BTreeMap;
use std::error::Error;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, PresentationBasis, ProjectDocument, ProjectId, Subtree,
};
use deadpan_store::ProjectStore;

use super::*;

const BEATS: usize = 10_000;
const HOLD_FRAMES: u64 = 12;
const PROJECT_ID: &str = "ui-large-root-outline";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Large-project fixture: 10,000 root Background/Silence Holds in a real SQLite package. No source media, decoding, PCM or large asset inventory is stressed."
            .into(),
    );
    // The initial project was created in run()'s private library. Keeping this
    // sibling there leaves lifetime and cleanup with the outer app owner, so
    // the service never holds a writer for an already-deleted temporary tree.
    let initial = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Large-project replay requires the initial private workspace")?;
    let path = initial
        .path
        .parent()
        .ok_or("Private workspace has no parent")?
        .join("large-root-outline.deadpan");
    let started = Instant::now();
    create_fixture(&path).map_err(|error| format!("Large fixture preparation: {error}"))?;
    d.metric(
        "large_project_fixture_preparation_ms",
        started.elapsed().as_secs_f64() * 1000.0,
        SampleOutcome::Completed,
    );

    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path))]);
    d.key_modified(egui::Key::O, egui::Modifiers::COMMAND)?;
    d.wait_for(
        "Open 10,000 root beats through the project service",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document.project_id().as_str() == PROJECT_ID)
                && app.beat_rows.len() == BEATS
                && !app.service.is_busy()
        },
    )?;
    d.command("sequence")?;
    d.settled()?;
    let duration = u64::try_from(BEATS)
        .map_err(|error| error.to_string())?
        .checked_mul(HOLD_FRAMES)
        .ok_or("Large fixture duration overflow")?;
    d.check(
        "Opened fixture contains exactly 10,000 real root beats",
        d.app().beat_rows.len() == BEATS && d.app().sequence_length() == duration,
        json!({"root_beats":BEATS,"frames":duration}),
        json!({"root_beats":d.app().beat_rows.len(),"frames":d.app().sequence_length()}),
    )?;
    let initial_rect = selected_geometry(d)?;
    d.click_at("Focus the first root beat", initial_rect.center())?;
    d.key(egui::Key::End)?;
    d.settled()?;
    d.check(
        "End reaches the last of 10,000 beats without flattening the outline",
        selected_index(d) == Some(BEATS - 1) && d.app().sequence_cursor == duration,
        json!({"selected_index":BEATS-1,"cursor":duration}),
        json!({"selected_index":selected_index(d),"cursor":d.app().sequence_cursor}),
    )?;
    selected_geometry(d)?;
    bounded_cards(d)?;
    d.capture("Last of 10,000 root beats")?;

    let before_scroll = drawn_indices(d);
    let pointer = selected_geometry(d)?.center();
    d.events(
        "Wheel horizontally backward over the large beat outline",
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(640.0, 0.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for frame in 0..16 {
        d.step(
            &format!("Large outline wheel transition {frame}"),
            frame < 4,
        )?;
        bounded_cards(d)?;
    }
    let after_scroll = drawn_indices(d);
    d.check(
        "Pointer wheel changes the visible root range",
        after_scroll.first() < before_scroll.first() && !after_scroll.is_empty(),
        json!({"first_visible_before":before_scroll.first(),"direction":"toward earlier beats"}),
        json!({"rendered_before":before_scroll,"rendered_after":after_scroll}),
    )?;
    let (clicked_index, clicked_rect) = fully_visible_cards(d)
        .into_iter()
        .find(|(index, _)| Some(*index) != selected_index(d))
        .ok_or("Wheel did not expose an earlier fully visible root card")?;
    d.click_at("Select a wheel-revealed root card", clicked_rect.center())?;
    d.settled()?;
    d.check(
        "Pointer selects the visible card's real node and boundary",
        selected_index(d) == Some(clicked_index)
            && d.app().sequence_cursor == d.app().beat_rows[clicked_index].start,
        json!(clicked_index),
        json!({"selected_index":selected_index(d),"cursor":d.app().sequence_cursor}),
    )?;
    selected_geometry(d)?;

    d.harness.set_size(egui::vec2(960.0, 640.0));
    for frame in 0..4 {
        d.step(
            &format!("Large outline minimum viewport transition {frame}"),
            true,
        )?;
        selected_geometry(d)?;
        bounded_cards(d)?;
    }
    d.key(egui::Key::End)?;
    d.settled()?;
    selected_geometry(d)?;
    d.check(
        "Large outline runs at the native minimum viewport",
        d.harness.ctx.content_rect().size() == egui::vec2(960.0, 640.0),
        json!([960, 640]),
        json!([
            d.harness.ctx.content_rect().width(),
            d.harness.ctx.content_rect().height()
        ]),
    )?;
    d.events(
        "Remove hover before navigation measurements",
        vec![egui::Event::PointerGone],
    )?;

    let start_frame = d.frame;
    let samples = if d.options.mode == RunMode::Performance {
        160
    } else {
        64
    };
    let mut cpu_samples = Vec::with_capacity(samples);
    for index in 0..samples {
        d.key(if index % 2 == 0 {
            egui::Key::K
        } else {
            egui::Key::J
        })?;
        let expected = if index % 2 == 0 { BEATS - 2 } else { BEATS - 1 };
        d.check(
            "Rapid near-end navigation keeps the requested beat selected",
            selected_index(d) == Some(expected),
            json!(expected),
            json!(selected_index(d)),
        )?;
        selected_geometry(d)?;
        bounded_cards(d)?;
        let cpu_ms = d
            .report
            .timings
            .iter()
            .find(|metric| metric.name == "ui_frame_cpu_ms")
            .and_then(|metric| metric.samples.last())
            .ok_or("Navigation frame has no CPU timing sample")?
            .elapsed_ms;
        cpu_samples.push(cpu_ms);
        d.metric(
            "large_project_navigation_cpu_ms",
            cpu_ms,
            SampleOutcome::Completed,
        );
    }
    let navigation_frames = d.frame - start_frame;
    cpu_samples.sort_by(f64::total_cmp);
    let p95 = cpu_samples
        .get((cpu_samples.len() * 95).div_ceil(100).saturating_sub(1))
        .copied()
        .ok_or("Large navigation measurement contains no samples")?;
    d.check(
        "Each large-outline navigation input completes in one replay frame",
        navigation_frames == u64::try_from(samples).map_err(|error| error.to_string())?,
        json!(samples),
        json!({"navigation_frames":navigation_frames,"samples":cpu_samples.len(),"cpu_p95_ms":p95}),
    )?;
    d.settled()?;
    d.capture("Large outline navigation final state")?;
    if let Some(step) = d.report.steps.last_mut() {
        step.semantic["large_project"] = json!({
            "root_beats":BEATS,"fixture":"Background/Silence Holds; no source media",
            "navigation_frames":navigation_frames,"navigation_samples":cpu_samples.len(),
            "navigation_cpu_p95_ms":p95,
        });
    }
    if d.options.mode == RunMode::Performance {
        d.check(
            "10,000-beat navigation CPU p95 stays below the 8 ms input budget",
            p95 < 8.0,
            json!(8.0),
            json!({"cpu_p95_ms":p95,"samples":cpu_samples.len()}),
        )?;
    }
    Ok(())
}

fn create_fixture(path: &Path) -> Result<(), Box<dyn Error>> {
    let root = NodeId::new("large-root")?;
    let group = NodeId::new("fixture-group")?;
    let initial = ProjectDocument::new(
        ProjectId::new(PROJECT_ID)?,
        RevisionId::new("large-initial")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )?;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::with_capacity(BEATS);
    for index in 0..BEATS {
        let id = NodeId::new(format!("hold-{index:05}"))?;
        nodes.insert(
            id.clone(),
            BeatNode::hold(
                format!("Scale fixture {:05}", index + 1),
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(i64::try_from(HOLD_FRAMES)?)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        );
        children.push(id);
    }
    nodes.insert(group.clone(), BeatNode::sequence("Fixture group", children));
    let mut store = ProjectStore::create(path, &initial)?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("large-inserted")?,
        command: Command::Insert {
            parent: root,
            index: 0,
            subtree: Subtree {
                root: group.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    })?;
    let inserted = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: inserted.project_id().clone(),
        expected_revision: inserted.revision_id().clone(),
        new_revision: RevisionId::new("large-ungrouped")?,
        command: Command::Ungroup { node: group },
    })?;
    Ok(())
}

fn selected_index(d: &Driver<'_>) -> Option<usize> {
    d.app()
        .beat_rows
        .iter()
        .position(|row| Some(&row.id) == d.app().selected_beat.as_ref())
}

fn root_index(label: &str) -> Option<usize> {
    label
        .strip_prefix("Beat ")?
        .split_once(':')?
        .0
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

fn drawn_indices(d: &Driver<'_>) -> Vec<usize> {
    d.harness
        .root()
        .children_recursive()
        .filter_map(|node| root_index(node.accesskit_node().label()?.as_ref()))
        .collect()
}

fn fully_visible_cards(d: &Driver<'_>) -> Vec<(usize, egui::Rect)> {
    d.harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let index = root_index(node.accesskit_node().label()?.as_ref())?;
            let rect = node.rect();
            let painted = d.harness.output().shapes.iter().any(|clipped| {
                matches!(&clipped.shape,egui::Shape::Rect(shape) if shape.rect == rect
                && [style::SELECTED,style::PANEL].contains(&shape.fill))
                    && clipped.clip_rect.contains_rect(rect.shrink(1.0))
            });
            (painted && rect.is_positive() && d.harness.ctx.content_rect().contains_rect(rect))
                .then_some((index, rect))
        })
        .collect()
}

fn selected_geometry(d: &mut Driver<'_>) -> Result<egui::Rect, String> {
    let selected = selected_index(d).ok_or("Large outline has no selected root beat")?;
    let rect = fully_visible_cards(d)
        .into_iter()
        .find(|(index, _)| *index == selected)
        .map(|(_, rect)| rect);
    let selected_fill = rect.is_some_and(|rect| d.harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape,egui::Shape::Rect(shape) if shape.rect == rect && shape.fill == style::SELECTED)
    }));
    d.check(
        "Selected large-outline card is fully visible with selection paint",
        rect.is_some() && selected_fill,
        json!({"selected_index":selected,"fully_visible":true,"selection_fill":true}),
        json!({"selected_index":selected,"rect":rect.map(|rect|format!("{rect:?}")),"selection_fill":selected_fill}),
    )?;
    rect.ok_or_else(|| "Selected large-outline card has no visible rectangle".into())
}

fn bounded_cards(d: &mut Driver<'_>) -> Result<(), String> {
    let viewport = d.harness.ctx.content_rect();
    let layout = style::Layout::for_size(viewport.width(), viewport.height());
    // Full window width is a conservative upper bound on the strip viewport.
    // Two extra cards allow the partially visible edge and one overscan card.
    let maximum = (viewport.width() / layout.card_width).ceil() as usize + 2;
    let widgets = drawn_indices(d).len();
    let painted = d
        .harness
        .output()
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape,egui::Shape::Rect(shape)
            if (shape.rect.height()-layout.card_height).abs()<0.1
            && (shape.rect.width()-(layout.card_width-8.0)).abs()<0.1
            && [style::PANEL,style::SELECTED].contains(&shape.fill))
        })
        .count();
    d.check(
        "10,000-beat outline draws only the viewport and bounded overscan",
        widgets > 0 && widgets <= maximum && painted > 0 && painted <= maximum,
        json!({"maximum_cards":maximum,"derived_from":"ceil(window width/card pitch)+2"}),
        json!({"root_beats":BEATS,"card_widgets":widgets,"card_paints":painted,"frame":d.frame}),
    )
}
