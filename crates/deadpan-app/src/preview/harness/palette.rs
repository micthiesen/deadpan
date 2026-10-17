//! The Keys sheet as an action palette, through the production router, the
//! project service and the real Help window: Enter and clicks run the
//! highlighted result as a typed command or key press would.

use super::*;
use crate::navigation::registry;
use egui::{Key, Modifiers};

fn text(value: &str) -> egui::Event {
    egui::Event::Text(value.into())
}

fn open_help(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Questionmark)?;
    d.wait_for("Help opens", |app| app.help_open)?;
    for frame in 0..4 {
        // The first Window pass measures its content before painting it.
        d.step(&format!("Help opening transition {frame}"), false)?;
    }
    Ok(())
}

/// Open Help, then `/` and the query in one native batch.
fn search(d: &mut Driver<'_>, query: &str) -> Result<(), String> {
    open_help(d)?;
    d.events(
        &format!("Search the palette for {query}"),
        vec![
            key_event(Key::Slash, Modifiers::NONE, true),
            text("/"),
            text(query),
        ],
    )?;
    d.step("Search results paint", true)
}

fn closed_without_edit(d: &Driver<'_>, revision: &str) -> bool {
    !d.app().help_open && d.revision() == revision
}

fn palette_row_text_is_fully_visible(
    d: &Driver<'_>,
    found: &crate::navigation::palette::Match,
) -> bool {
    let body = super::scenarios::text_paint_visibility(d, found.spec.name)
        .into_iter()
        .filter(|paint| {
            paint["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(found.spec.name) && text.contains(&found.help))
        })
        .collect::<Vec<_>>();
    let key = super::scenarios::text_paint_visibility(d, &found.keys)
        .into_iter()
        .filter(|paint| paint["text"].as_str() == Some(found.keys.as_str()))
        .collect::<Vec<_>>();
    body.len() == 1
        && body[0]["fully_visible"] == true
        && key.len() == 1
        && key[0]["fully_visible"] == true
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Palette keys and clicks are replayed as egui events through the production router; physical key delivery, VoiceOver speech of the highlighted row and pointer hit-testing on a native window are not exercised here.".into());
    super::transcript::focus_your_edit(d)?;
    d.settled()?;
    // A copied Original moment for Place slice.
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
    d.wait_for("Original slice copy is durable", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })?;
    d.command("sequence")?;
    d.settled()?;
    let revision = d.revision();

    // `/splice` and Enter in one batch: the field reads the query, Enter
    // runs its first result on the next frame, and Place slice opens.
    open_help(d)?;
    d.events(
        "Search splice and press Enter in one native batch",
        vec![
            key_event(Key::Slash, Modifiers::NONE, true),
            text("/"),
            text("splice"),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.wait_for("The highlighted :splice opens Place slice", |app| {
        app.splice.is_some()
    })?;
    d.check(
        "Enter on the highlighted :splice closes Help and opens Place slice without an edit",
        closed_without_edit(d, &revision) && d.app().splice.is_some() && !d.app().command_open,
        json!({"help_open":false,"place_slice":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.capture("Place slice opened from the palette")?;
    d.key(Key::Escape)?;
    d.wait_for("Place slice cancels", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()?;

    // Up/Down move the highlight; the hint teaches them while searching.
    search(d, "jobs")?;
    let count = crate::navigation::palette::matches(&d.app().bindings, "jobs").len();
    let hint = ["Enter", "Up / Down", "Ctrl N / Ctrl P"]
        .map(|key| (key, super::scenarios::keycap_painted(d, key)));
    d.check(
        "Searching teaches Up/Down, Ctrl N/P and Enter as keycaps and highlights the first result",
        hint.iter().all(|(_, painted)| *painted)
            && d.app().help_scroll.palette.highlighted() == 0
            && count >= 2,
        json!({"keycaps":["Enter","Up / Down","Ctrl N / Ctrl P"],"highlighted":0,"results_at_least":2}),
        json!({"keycaps":hint,"highlighted":d.app().help_scroll.palette.highlighted(),"results":count}),
    )?;
    d.capture("Palette highlights the first jobs result")?;
    d.key(Key::ArrowDown)?;
    let down = d.app().help_scroll.palette.highlighted();
    d.key_modified(Key::P, Modifiers::CTRL)?;
    let back = d.app().help_scroll.palette.highlighted();
    d.check(
        "Down and Ctrl P move the highlight in the open search",
        down == 1 && back == 0 && d.app().help_open,
        json!({"after_down":1,"after_ctrl_p":0}),
        json!({"after_down":down,"after_ctrl_p":back}),
    )?;
    d.key(Key::Enter)?;
    d.wait_for("The highlighted :jobs opens its panel", |app| app.jobs.open)?;
    d.check(
        "A command without arguments runs as if typed",
        closed_without_edit(d, &revision) && d.app().jobs.open,
        json!({"help_open":false,"jobs_open":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.wait_for("The Jobs panel closes", |app| !app.jobs.open)?;
    d.settled()?;

    // A key-only action is performed through the editor's own dispatch.
    search(d, "v")?;
    let first = crate::navigation::palette::matches(&d.app().bindings, "v")
        .first()
        .map(|found| found.spec.id);
    d.key(Key::Enter)?;
    d.wait_for("The key-only Visual action runs", |app| !app.help_open)?;
    d.step("Visual selection paints", true)?;
    d.check(
        "Enter on a key-only action starts Visual as its key would",
        first == Some("visual")
            && closed_without_edit(d, &revision)
            && d.app().edit_selection() != crate::navigation::EditSelection::None,
        json!({"first":"visual","edit_selection":"active","revision":revision}),
        json!({"first":first,"snapshot":d.snapshot()}),
    )?;
    d.capture("Visual started from the palette")?;
    d.key(Key::Escape)?;
    d.wait_for("Escape clears the Visual selection", |app| {
        app.edit_selection() == crate::navigation::EditSelection::None
    })?;

    // An operator enters its pending prefix and waits for its motion.
    search(d, "cut to a motion")?;
    d.key(Key::Enter)?;
    d.wait_for("The cut operator waits for its motion", |app| {
        !app.help_open
    })?;
    d.step("Pending operator paints", true)?;
    let pending = d.app().bindings.pending();
    let footer = super::scenarios::text_paint_visibility(d, "PENDING");
    d.check(
        "An operator chosen in the palette shows its pending prefix and changes nothing",
        pending
            == d.app()
                .bindings
                .key_label(crate::navigation::BindingId::CutOperator)
            && d.app().bindings.operator_pending()
            && d.app().operator_target.is_some()
            && closed_without_edit(d, &revision),
        json!({"pending":"d","operator_captured":true,"revision":revision}),
        json!({"pending":pending,"footer":footer,"captured":d.app().operator_target.is_some()}),
    )?;
    d.capture("Cut operator pending from the palette")?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels the pending operator",
        d.app().bindings.pending().is_empty() && d.revision() == revision,
        json!(""),
        json!(d.app().bindings.pending()),
    )?;

    // A command with arguments opens the command line with its usage.
    search(d, ":hold")?;
    d.key(Key::Enter)?;
    d.wait_for("The command line opens for :hold", |app| app.command_open)?;
    d.step("Completion paints", true)?;
    let usage = super::scenarios::text_paint_visibility(d, ":hold 0.5s");
    d.check(
        "A command with arguments opens the command line with its name and usage",
        !d.app().help_open && d.app().command == "hold " && !usage.is_empty(),
        json!({"command":"hold ","usage_painted":true}),
        json!({"command":d.app().command,"usage":usage}),
    )?;
    d.capture("Palette prefilled :hold")?;
    d.key(Key::Escape)?;
    d.wait_for("Command entry closes", |app| !app.command_open)?;

    // A mode-only key keeps Help open and says where and how it works.
    search(d, "apply trim")?;
    d.key(Key::Enter)?;
    d.step("Explanation paints", true)?;
    let explained = super::scenarios::text_paint_visibility(d, "works in Trim");
    d.check(
        "An action that does not work here keeps Help open and explains where and how",
        d.app().help_open
            && d.revision() == revision
            && d.app().trim.is_none()
            && explained.iter().any(|part| part["fully_visible"] == true),
        json!({"help_open":true,"explained":"works in Trim"}),
        json!({"help_open":d.app().help_open,"explained":explained}),
    )?;
    d.capture("Palette explains a Trim key")?;
    // Escape leaves the field, then closes Help, as before.
    d.key(Key::Escape)?;
    d.check(
        "The first Escape still leaves search, keeping Help",
        d.app().help_open,
        json!(true),
        json!(d.app().help_open),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "The second Escape closes Help",
        !d.app().help_open,
        json!(false),
        json!(d.app().help_open),
    )?;

    // A click runs its row.
    search(d, "jobs")?;
    let spec = registry::by_id("jobs").ok_or("jobs is registered")?;
    let label = format!(
        "{}. {}",
        spec.name,
        registry::render(spec.help, &d.app().bindings)
    );
    d.click(&label)?;
    d.wait_for("A clicked row runs", |app| app.jobs.open)?;
    d.check(
        "Clicking a result runs it",
        closed_without_edit(d, &revision) && d.app().jobs.open,
        json!({"help_open":false,"jobs_open":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.wait_for("The Jobs panel closes", |app| !app.jobs.open)?;

    // The two forms of one registry action remain separately searchable and
    // executable. This starts after the checks that depend on the untouched
    // baseline revision.
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let original_structure = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project for palette history check")?
        .document
        .nodes()
        .clone();
    let before_split = d.revision();
    d.command("split")?;
    d.changed(&before_split)?;
    d.settled()?;
    let split_revision = d.revision();
    let split_structure = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project after palette split")?
        .document
        .nodes()
        .clone();
    d.check(
        "A split creates a durable edit for the palette's Undo / Redo choices",
        split_structure != original_structure
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_undo),
        json!({"structure_changed":true,"can_undo":true}),
        json!({"structure_changed":split_structure != original_structure,"nodes":split_structure.len(),"can_undo":d.app().workspace.as_ref().map(|workspace| workspace.can_undo)}),
    )?;

    search(d, "undo")?;
    let history_rows = crate::navigation::palette::matches(&d.app().bindings, "undo");
    let undo_row = history_rows.iter().find(|found| {
        found.choice.spec_id == "undo"
            && found.choice.form == crate::navigation::palette::Form::Command(0)
    });
    let redo_row = history_rows.iter().find(|found| {
        found.choice.spec_id == "undo"
            && found.choice.form == crate::navigation::palette::Form::Command(1)
    });
    let painted_history = [undo_row, redo_row]
        .map(|row| row.is_some_and(|found| palette_row_text_is_fully_visible(d, found)));
    d.check(
        "One broad search paints distinct Undo and Redo names and key paths",
        undo_row.is_some_and(|found| found.spec.name == "Undo")
            && redo_row.is_some_and(|found| found.spec.name == "Redo")
            && undo_row.unwrap().keys != redo_row.unwrap().keys
            && painted_history == [true, true],
        json!({"rows":[{"name":"Undo","key":"u"},{"name":"Redo","key":"Ctrl R / ⌘⇧Z"}]}),
        json!({"rows":history_rows.iter().map(|found| json!({"name":found.spec.name,"keys":found.keys,"form":format!("{:?}",found.choice.form)})).collect::<Vec<_>>(),"painted":painted_history}),
    )?;
    d.capture("Palette shows separate Undo and Redo actions")?;
    // `search` opens a fresh Help sheet. Leave the current field and sheet
    // before starting the command-specific searches below.
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.wait_for("Help closes after the broad Undo / Redo search", |app| {
        !app.help_open
    })?;

    // The :undo query must pick Command(0), then the exact :redo query must
    // pick Command(1) and restore the same durable split.
    search(d, ":undo")?;
    let undo_matches = crate::navigation::palette::matches(&d.app().bindings, ":undo");
    d.check(
        "Searching :undo highlights the Undo form",
        undo_matches.first().is_some_and(|found| {
            found.choice.spec_id == "undo"
                && found.choice.form == crate::navigation::palette::Form::Command(0)
                && found.spec.name == "Undo"
        }),
        json!({"form":"Command(0)","name":"Undo"}),
        json!(
            undo_matches.first().map(
                |found| json!({"form":format!("{:?}",found.choice.form),"name":found.spec.name})
            )
        ),
    )?;
    d.key(Key::Enter)?;
    d.changed(&split_revision)?;
    let undone_revision = d.revision();
    let undone_structure = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project after palette Undo")?
        .document
        .nodes()
        .clone();
    d.check(
        "Palette Undo reverses the split and makes Redo available",
        undone_structure == original_structure
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_redo),
        json!({"structure_restored":true,"can_redo":true}),
        json!({"structure_restored":undone_structure == original_structure,"nodes":undone_structure.len(),"can_redo":d.app().workspace.as_ref().map(|workspace| workspace.can_redo)}),
    )?;

    search(d, ":redo")?;
    let redo_matches = crate::navigation::palette::matches(&d.app().bindings, ":redo");
    d.check(
        "Searching :redo highlights the Redo form rather than Undo",
        redo_matches.first().is_some_and(|found| {
            found.choice.spec_id == "undo"
                && found.choice.form == crate::navigation::palette::Form::Command(1)
                && found.spec.name == "Redo"
        }),
        json!({"form":"Command(1)","name":"Redo"}),
        json!(
            redo_matches.first().map(
                |found| json!({"form":format!("{:?}",found.choice.form),"name":found.spec.name})
            )
        ),
    )?;
    d.key(Key::Enter)?;
    d.changed(&undone_revision)?;
    let redone_structure = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project after palette Redo")?
        .document
        .nodes()
        .clone();
    d.check(
        "Palette Redo restores the same durable split",
        redone_structure == split_structure
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_undo && !workspace.can_redo),
        json!({"structure_restored":true,"can_undo":true,"can_redo":false}),
        json!({"structure_restored":redone_structure == split_structure,"nodes":redone_structure.len(),"can_undo":d.app().workspace.as_ref().map(|workspace| workspace.can_undo),"can_redo":d.app().workspace.as_ref().map(|workspace| workspace.can_redo)}),
    )?;

    // Search both directional key-only forms together, then choose the
    // previous row by click and the next row by its exact configured key.
    let previous_key = d
        .app()
        .bindings
        .key_label(crate::navigation::BindingId::FramePrevious);
    let next_key = d
        .app()
        .bindings
        .key_label(crate::navigation::BindingId::FrameNext);
    search(d, "frame")?;
    let frame_rows = crate::navigation::palette::matches(&d.app().bindings, "frame");
    let previous = frame_rows.iter().find(|found| {
        found.choice.spec_id == "frame"
            && found.choice.form == crate::navigation::palette::Form::Binding(0)
    });
    let next = frame_rows.iter().find(|found| {
        found.choice.spec_id == "frame"
            && found.choice.form == crate::navigation::palette::Form::Binding(1)
    });
    let contains_primary_key = |found: &&crate::navigation::palette::Match, key: &str| {
        found
            .keys
            .split(" · ")
            .flat_map(|part| part.split(" / "))
            .any(|shown| shown == key)
    };
    let painted_directions = [previous, next]
        .map(|row| row.is_some_and(|found| palette_row_text_is_fully_visible(d, found)));
    d.check(
        "A broad frame search paints separate Previous frame and Next frame rows and keys",
        previous.is_some_and(|found| {
            found.spec.name == "Previous frame" && contains_primary_key(&found, &previous_key)
        }) && next.is_some_and(|found| {
            found.spec.name == "Next frame" && contains_primary_key(&found, &next_key)
        })
            && previous_key != next_key
            && painted_directions == [true, true],
        json!({"previous":"Previous frame","previous_key":previous_key.clone(),"next":"Next frame","next_key":next_key.clone()}),
        json!({"rows":frame_rows.iter().filter(|found| found.choice.spec_id == "frame").map(|found| json!({"name":found.spec.name,"keys":found.keys,"form":format!("{:?}",found.choice.form)})).collect::<Vec<_>>(),"painted":painted_directions}),
    )?;
    d.capture("Palette distinguishes Previous frame and Next frame")?;
    let cursor_before = d.app().sequence_cursor;
    let previous_label = format!(
        "{}. {}",
        previous.unwrap().spec.name,
        previous.unwrap().help
    );
    d.click(&previous_label)?;
    d.wait_for("The clicked Previous frame moves left", |app| {
        app.sequence_cursor + 1 == cursor_before && !app.help_open
    })?;

    search(d, &next_key)?;
    let next_matches = crate::navigation::palette::matches(&d.app().bindings, &next_key);
    let next_index = next_matches
        .iter()
        .position(|found| {
            found.choice.spec_id == "frame"
                && found.choice.form == crate::navigation::palette::Form::Binding(1)
                && found
                    .keys
                    .split(" · ")
                    .flat_map(|part| part.split(" / "))
                    .any(|key| key == next_key)
        })
        .ok_or("exact next-frame key did not find its Next frame choice")?;
    for _ in 0..next_index {
        d.key(Key::ArrowDown)?;
    }
    d.check(
        "The exact next-frame key query highlights Next frame",
        d.app().help_scroll.palette.highlighted() == next_index,
        json!({"highlighted":next_index,"name":"Next frame","key":next_key.clone()}),
        json!({"highlighted":d.app().help_scroll.palette.highlighted()}),
    )?;
    let cursor_before = d.app().sequence_cursor;
    d.key(Key::Enter)?;
    d.wait_for("The exact-key Next frame choice moves right", |app| {
        app.sequence_cursor == cursor_before + 1 && !app.help_open
    })?;
    d.check(
        "Palette key-only Previous frame and Next frame move in their named directions",
        d.app().sequence_cursor == cursor_before + 1,
        json!({"next_cursor":cursor_before + 1}),
        json!({"cursor":d.app().sequence_cursor}),
    )?;
    Ok(())
}
