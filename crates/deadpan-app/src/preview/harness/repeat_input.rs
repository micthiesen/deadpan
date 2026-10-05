//! Real SQLite wraps with deterministic delivery boundaries. Holding the UI
//! mailbox never pauses the writer or fabricates its completion.

use egui::Key;

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No Repeat workspace")?
        .document
        .clone();
    let initial_duration = d.app().sequence_length();
    let original = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No initial Repeat target")?;
    let initial_revision = d.revision();
    let burst_start = d.report.steps.len();
    burst(d, 8)?;
    super::scenarios::footer_anchored(d, "First Repeat burst after command exit")?;
    visible_status(d, "7 Repeats waiting")?;
    d.capture("Explicit Repeat wraps waiting")?;
    d.changed(&initial_revision)?;
    let counts = [
        "command_admitted",
        "repeat_queued",
        "repeat_dequeued",
        "command_rejected",
        "command_committed",
    ]
    .map(|stage| stage_count(d, burst_start, stage));
    let revisions: std::collections::BTreeSet<_> = d.report.steps[burst_start..]
        .iter()
        .filter_map(|step| step.semantic["revision"].as_str())
        .filter(|revision| *revision != initial_revision)
        .map(str::to_owned)
        .collect();
    d.check(
        "Eight rapid wraps commit separately without busy rejection",
        counts == [8, 7, 7, 0, 8] && revisions.len() == 8
            && d.app().sequence_length() == initial_duration * 256,
        json!({"admitted":8,"queued":7,"dequeued":7,"rejected":0,"committed":8,"revisions":8,"duration":initial_duration * 256}),
        json!({"counts":counts,"revisions":revisions,"duration":d.app().sequence_length()}),
    )?;
    let mut inner = d.app().selected_beat.clone().ok_or("No Repeat selection")?;
    let workspace = d.app().workspace.as_ref().ok_or("No Repeat result")?;
    for _ in 0..8 {
        let Some(NodeKind::Repeat {
            child,
            iterations,
            gap: None,
            ..
        }) = workspace
            .document
            .nodes()
            .get(&inner)
            .map(|node| &node.kind)
        else {
            return Err("Rapid wraps did not produce eight nested Repeats".into());
        };
        if iterations.len() != 2 {
            return Err("Rapid wrap changed its total plays".into());
        }
        inner = child.clone();
    }
    d.check(
        "Eight wraps retain the original inner beat",
        inner == original,
        json!(original),
        json!(inner),
    )?;
    undo(d, &baseline, initial_duration, 8)?;

    // A completion between the two letters of rr must not eat the first r.
    let before = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    burst(d, 2)?;
    d.key(Key::R)?;
    d.wait_for("Writer finished while its UI completion is held", |app| {
        !app.service.is_busy()
    })?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&before)?;
    d.check(
        "Own Repeat completion preserves the next partial rr",
        d.app().bindings.pending() == "r",
        json!("r"),
        json!(d.app().bindings.pending()),
    )?;
    let before = d.revision();
    d.key(Key::R)?;
    d.changed(&before)?;
    d.check(
        "Completing the retained prefix adds a third wrap after the queued continuation",
        d.app().sequence_length() == initial_duration * 8,
        json!(initial_duration * 8),
        json!(d.app().sequence_length()),
    )?;
    undo(d, &baseline, initial_duration, 3)?;

    // The service may already be idle, but only adopting its matched completion
    // permits continuation. A real pointer context change wins first.
    let before = d.revision();
    let start = d.report.steps.len();
    d.app_mut().feedback.hold_project_updates = true;
    burst(d, 3)?;
    d.wait_for("Writer completion awaits pointer context decision", |app| {
        !app.service.is_busy()
    })?;
    d.click("Original")?;
    d.check(
        "Pointer context change cancels two waiting wraps",
        !d.app().repeat_queue.active()
            && d.app().repeat_queue.waiting() == 0
            && stage_count(d, start, "repeat_cancelled") == 2,
        json!({"active":false,"waiting":0,"cancelled":2}),
        d.snapshot(),
    )?;
    // Pointer actions run after the notice panel, so inspect exactly the next paint.
    d.step("Paint pointer cancellation notice", true)?;
    visible_status(d, "Cancelled 2 queued Repeats")?;
    d.capture("Pointer cancellation retains the submitted edit")?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&before)?;
    d.check("Only the submitted wrap finishes after context cancellation", stage_count(d, start, "command_admitted") == 1 && d.app().sequence_length() == initial_duration * 2, json!({"admitted":1,"duration":initial_duration * 2}), json!({"admitted":stage_count(d, start, "command_admitted"),"duration":d.app().sequence_length()}))?;
    undo(d, &baseline, initial_duration, 1)?;

    let before = d.revision();
    let start = d.report.steps.len();
    d.app_mut().feedback.hold_project_updates = true;
    burst(d, 21)?;
    d.check(
        "Repeat queue is bounded and explicitly rejects overflow",
        d.app().repeat_queue.waiting() == 16 && stage_count(d, start, "command_rejected") == 4,
        json!({"waiting":16,"rejected":4}),
        d.snapshot(),
    )?;
    visible_status(d, "4 Repeat intents not queued")?;
    d.capture("Bounded Repeat queue explains overflow")?;
    d.wait_for(
        "Overflow writer finishes before its completion is delivered",
        |app| !app.service.is_busy(),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Repeat resize has no root viewport")?
            .inner_rect = Some(rect);
        d.capture(&format!(
            "Repeat overflow on first resize paint at {width}x{height}"
        ))?;
        visible_status(d, "4 Repeat intents not queued")?;
        visible_status(d, "Working")?;
        d.check(
            "Queued wraps are not reported as saved",
            scenarios::text_paint_visibility(d, "Saved").is_empty(),
            json!("Working until the queue completes"),
            d.snapshot(),
        )?;
    }
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels all sixteen waiting wraps",
        !d.app().repeat_queue.active() && stage_count(d, start, "repeat_cancelled") == 16,
        json!({"active":false,"cancelled":16}),
        d.snapshot(),
    )?;
    visible_status(d, "Cancelled 16 queued Repeats")?;
    d.capture("Escape cancels only waiting wraps")?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&before)?;
    d.check("Escape never rolls back the already submitted wrap", stage_count(d, start, "command_admitted") == 1 && d.app().sequence_length() == initial_duration * 2, json!({"admitted":1,"duration":initial_duration * 2}), json!({"admitted":stage_count(d, start, "command_admitted"),"duration":d.app().sequence_length()}))?;
    undo(d, &baseline, initial_duration, 1)?;

    // Holding the writer's mailbox makes its busy window deterministic. A file
    // action during that window must refuse visibly instead of vanishing.
    let before = d.revision();
    d.app().service.hold_requests_for_check(true);
    burst(d, 1)?;
    d.check(
        "A held wrap keeps the project command slot busy",
        d.app().service.is_busy() && d.revision() == before,
        json!({"busy":true,"revision":before}),
        d.snapshot(),
    )?;
    let dialogs = std::mem::replace(
        &mut d.app_mut().dialogs,
        Dialogs::scripted(vec![(DialogKind::OpenProject, None)]),
    );
    d.key_modified(Key::O, egui::Modifiers::COMMAND)?;
    let refused = "Open project did not start because another project command is still in progress";
    d.check(
        "Cmd+O during a busy save is refused with a visible reason",
        !d.app().dialogs.is_open()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains(refused)),
        json!({"dialog_open":false,"error":refused}),
        d.snapshot(),
    )?;
    d.step("Paint busy Open refusal", true)?;
    visible_status(d, "Open project did not start")?;
    d.capture("Busy writer refuses Open visibly")?;
    d.app_mut().dialogs = dialogs;
    d.app().service.hold_requests_for_check(false);
    d.changed(&before)?;
    d.check(
        "The held wrap still commits after the refused Open",
        d.app().sequence_length() == initial_duration * 2,
        json!(initial_duration * 2),
        json!(d.app().sequence_length()),
    )?;
    undo(d, &baseline, initial_duration, 1)?;
    d.report.skipped.push("Repeat boundary checks delay delivery of real SQLite updates; the writer and production event router remain live. Physical key delivery is not exercised.".into());
    Ok(())
}

