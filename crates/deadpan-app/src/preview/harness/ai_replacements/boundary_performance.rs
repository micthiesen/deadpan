//! Isolated Enter-to-commit/picture samples after a real Source Slip preview.

use super::*;
use deadpan_core::{NodeId, ProjectDocument};
use deadpan_store::{AccessMode, ProjectStore};

const WARMUPS: usize = 4;
const MEASURED: usize = 40;
const COMMIT_METRIC: &str = "boundary_source_input_to_commit_ms";
const PICTURE_METRIC: &str = "boundary_source_input_to_picture_complete_ms";

pub(super) fn run(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    changed: &ProjectDocument,
    hold: &NodeId,
    source: &NodeId,
) -> Result<(), String> {
    d.report.skipped.push("Boundary edit timings use 4 warmup and 40 measured Source Slip +1f commits. Each sample starts at Apply Enter after the proposed picture is ready and ends at the committed revision or offscreen GPU completion. Preview preparation, replacement preflight waits, durability checks and Undo are excluded. Distributions are reported without a new latency threshold.".into());
    // Recovery above discarded an operational row, not the authored Slip.
    // One ordinary Undo restores the accepted baseline before any warmup.
    undo_to_accepted(d, before, hold)?;
    for cycle in 1..=WARMUPS + MEASURED {
        edit_and_undo(d, before, changed, hold, source, cycle > WARMUPS, cycle)?;
    }
    for metric in [COMMIT_METRIC, PICTURE_METRIC] {
        d.check(
            &format!("{metric} contains exactly 40 measured completions"),
            sample_count(d, metric) == MEASURED,
            json!({"warmups_excluded":WARMUPS,"measured":MEASURED}),
            json!({"measured":sample_count(d, metric)}),
        )?;
    }
    Ok(())
}

fn edit_and_undo(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    expected: &ProjectDocument,
    hold: &NodeId,
    source: &NodeId,
    measured: bool,
    cycle: usize,
) -> Result<(), String> {
    d.click("Current group beat outline pane")?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    d.check(
        "Boundary timing starts at the same neighboring Source",
        d.app().selected_beat.as_ref() == Some(source) && d.app().sequence_cursor == 0,
        json!({"source":source,"cursor":0}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.command("slip +1f")?;
    d.wait_for(
        "Boundary timing proposal is ready before measurement",
        |app| {
            !app.service.is_busy()
                && app
                    .slip
                    .as_ref()
                    .is_some_and(|draft| draft.ready_for_check())
                && !app.presentation.loading()
                && !app.presentation.needs_render()
        },
    )?;
    let resolution = &d
        .app()
        .slip
        .as_ref()
        .and_then(|draft| draft.prepared_for_check())
        .ok_or("Boundary timing has no prepared Slip")?
        .resolution;
    let physical_source = resolution.physical_source.clone();
    d.check(
        "Boundary timing measures a real one-frame Source change",
        resolution.applied_delta_frames == 1
            && resolution.before != resolution.after
            && resolution.target == *source
            && d.revision() == revision,
        json!({"applied_delta_frames":1,"source":source,"revision":revision}),
        json!(resolution),
    )?;
    d.click("Slip preview keyboard controls")?;
    // All proposal work and focus input finish before these exact sample cursors.
    let trace_start = d.report.steps.len();
    let commit_start = sample_count(d, "input_to_commit_ms");
    let picture_start = sample_count(d, "input_to_picture_complete_ms");
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    let commit = exactly_one_sample(d, "input_to_commit_ms", commit_start)?;
    let picture = exactly_one_sample(d, "input_to_picture_complete_ms", picture_start)?;
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
        &format!("Boundary cycle {cycle} completes one admitted edit and one picture"),
        admitted == 1 && committed == 1 && rejected == 0 && d.revision() != revision,
        json!({"admitted":1,"committed":1,"rejected":0,"picture_completions":1}),
        json!({"admitted":admitted,"committed":committed,"rejected":rejected,"input_to_commit_ms":commit,"input_to_picture_complete_ms":picture}),
    )?;
    if measured {
        d.metric(COMMIT_METRIC, commit, SampleOutcome::Completed);
        d.metric(PICTURE_METRIC, picture, SampleOutcome::Completed);
    }

    // The measured subset is closed before model availability or store checks.
    d.wait_for("Boundary timing replacement reaches unavailable", |app| {
        app.ai_preparations()
            .iter()
            .any(|item| &item.target.node == hold && item.state == PreparationState::Unavailable)
    })?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No boundary timing project")?;
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    let preparations = store
        .generation_preparations(None, 2)
        .map_err(|error| error.to_string())?;
    let NodeKind::Hold { recipe } = &before.nodes()[hold].kind else {
        return Err("Boundary timing baseline lost its Hold".into());
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        return Err("Boundary timing baseline lost its accepted pictures".into());
    };
    let durable_replacement = matches!(preparations.as_slice(), [item]
        if &item.target.node == hold
            && item.current_revision == *workspace.document.revision_id()
            && item.duration == recipe.duration
            && item.state == PreparationState::Unavailable
            && matches!(&item.origin,
                deadpan_store::generation_preparations::PreparationOrigin::AcceptedBoundary { accepted: artifact, .. }
                    if artifact.as_ref() == &accepted.artifact)
            && matches!(item.intent.cause,
                deadpan_store::generation_intents::IntentCause::SourceBoundaryChanged));
    d.check(
        "Boundary timing commit atomically preserves the exact slipped Source and fallback",
        workspace.document.nodes() == expected.nodes()
            && workspace.document.audio_bindings() == expected.audio_bindings()
            && workspace.document.nodes()[&physical_source] != before.nodes()[&physical_source]
            && durable_replacement,
        json!({"authored_nodes":"exact Slip and fallback","replacement":"one captured accepted artifact"}),
        json!({"revision":d.revision(),"replacement":durable_replacement,"preparations":preparations}),
    )?;
    drop(store);
    undo_to_accepted(d, before, hold)
}

fn undo_to_accepted(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    hold: &NodeId,
) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    d.wait_for("Boundary timing Undo retires pending preparation", |app| {
        app.ai_preparations().is_empty()
    })?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Boundary Undo lost the project")?;
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    let preparations = store
        .generation_preparations(None, 1)
        .map_err(|error| error.to_string())?;
    let intents = store
        .generation_intents(None, 1)
        .map_err(|error| error.to_string())?;
    d.check(
        "One boundary timing Undo restores exact nodes, accepted artifact and no pending work",
        workspace.document.nodes() == before.nodes()
            && workspace.document.audio_bindings() == before.audio_bindings()
            && workspace.document.nodes()[hold] == before.nodes()[hold]
            && preparations.is_empty()
            && intents.is_empty(),
        json!({"nodes":"original","accepted_artifact":"exact","preparations":0,"intents":0}),
        json!({"revision":d.revision(),"preparations":preparations.len(),"intents":intents.len()}),
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
        .ok_or_else(|| format!("Boundary edit has no {name} timing after sample {start}"))?;
    match samples {
        [sample] if sample.outcome == SampleOutcome::Completed && sample.elapsed_ms.is_finite() => {
            Ok(sample.elapsed_ms)
        }
        _ => Err(format!(
            "Boundary edit expected exactly one completed {name} sample, observed {samples:?}"
        )),
    }
}
