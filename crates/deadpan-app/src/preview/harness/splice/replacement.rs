//! Exact Edit selection, explicit Replace, native event ownership and history.

use deadpan_core::{Command, CommandRequest, FrameRange, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    selection(d)?;
    refine_and_cancel(d)?;
    for removed in [7, 14, 30] {
        replace_commit(d, removed)?;
    }
    for before in [false, true] {
        fast_paste(d, before)?;
    }
    command_capture(d)?;
    nested(d)?;
    Ok(())
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn select(d: &mut Driver<'_>, start: u64, end: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, start - d.app().scope_start, true)?;
    d.key(Key::V)?;
    motion(d, end.abs_diff(start), end > start)?;
    d.key(Key::V)?;
    d.settled()
}

fn motion(d: &mut Driver<'_>, frames: u64, forward: bool) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().bytes() {
        d.key(match digit {
            b'0' => Key::Num0,
            b'1' => Key::Num1,
            b'2' => Key::Num2,
            b'3' => Key::Num3,
            b'4' => Key::Num4,
            b'5' => Key::Num5,
            b'6' => Key::Num6,
            b'7' => Key::Num7,
            b'8' => Key::Num8,
            b'9' => Key::Num9,
            _ => unreachable!(),
        })?;
    }
    d.key(if forward { Key::L } else { Key::H })
}