fn burst(d: &mut Driver<'_>, count: usize) -> Result<(), String> {
    let events = (0..count)
        .flat_map(|_| {
            [
                key_event(Key::R, egui::Modifiers::NONE, true),
                key_event(Key::R, egui::Modifiers::NONE, false),
                key_event(Key::R, egui::Modifiers::NONE, true),
                key_event(Key::R, egui::Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events(
        &format!("{count} rapid Repeat wraps in one input batch"),
        events,
    )
}

fn stage_count(d: &Driver<'_>, start: usize, name: &str) -> usize {
    d.report.steps[start..]
        .iter()
        .flat_map(|step| step.semantic["stages"].as_array().into_iter().flatten())
        .filter(|event| event["stage"] == name)
        .count()
}

fn visible_status(d: &mut Driver<'_>, needle: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, needle);
    d.check(
        &format!("Repeat status is fully painted: {needle}"),
        !paint.is_empty() && paint.iter().all(|text| text["fully_visible"] == true),
        json!("visible inside viewport and clip"),
        json!(paint),
    )
}

fn undo(
    d: &mut Driver<'_>,
    baseline: &deadpan_core::ProjectDocument,
    duration: u64,
    count: u32,
) -> Result<(), String> {
    for remaining in (0..count).rev() {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
        let expected = duration * 2_u64.pow(remaining);
        d.check(
            "One undo removes exactly one explicit wrap",
            d.app().sequence_length() == expected,
            json!(expected),
            json!(d.app().sequence_length()),
        )?;
    }
    let document = &d
        .app()
        .workspace
        .as_ref()
        .ok_or("Undo lost workspace")?
        .document;
    let mut restored =
        serde_json::to_value(document.as_ref()).map_err(|error| error.to_string())?;
    let mut expected = serde_json::to_value(baseline).map_err(|error| error.to_string())?;
    // Undo receives a fresh revision; every other authored field must match,
    // including marks, sparse overrides, media bindings and presentation basis.
    restored["revision_id"] = Value::Null;
    expected["revision_id"] = Value::Null;
    d.check(
        "Undo restores the exact authored Original baseline under a fresh revision",
        restored == expected
            && document.revision_id() != baseline.revision_id()
            && d.app().sequence_cursor == 0,
        json!({"nodes":"identical","revision":"fresh","cursor":0}),
        d.snapshot(),
    )
}
