//! Actual cached editing and deterministic Hold feedback, including durable undo.
//! Timing subsets stop before undo so the two product budgets remain distinct.

use std::collections::BTreeMap;

use deadpan_core::{BeatNode, HoldAudio, HoldVideo};

use super::*;

#[derive(Clone, Copy)]
enum EditKind {
    Repeat,
    Hold,
}

impl EditKind {
    fn name(self) -> &'static str {
        match self {
            Self::Repeat => "Cached Repeat",
            Self::Hold => "Inserted silent freeze",
        }
    }

    fn metric(self) -> &'static str {
        match self {
            Self::Repeat => "cached_repeat_input_to_picture_complete_ms",
            Self::Hold => "hold_fallback_input_to_picture_complete_ms",
        }
    }

    fn commit_metric(self) -> &'static str {
        match self {
            Self::Repeat => "cached_repeat_input_to_commit_ms",
            Self::Hold => "hold_input_to_commit_ms",
        }
    }

    fn budget_ms(self) -> f64 {
        match self {
            Self::Repeat => 50.0,
            Self::Hold => 100.0,
        }
    }
}

struct Baseline {
    nodes: BTreeMap<NodeId, BeatNode>,
    original: NodeId,
    duration: u64,
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    d.key(egui::Key::Home)?;
    d.settled()?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No edit-latency workspace")?;
    let roots = root_children(workspace);
    let original = roots
        .first()
        .ok_or("Edit-latency fixture has no root beat")?
        .clone();
    let baseline = Baseline {
        nodes: workspace.document.nodes().clone(),
        original,
        duration: d.app().sequence_length(),
    };
    d.check(
        "Edit latency starts with the full single-Original Source baseline",
        roots.len() == 1
            && matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
            && baseline
                .nodes
                .get(&baseline.original)
                .is_some_and(|node| matches!(&node.kind, NodeKind::Source { .. }))
            && baseline.duration > 0
            && d.app().sequence_cursor == 0,
        json!("one full Original Source at boundary zero"),
        d.snapshot(),
    )?;
    d.report.skipped.push(
        "Edit timings measure the real committed revision through offscreen GPU completion. Physical display presentation and acoustic Hold silence are outside this scenario."
            .into(),
    );
    let measured_cycles = if d.options.mode == RunMode::Performance {
        40
    } else {
        2
    };
    for kind in [EditKind::Repeat, EditKind::Hold] {
        if d.options.mode == RunMode::Performance {
            for cycle in 1..=4 {
                edit_and_undo(d, &baseline, kind, false, cycle)?;
            }
        }
        for cycle in 1..=measured_cycles {
            edit_and_undo(d, &baseline, kind, true, cycle)?;
        }
        let mut samples = samples(d, kind.metric());
        samples.sort_by(f64::total_cmp);
        let p95 = samples
            .get((samples.len() * 95).div_ceil(100).saturating_sub(1))
            .copied();
        d.check(
            &format!(
                "{} has one measured picture completion per edit",
                kind.name()
            ),
            samples.len() == measured_cycles,
            json!(measured_cycles),
            json!({"samples":samples.len(),"p95_ms":p95}),
        )?;
    }
    d.capture("Original restored after measured Repeat and Hold edits")?;
    // A latency miss does not prevent collecting the other independent budget.
    // Structural failures above still stop immediately with their exact state.
    let mut failure = None;
    if d.options.mode == RunMode::Performance {
        for kind in [EditKind::Repeat, EditKind::Hold] {
            let mut values = samples(d, kind.metric());
            values.sort_by(f64::total_cmp);
            let p95 = values
                .get((values.len() * 95).div_ceil(100).saturating_sub(1))
                .copied();
            if let Err(error) = d.check(
                &format!("{} p95 meets its preview feedback budget", kind.name()),
                values.len() >= 40 && p95.is_some_and(|value| value < kind.budget_ms()),
                json!({"minimum_samples":40,"p95_below_ms":kind.budget_ms()}),
                json!({"samples":values.len(),"p95_ms":p95}),
            ) {
                failure.get_or_insert(error);
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn edit_and_undo(
    d: &mut Driver<'_>,
    baseline: &Baseline,
    kind: EditKind,
    measured: bool,
    cycle: usize,
) -> Result<(), String> {
    let before = d.revision();
    let trace_start = d.report.steps.len();
    let picture_start = sample_count(d, "input_to_picture_complete_ms");
    let commit_start = sample_count(d, "input_to_commit_ms");
    match kind {
        EditKind::Repeat => d.chord(&[egui::Key::R, egui::Key::R])?,
        EditKind::Hold => d.command("hold 11f")?,
    }
    d.changed(&before)?;
    let picture = exactly_one_sample(d, "input_to_picture_complete_ms", picture_start)?;
    let commit = exactly_one_sample(d, "input_to_commit_ms", commit_start)?;
    let stage_count = |name| {
        d.report.steps[trace_start..]
            .iter()
            .flat_map(|step| step.semantic["stages"].as_array().into_iter().flatten())
            .filter(|event| event["stage"] == name)
            .count()
    };
    let admitted = stage_count("command_admitted");
    let committed = stage_count("command_committed");
    let rejected = stage_count("command_rejected");
    d.check(
        &format!("{} completes one admitted edit and one picture", kind.name()),
        admitted == 1 && committed == 1 && rejected == 0 && d.revision() != before,
        json!({"admitted":1,"committed":1,"rejected":0,"picture_completions":1}),
        json!({"admitted":admitted,"committed":committed,"rejected":rejected,"revision":d.revision(),"input_to_commit_ms":commit,"input_to_picture_complete_ms":picture}),
    )?;
    assert_edit(d, baseline, kind)?;
    if measured {
        d.metric(kind.metric(), picture, SampleOutcome::Completed);
        d.metric(kind.commit_metric(), commit, SampleOutcome::Completed);
    }
    if measured && d.options.mode == RunMode::Visual {
        d.capture(&format!(
            "{} committed and displayed, cycle {cycle}",
            kind.name()
        ))?;
    }
    // This input begins only after collecting the edit's exact timing subset.
    // Undo uses the same UI/service path and receives a fresh revision.
    let edit_revision = d.revision();
    d.key(egui::Key::U)?;
    d.changed(&edit_revision)?;
    let restored = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Undo lost the workspace")?;
    d.check(
        &format!("One undo after {} restores the exact Original baseline", kind.name()),
        restored.document.nodes() == &baseline.nodes
            && root_children(restored) == std::slice::from_ref(&baseline.original)
            && d.app().beat_rows.len() == 1
            && d.app().sequence_length() == baseline.duration
            && d.app().selected_beat.as_ref() == Some(&baseline.original)
            && d.app().sequence_cursor == 0
            && d.revision() != before,
        json!({"root":baseline.original,"root_beats":1,"duration":baseline.duration,"cursor":0,"authored_nodes":"identical","revision":"fresh"}),
        d.snapshot(),
    )?;
    if measured && d.options.mode == RunMode::Visual {
        d.capture(&format!(
            "Original restored after {}, cycle {cycle}",
            kind.name()
        ))?;
    }
    Ok(())
}

fn assert_edit(d: &mut Driver<'_>, baseline: &Baseline, kind: EditKind) -> Result<(), String> {
    let app = d.app();
    let workspace = app
        .workspace
        .as_ref()
        .ok_or("Committed edit lost its workspace")?;
    let selected = app
        .selected_beat
        .as_ref()
        .and_then(|id| workspace.document.nodes().get(id));
    let (valid, expected_duration, expected_structure) = match kind {
        EditKind::Repeat => (
            app.beat_rows.len() == 1
                && selected.is_some_and(|node| {
                    matches!(&node.kind, NodeKind::Repeat { child, iterations, gap }
                    if child == &baseline.original && iterations.len() == 2 && gap.is_none())
                }),
            baseline
                .duration
                .checked_mul(2)
                .ok_or("Repeat duration overflow")?,
            "one Repeat with two total plays of the Original",
        ),
        EditKind::Hold => (
            app.beat_rows.len() == 2
                && selected.is_some_and(|node| {
                    matches!(&node.kind, NodeKind::Hold { recipe }
                    if recipe.duration.frames() == 11
                    && matches!(recipe.video, HoldVideo::Freeze { .. })
                    && recipe.audio == HoldAudio::Silence)
                }),
            baseline
                .duration
                .checked_add(11)
                .ok_or("Hold duration overflow")?,
            "selected 11-frame Freeze/Silence Hold before the unchanged Original",
        ),
    };
    let original_unchanged = workspace.document.nodes().get(&baseline.original)
        == baseline.nodes.get(&baseline.original);
    d.check(
        &format!("{} displays its exact committed structural result", kind.name()),
        valid
            && original_unchanged
            && app.sequence_length() == expected_duration
            && app.presentation.has_displayed()
            && !app.presentation.loading()
            && !app.presentation.needs_render(),
        json!({"structure":expected_structure,"duration":expected_duration,"original":"unchanged","picture":"submitted"}),
        d.snapshot(),
    )
}

fn sample_count(d: &Driver<'_>, name: &str) -> usize {
    d.report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .map_or(0, |metric| metric.samples.len())
}

fn exactly_one_sample(d: &Driver<'_>, name: &str, start: usize) -> Result<f64, String> {
    let samples = d
        .report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .and_then(|metric| metric.samples.get(start..))
        .ok_or_else(|| format!("Edit has no {name} timing after sample {start}"))?;
    match samples {
        [sample] if sample.outcome == SampleOutcome::Completed && sample.elapsed_ms.is_finite() => {
            Ok(sample.elapsed_ms)
        }
        _ => Err(format!(
            "Edit expected exactly one completed {name} sample, observed {samples:?}"
        )),
    }
}

fn samples(d: &Driver<'_>, name: &str) -> Vec<f64> {
    d.report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .map(|metric| {
            metric
                .samples
                .iter()
                .filter(|sample| sample.outcome == SampleOutcome::Completed)
                .map(|sample| sample.elapsed_ms)
                .collect()
        })
        .unwrap_or_default()
}