fn selection(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    let revision = d.revision();
    select(d, 30, 60)?;
    let selected = d.app().edit_range.clone();
    d.key(Key::L)?;
    d.check(
        "A finished Edit selection retains its exact half-open bounds while the cursor moves",
        d.app().selected_edit_range() == Some(range(30, 60))
            && d.app().edit_range == selected
            && d.app().sequence_cursor == 61
            && d.revision() == revision,
        json!({"range":[30,60],"cursor":61,"saved_revision":revision}),
        d.snapshot(),
    )?;
    paint_text(d, "Edit [30..60)")?;
    d.check("The selected temporal part of its beat is painted distinctly",
        d.harness.output().shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == style::LAVENDER && (rect.rect.height() - 6.0).abs() < 0.1 && shape.clip_rect.contains_rect(rect.rect))),
        json!("visible selected interval bar"), d.snapshot())?;
    d.command("source")?;
    d.key(Key::L)?;
    d.command("sequence")?;
    d.check(
        "Original browsing freezes and retains the independent Edit range and copied register",
        d.app().selected_edit_range() == Some(range(30, 60))
            && !d.app().edit_range.active
            && copied(d) == Some(10..24),
        json!({"edit_range":[30,60],"copied":[10,24]}),
        d.snapshot(),
    )?;
    select(d, 60, 30)?;
    d.check(
        "Reverse keyboard selection normalizes to the same half-open Edit interval",
        d.app().selected_edit_range() == Some(range(30, 60)) && d.app().sequence_cursor == 30,
        json!({"range":[30,60],"cursor":30}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    d.events(
        "Deliver Visual plus a duplicate-digit count in one native batch",
        [Key::V, Key::Num1, Key::Num1, Key::L]
            .into_iter()
            .flat_map(|key| {
                [
                    key_event(key, Modifiers::NONE, true),
                    key_event(key, Modifiers::NONE, false),
                ]
            })
            .collect(),
    )?;
    d.check(
        "One native batch extends Edit Visual selection by its complete count",
        d.app().selected_edit_range() == Some(range(0, 11)) && d.app().edit_range.active,
        json!({"range":[0,11],"active":true}),
        d.snapshot(),
    )?;
    d.key(Key::J)?;
    d.check(
        "Beat motion extends the active Edit range to its next boundary",
        d.app().selected_edit_range() == Some(range(0, 120)),
        json!([0, 120]),
        d.snapshot(),
    )?;
    d.key(Key::K)?;
    d.check(
        "Reverse beat motion returns the active range to its anchor",
        d.app().selected_edit_range().is_none() && d.app().edit_range.active,
        json!("empty active selection"),
        d.snapshot(),
    )?;
    d.key_modified(Key::G, Modifiers::SHIFT)?;
    d.check(
        "G extends to the enclosing Sequence end",
        d.app().selected_edit_range() == Some(range(0, 120)),
        json!([0, 120]),
        d.snapshot(),
    )?;
    d.chord(&[Key::G, Key::G])?;
    d.check(
        "gg returns an active selection to an empty anchor without committing",
        d.app().selected_edit_range().is_none()
            && d.app().edit_range.active
            && d.revision() == revision,
        json!({"empty":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    active_selection_audition(d)?;
    select(d, 30, 60)
}

fn active_selection_audition(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num3,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num3,
        Key::Num0,
        Key::L,
    ])?;
    let selection = d.app().edit_range.clone();
    let rate = document(d)?.presentation_basis().frame_rate;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No selected Edit audition")?;
    let start = rate
        .audio_boundary(ProjectFrame(30))
        .map_err(|error| error.to_string())?
        .0
        - d.app().audition_context.lead.0;
    let end = rate
        .audio_boundary(ProjectFrame(60))
        .map_err(|error| error.to_string())?
        .0
        + d.app().audition_context.follow.0;
    d.check(
        "Edit selection audition uses the complete selected interval with exact context",
        run.window().start() == AudioSample(start) && run.window().end() == AudioSample(end),
        json!([start, end]),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(start).map_err(|error| error.to_string())?;
    let update = delivery(d, Phase::Playing, AudioSample(end - 137), generation)?;
    inject(
        d,
        update,
        "Advance delivery beyond the active Edit selection",
    )?;
    d.check(
        "Playback advances the cursor without extending active Visual selection",
        d.app().edit_range == selection && d.app().selected_edit_range() == Some(range(30, 60)),
        json!({"range":[30,60],"active":true}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.app_mut().feedback.simulate_playback = simulated;
    d.key(Key::Escape)
}

fn refine_and_cancel(d: &mut Driver<'_>) -> Result<(), String> {
    let entry = editor(d);
    let selected = d.app().edit_range.clone();
    let saved = document(d)?.clone();
    d.command("splice")?;
    wait_ready(d)?;
    let insertion = draft(d)?.proposal_for_check().destination.clone();
    d.check(
        "Place slice defaults to Insert even when an Edit range is selected",
        !matches!(insertion, Destination::Replace { .. }) && prepared(d)?.removed.is_none(),
        json!("Insert is explicit default"),
        state(d),
    )?;
    d.click("Replace selection · r")?;
    wait_ready(d)?;
    d.check(
        "Pointer Replace selection toggles the operation instead of hitting the heading focus region",
        draft(d)?.proposal_for_check().destination == Destination::Replace { range: range(30,60) }
            && prepared(d)?.removed == Some(range(30,60)) && *document(d)? == saved,
        json!({"operation":"Replace","removed":[30,60],"saved_unchanged":true}), state(d),
    )?;
    paint_text(d, "UNSAVED · Replace")?;
    d.check(
        "Heading keyboard focus excludes the adjacent native operation button",
        !d.rect(HEADING)?.intersects(d.rect("Insert instead · r")?),
        json!("disjoint pointer targets"),
        d.snapshot(),
    )?;
    d.click("Insert instead · r")?;
    wait_ready(d)?;
    d.check(
        "Pointer Insert instead restores the captured Insert destination without saving",
        draft(d)?.proposal_for_check().destination == insertion
            && prepared(d)?.removed.is_none()
            && *document(d)? == saved,
        json!({"operation":"Insert","saved_unchanged":true}),
        state(d),
    )?;
    paint_text(d, "UNSAVED · Insert")?;
    d.click(HEADING)?;
    expect_focus(d, HEADING)?;
    d.events(
        "Switch operation and refine Original In in one native key batch",
        [Key::R, Key::I, Key::Num1, Key::L]
            .into_iter()
            .flat_map(|key| {
                [
                    key_event(key, Modifiers::NONE, true),
                    key_event(key, Modifiers::NONE, false),
                ]
            })
            .collect(),
    )?;
    wait_ready(d)?;
    d.check("Replace fixes the removed Edit interval while local In refinement changes only the inserted slice",
        draft(d)?.proposal_for_check().destination == Destination::Replace { range: range(30,60) }
            && draft(d)?.proposal_for_check().source.boundaries()? == (11..24)
            && prepared(d)?.range == range(30,43) && prepared(d)?.removed == Some(range(30,60)) && copied(d) == Some(10..24),
        json!({"removed":[30,60],"inserted":[30,43],"delta":-17,"copied":[10,24]}), state(d))?;
    let identity = draft(d)?.proposal_for_check().id.clone();
    d.chord(&[Key::D, Key::J, Key::K])?;
    d.check("Destination and seam keys explain the fixed replacement without changing its hidden Insert target",
        draft(d)?.proposal_for_check().id == identity && prepared(d)?.removed == Some(range(30,60)),
        json!("unchanged proposal identity and removed interval"), state(d))?;
    d.key(Key::R)?;
    wait_ready(d)?;
    d.check("Returning to Insert restores the exact entry destination with local source refinement retained",
        draft(d)?.proposal_for_check().destination == insertion && draft(d)?.proposal_for_check().source.boundaries()? == (11..24),
        json!(format!("{insertion:?}")), state(d))?;
    d.key(Key::R)?;
    wait_ready(d)?;
    d.key(Key::F)?;
    wait_picture(d, 11, "Showing proposed edit frame 31")?;
    replacement_layout(d)?;
    focus_with_tab(d, "Insert instead · r")?;
    d.key(Key::R)?;
    d.check(
        "Plain r does not activate the operation while a native button owns focus",
        matches!(
            draft(d)?.proposal_for_check().destination,
            Destination::Replace { .. }
        ),
        json!("native focus owns action"),
        state(d),
    )?;
    d.key(Key::Enter)?;
    wait_ready(d)?;
    d.check(
        "Native Enter on the operation control returns to Insert without committing",
        draft(d)?.proposal_for_check().destination == insertion && *document(d)? == saved,
        json!("Insert, saved unchanged"),
        state(d),
    )?;
    focus_with_tab(d, HEADING)?;
    d.events(
        "IME composition and r arrive together",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "r".into(),
                active_range_chars: Some(0..1),
            }),
            key_event(Key::R, Modifiers::NONE, true),
            key_event(Key::R, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME owns the operation key during composition",
        d.app().ime_composing && draft(d)?.proposal_for_check().destination == insertion,
        json!("no operation change"),
        state(d),
    )?;
    d.events(
        "IME commit and r arrive together",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("r".into())),
            key_event(Key::R, Modifiers::NONE, true),
            key_event(Key::R, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "The composition commit batch cannot toggle Replace",
        !d.app().ime_composing && draft(d)?.proposal_for_check().destination == insertion,
        json!("Insert retained"),
        state(d),
    )?;
    d.key(Key::R)?;
    wait_ready(d)?;
    cancel(d)?;
    d.check(
        "Cancel restores the complete entry Edit selection, cursors, register and saved document",
        d.app().edit_range == selected
            && editor(d) == entry
            && *document(d)? == saved
            && copied(d) == Some(10..24),
        json!({"editor":entry,"revision":saved.revision_id(),"selection_restored":true}),
        d.snapshot(),
    )
}

fn replacement_layout(d: &mut Driver<'_>) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing replacement viewport")?
            .inner_rect = Some(rect);
        d.step("Paint explicit replacement at the requested viewport", true)?;
        wait_endpoints(d, 11, 24)?;
        for text in [
            "UNSAVED · Replace",
            "Fixed Edit [30..60)",
            "Remove [30..60)",
            "Insert [30..43)",
            "-17 f",
        ] {
            paint_text(d, text)?;
        }
        endpoint_painted(d, "First included Original picture, frame 12")?;
        endpoint_painted(d, "Last included Original picture, frame 24")?;
        viewer_painted(d)?;
        d.check(
            "Replace omits destination and seam controls while exposing its retained removal",
            d.rect("Previous seam  k").is_err()
                && d.rect("Next seam  j").is_err()
                && d.rect("Insert instead · r").is_ok(),
            json!("fixed removed interval, source refinement, picture inspection"),
            state(d),
        )?;
        d.capture(&format!(
            "Linked replacement with removed interval and signed delta at {width} by {height}"
        ))?;
    }
    Ok(())
}

fn replace_commit(d: &mut Driver<'_>, removed: u64) -> Result<(), String> {
    select(d, 30, 30 + removed)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::R)?;
    wait_ready(d)?;
    let proposed = prepared(d)?;
    d.check(
        "Shorter, equal and longer replacements retain their exact inserted and removed intervals",
        proposed.range == range(30, 44)
            && proposed.removed == Some(range(30, 30 + removed as i64))
            && proposed.plan.duration().frames() == 134 - removed as i64,
        json!({"removed":[30,30+removed],"inserted":[30,44],"total":134-removed}),
        state(d),
    )?;
    d.key(Key::F)?;
    d.key(Key::H)?;
    wait_picture(d, 29, "Showing proposed edit frame 30")?;
    d.key(Key::L)?;
    wait_picture(d, 10, "Showing proposed edit frame 31")?;
    motion(d, 13, true)?;
    wait_picture(d, 23, "Showing proposed edit frame 44")?;
    d.key(Key::L)?;
    wait_picture(d, 30 + removed, "Showing proposed edit frame 45")?;
    d.key(Key::B)?;
    wait_picture(
        d,
        30 + removed,
        &format!("Showing sequence frame {}", 31 + removed),
    )?;
    d.key(Key::B)?;
    wait_picture(d, 30 + removed, "Showing proposed edit frame 45")?;
    if removed == 30 {
        comparison_audition(d, &proposed)?;
    }
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check("Enter commits the exact replacement preview once and selects the inserted result",
        *document(d)? == *proposed.snapshot.document && d.app().selected_beat.as_ref() == Some(&proposed.node) && d.app().sequence_cursor == 30 && d.app().selected_edit_range().is_none(),
        json!({"revision":proposed.snapshot.document.revision_id(),"cursor":30,"range_cleared":true}), d.snapshot())?;
    undo_restores(d, &saved)
}

fn comparison_audition(d: &mut Driver<'_>, proposed: &Prepared) -> Result<(), String> {
    let selected = d.app().edit_range.clone();
    let rate = proposed.snapshot.document.presentation_basis().frame_rate;
    let boundary = |frame: i64| {
        rate.audio_boundary(ProjectFrame(frame))
            .map_err(|error| error.to_string())
    };
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let run = d.app().transport.as_ref().ok_or("No replacement loop")?;
    let window = *run.window();
    d.check("Proposed audition encloses exactly the inserted interval and shared context",
        window.start() == AudioSample(boundary(30)?.0-d.app().audition_context.lead.0) && window.end() == AudioSample(boundary(44)?.0+d.app().audition_context.follow.0),
        json!({"start":boundary(30)?.0-d.app().audition_context.lead.0,"end":boundary(44)?.0+d.app().audition_context.follow.0}), d.snapshot())?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed
        .restart(window.start().0)
        .map_err(|error| error.to_string())?;
    let heard = AudioSample(boundary(44)?.0 + 137);
    let update = delivery(d, Phase::Playing, heard, generation)?;
    inject(
        d,
        update,
        "Deliver a suffix sample 137 samples after the proposed right join",
    )?;
    d.key(Key::B)?;
    let expected = AudioSample(boundary(60)?.0 + 137);
    d.check("Before audition uses the removed interval and maps the exact suffix sample by join boundaries",
        d.app().transport.as_ref().is_some_and(|run| run.sample == expected && run.window().start() == window.start() && run.window().end() == AudioSample(boundary(60).unwrap().0+d.app().audition_context.follow.0)) && d.app().edit_range == selected,
        json!({"heard_sample":expected.0,"removed_end":60,"selection_unchanged":true}), d.snapshot())?;
    d.key(Key::Space)?;
    d.check(
        "Replacement pause retains the exact compared sample",
        d.app().transport.is_none() && draft(d)?.position == Some(expected),
        json!(expected.0),
        state(d),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Replacement resume uses the exact retained sample",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == expected),
        json!(expected.0),
        d.snapshot(),
    )?;
    d.key(Key::B)?;
    d.check(
        "Returning to Proposed maps the suffix sample back without rounding drift",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == heard && run.window() == &window),
        json!(heard.0),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.app_mut().feedback.simulate_playback = simulated;
    Ok(())
}

fn fast_paste(d: &mut Driver<'_>, before: bool) -> Result<(), String> {
    select(d, 30, 60)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    d.key_modified(
        Key::P,
        if before {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        },
    )?;
    d.changed(&revision)?;
    d.check(
        "Fast p/P replaces the selected range instead of inserting beside the selected beat",
        d.app().sequence_length() == 104
            && d.app().sequence_cursor == 30
            && d.app().selected_edit_range().is_none()
            && copied(d) == Some(10..24),
        json!({"before_key":before,"duration":104,"cursor":30}),
        d.snapshot(),
    )?;
    wait_picture(d, 10, "Showing sequence frame 31")?;
    motion(d, 14, true)?;
    wait_picture(d, 60, "Showing sequence frame 45")?;
    undo_restores(d, &saved)
}

fn command_capture(d: &mut Driver<'_>) -> Result<(), String> {
    for command in ["splice", "paste", "paste-before"] {
        d.key(Key::Escape)?;
        d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L, Key::S])?;
        d.wait_for(
            "Prepare a real preceding edit for stale command capture",
            |app| !app.service.is_busy(),
        )?;
        d.settled()?;
        select(d, 30, 60)?;
        d.key(Key::Colon)?;
        d.events(
            "Type the placement command against its captured selection",
            vec![egui::Event::Text(command.into())],
        )?;
        let revision = document(d)?.revision_id().clone();
        d.app().service.submit(ProjectRequest::Undo {
            expected_revision: revision,
        })?;
        d.wait_for(
            "Concurrent Undo changes revision while placement command text remains open",
            |app| !app.service.is_busy() && app.selected_edit_range().is_none(),
        )?;
        let saved = document(d)?.clone();
        d.key(Key::Enter)?;
        d.settled()?;
        d.check("A placement command cannot acquire a new destination after its captured revision changes",
            d.app().splice.is_none() && *document(d)? == saved && d.app().error.as_deref().is_some_and(|error| error.contains("captured Edit destination changed")),
            json!({"command":command,"stale_rejected":true,"revision":saved.revision_id()}), d.snapshot())?;
    }
    // A command opened in Original captures the absence of an Edit destination.
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::S)?;
    d.wait_for("Split finishes while its completion is withheld", |app| {
        !app.service.is_busy()
    })?;
    d.command("source")?;
    d.key(Key::Colon)?;
    d.events(
        "Type paste while command entry has no Edit target",
        vec![egui::Event::Text("paste".into())],
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.wait_for("Late split completion selects Your edit", |app| {
        app.view == View::Sequence && !app.service.is_busy()
    })?;
    let saved = document(d)?.clone();
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "A late edit completion cannot supply an absent command-entry destination",
        *document(d)? == saved
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Return to Your edit")),
        json!("captured absence retained"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    Ok(())
}

fn nested(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    let original = document(d)?.clone();
    let revision = d.revision();
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L, Key::S])?;
    d.changed(&revision)?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("No nested replacement workspace")?;
    let path = workspace.path.clone();
    let source = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No selected split suffix")?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Release writer before grouping the nested replacement fixture",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let inner = NodeId::new("replacement-inner").map_err(|error| error.to_string())?;
    let outer = NodeId::new("replacement-outer").map_err(|error| error.to_string())?;
    let mut selected = source;
    for id in [&inner, &outer] {
        let document = store.snapshot().map_err(|error| error.to_string())?;
        let children = document
            .children(document.root())
            .cloned()
            .collect::<Vec<_>>();
        let start = children
            .iter()
            .position(|child| child == &selected)
            .ok_or("Missing nested grouping target")?;
        store
            .commit(&CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("{id}-fixture"))
                    .map_err(|error| error.to_string())?,
                command: Command::Group {
                    parent: document.root().clone(),
                    start,
                    end: start + 1,
                    id: id.clone(),
                    label: format!("{id} group"),
                },
            })
            .map_err(|error| error.to_string())?;
        selected = id.clone();
    }
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Reopen nested replacement fixture", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for(
        "Original copy is durably saved before the next operation",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.command("sequence")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num3,
        Key::Num0,
        Key::L,
        Key::Enter,
        Key::Enter,
    ])?;
    d.check(
        "Keyboard group entry retains the nonzero absolute Edit origin",
        d.app().sequence_scope.groups() == [outer.clone(), inner.clone()]
            && d.app().scope_start == 20,
        json!({"scope":[outer,inner],"start":20}),
        d.snapshot(),
    )?;
    select(d, 30, 60)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::R)?;
    wait_ready(d)?;
    let proposed = prepared(d)?;
    d.check(
        "Nested replacement uses the explicitly entered Sequence and absolute Edit range",
        draft(d)?.proposal_for_check().parent == inner
            && proposed.range == range(30, 44)
            && proposed.removed == Some(range(30, 60)),
        json!({"parent":inner,"removed":[30,60],"inserted":[30,44]}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    undo_restores(d, &saved)?;
    select(d, 30, 60)?;
    d.key(Key::Backspace)?;
    d.check(
        "Leaving the owning Sequence discards its selection instead of retargeting a parent range",
        d.app().selected_edit_range().is_none()
            && !d.app().edit_range.active
            && d.app().sequence_scope.groups() == [outer],
        json!("range cleared in parent group"),
        d.snapshot(),
    )?;
    d.key(Key::Backspace)?;
    for _ in 0..3 {
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
    }
    restored(d, &original)
}

fn cancel(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.wait_for("Cancel replacement without saving", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()
}

fn undo_restores(d: &mut Driver<'_>, saved: &deadpan_core::ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    restored(d, saved)
}

fn restored(d: &mut Driver<'_>, saved: &deadpan_core::ProjectDocument) -> Result<(), String> {
    let expected = serde_json::to_value(saved).map_err(|error| error.to_string())?;
    let mut actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    actual["revision_id"] = expected["revision_id"].clone();
    d.check(
        "One Undo restores every authored field with a fresh durable revision",
        actual == expected && document(d)?.revision_id() != saved.revision_id(),
        json!({"exact_authored_document":true,"old_revision":saved.revision_id()}),
        json!({"new_revision":document(d)?.revision_id(),"equal":actual == expected}),
    )
}
