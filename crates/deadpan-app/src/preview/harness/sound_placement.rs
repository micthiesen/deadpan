//! Authored sound placement through production import, input and project services.

use super::*;
use deadpan_core::{AudioEdgePolicy, AudioSample, ExactRatio, FrameDuration, SoundEvent, SoundId};
use egui::{Key, Modifiers};

mod allowances;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Sound placement uses real source qualification and durable project commands. This scenario does not start audio output or establish PCM, device timing, or listening evidence.".into(),
    );
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    d.click("Add sound…  ⌘I")?;
    d.wait_for("Sound is qualified in the real catalog", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::L])?;
    d.settled()?;
    let catalog = d.app().sound_rows[0].clone();
    let baseline = targets(d);
    let nodes = document(d)?.nodes().clone();
    let initial_sounds = document(d)?.sounds().clone();
    let rate = document(d)?.presentation_basis().frame_rate;
    let onset = rate
        .audio_boundary(ProjectFrame(2))
        .map_err(|error| error.to_string())?;
    d.check(
        "Placement starts from an explicit edit cursor and retained beat",
        d.app().sequence_cursor == 2 && d.app().selected_beat.is_some(),
        json!({"edit_frame":2,"selected_beat":true}),
        d.snapshot(),
    )?;
    d.click(&catalog.1)?;
    // The catalog click changes selection on release; its dependent controls
    // enter the UI tree on the next paint, as in the native event loop.
    d.step("Paint controls for the selected catalog sound", true)?;
    let before = d.revision();
    d.click("Place at edit cursor  ·  ,s")?;
    d.changed(&before)?;
    let first = selected(d)?;
    let first_recipe = event(d, &first)?;
    d.check(
        "Placement authors one root sound without changing picture duration or editor targets",
        document(d)?.sounds().len() == initial_sounds.len() + 1
            && first_recipe.owner == *document(d)?.root()
            && first_recipe.source.asset == catalog.0
            && first_recipe.offset == onset
            && document(d)?.nodes() == &nodes
            && targets(d) == baseline,
        json!({"offset":onset,"targets":baseline,"unchanged_picture_nodes":true}),
        d.snapshot(),
    )?;

    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "One undo removes only the placed sound",
        document(d)?.sounds() == &initial_sounds
            && document(d)?.nodes() == &nodes
            && targets(d) == baseline,
        json!({"sounds":initial_sounds,"targets":baseline}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    d.command("sounds")?;
    d.check(
        "Redo restores the same sound identity and recipe",
        event(d, &first)? == first_recipe && targets(d) == baseline,
        json!({"sound":first,"recipe":first_recipe}),
        d.snapshot(),
    )?;
    focus_entry(d, &first, &baseline)?;

    // Exercise the actual text widget, including final native text and closing
    // keys in one frame. Neither cancellation nor text letters may edit a beat.
    d.key(Key::Enter)?;
    d.check(
        "Enter offers an exact sample position in the native command footer",
        d.app().command_open && d.app().command == format!("sound-at {}", onset.0),
        json!(format!("sound-at {}", onset.0)),
        d.snapshot(),
    )?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    let before = d.revision();
    d.events(
        "Native command text and Escape cancel sound editing in one frame",
        vec![
            key_event(Key::D, Modifiers::NONE, true),
            key_event(Key::D, Modifiers::NONE, false),
            key_event(Key::D, Modifiers::NONE, true),
            key_event(Key::D, Modifiers::NONE, false),
            egui::Event::Text("sound-at 999 dd ".into()),
            key_event(Key::Escape, Modifiers::NONE, true),
            key_event(Key::Escape, Modifiers::NONE, false),
        ],
    )?;
    scenarios::footer_anchored(d, "Cancelled sound position footer remains anchored")?;
    d.check(
        "Cancellation retains native text and performs no sound or beat command",
        !d.app().command_open
            && d.app().command == "sound-at 999 dd "
            && d.revision() == before
            && event(d, &first)? == first_recipe
            && document(d)?.nodes() == &nodes,
        json!("Closed command with final text; unchanged revision and recipe"),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Commit exact sample 137 from final native text and Enter",
        vec![
            egui::Event::Text("sound-at 137".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before)?;
    check_offset(
        d,
        &first,
        137,
        "Fine positioning retains sample 137 without frame rounding",
    )?;
    d.key(Key::Enter)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Native sound command hints accept a leading colon and uppercase verb",
        vec![egui::Event::Text(":SOUND-AT 137".into())],
    )?;
    visible(d, "Position: whole 48 kHz samples · 48,000 = 1 second")?;
    d.key(Key::Escape)?;

    d.command("sounds")?;
    let before = d.revision();
    d.key(Key::L)?;
    d.changed(&before)?;
    let frame_samples = rate
        .audio_boundary(ProjectFrame(1))
        .map_err(|error| error.to_string())?
        .0;
    check_onset(
        d,
        &first,
        137 + frame_samples,
        "Sound l moves one project frame on the sample clock",
    )?;
    d.check(
        "Frame nudges translate the exact recipe without changing its sample offset",
        event(d, &first)?.offset == AudioSample(137)
            && event(d, &first)?.mapping.start_frames() == ExactRatio::integer(1),
        json!({"offset":137,"mapping_start_frames":1}),
        json!(event(d, &first)?),
    )?;
    let exact_revision = d.revision();
    let exact_recipe = event(d, &first)?;
    let rounded_onset = audible(d, &first)?.start.0;
    d.key(Key::Enter)?;
    d.check(
        "The exact-phase sound opens a rounded sample label without editing its recipe",
        d.app().command_open && d.app().command == format!("sound-at {rounded_onset}"),
        json!(format!("sound-at {rounded_onset}")),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.wait_for(
        "Unchanged sound position entry finishes without work",
        |app| !app.service.is_busy(),
    )?;
    d.step(
        "Observe any completion after accepting an unchanged sound position",
        false,
    )?;
    d.check(
        "Accepting the unchanged rounded position preserves exact phase and creates no history",
        !d.app().command_open
            && d.revision() == exact_revision
            && event(d, &first)? == exact_recipe,
        json!({"revision":exact_revision,"recipe":exact_recipe}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key(Key::L)?;
    d.changed(&before)?;
    let twice = event(d, &first)?;
    let twice_audible = audible(d, &first)?;
    let before = d.revision();
    d.chord(&[Key::Num2, Key::H])?;
    d.changed(&before)?;
    check_onset(
        d,
        &first,
        137,
        "A two-frame reverse nudge restores the exact onset",
    )?;
    let before = d.revision();
    d.chord(&[Key::Num2, Key::L])?;
    d.changed(&before)?;
    d.check(
        "Two single-frame sound nudges equal one counted two-frame nudge",
        event(d, &first)? == twice && audible(d, &first)? == twice_audible,
        json!({"recipe":twice,"audible":twice_audible}),
        json!({"recipe":event(d, &first)?,"audible":audible(d, &first)?}),
    )?;
    for _ in 0..2 {
        let before = d.revision();
        d.key(Key::H)?;
        d.changed(&before)?;
    }
    check_onset(
        d,
        &first,
        137,
        "Sound h reverses the frame step without moving the edit cursor",
    )?;
    commit(d, "sound-gain -3")?;
    check_gain(d, &first, -3_000)?;
    d.command("sounds")?;
    let before = d.revision();
    d.key(Key::Plus)?;
    d.changed(&before)?;
    check_gain(d, &first, 0)?;
    let before = d.revision();
    d.key(Key::Minus)?;
    d.changed(&before)?;
    check_gain(d, &first, -3_000)?;
    for (command, expected) in [
        ("sound-edges hard", AudioEdgePolicy::Hard),
        ("sound-edges soft", AudioEdgePolicy::Automatic),
    ] {
        commit(d, command)?;
        let recipe = event(d, &first)?;
        d.check(
            "Edge commands set both sound endpoints without replacing the recipe",
            recipe.start_edge == expected
                && recipe.end_edge == expected
                && recipe.source == first_recipe.source
                && same_mapping(&recipe, &first_recipe)?
                && recipe.offset == AudioSample(137),
            json!({"edge":expected,"offset":137}),
            json!(recipe),
        )?;
    }
    d.check(
        "Sound parameters retain the Original, picture nodes, cursors and selected beat",
        targets(d) == baseline && document(d)?.nodes() == &nodes,
        baseline.clone(),
        d.snapshot(),
    )?;
    command_targets(d, &first, &baseline)?;

    rejected(
        d,
        "sound-at 9223372036854775807",
        "Overflowing placement reports an error without an edit",
    )?;
    // A valid command also proves recovery from the rejected edit.
    commit(d, "sound-gain -6")?;
    d.click(&catalog.1)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::S])?;
    d.changed(&before)?;
    let second = selected(d)?;
    d.check(
        "Comma s places a separate event from the same catalog asset",
        second != first && document(d)?.sounds().len() == 2,
        json!("Two independently authored sound events"),
        d.snapshot(),
    )?;
    d.command("sounds")?;
    let ordered = document(d)?.sounds().keys().cloned().collect::<Vec<_>>();
    d.chord(&[Key::Num6, Key::Num4, Key::K])?;
    d.check(
        "Placed-sound k selects the first displayed event without selecting a beat",
        d.app().selected_event.as_ref() == Some(&ordered[0]) && targets(d) == baseline,
        json!({"selected_event":ordered[0],"targets":baseline}),
        d.snapshot(),
    )?;
    d.key(Key::J)?;
    d.check(
        "Placed-sound j selects the next event and retains beat selection",
        d.app().selected_event.as_ref() == Some(&ordered[1]) && targets(d) == baseline,
        json!({"selected_event":ordered[1],"targets":baseline}),
        d.snapshot(),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.capture(&format!(
            "Placed sounds and exact inspector at {width}x{height}"
        ))?;
        visible(d, "PLACED SOUNDS")?;
        reveal(d, "Start", 240.0)?;
        d.settled()?;
        d.capture(&format!("Settled sound workspace at {width}x{height}"))?;
        scenarios::viewer_visible(d)?;
        let label = d
            .app()
            .presentation
            .displayed_label()
            .ok_or("Missing picture label")?;
        let viewer = d.rect(&label)?;
        d.check(
            "Sound editing retains useful picture height at the minimum viewport",
            viewer.height() >= 120.0,
            json!({"minimum_picture_height":120.0}),
            json!({"viewer_height":viewer.height(),"viewport":[width,height]}),
        )?;
        if width == 960.0 {
            compact_playback(d)?;
        }
        reveal(d, "Fine position · 48 kHz samples", 240.0)?;
        visible(d, "Fine position · 48 kHz samples")?;
        reveal(d, "Change gain  ·  + / −", -240.0)?;
        visible(d, "Change gain  ·  + / −")?;
        reveal(d, "Remove sound  ·  dd", -240.0)?;
        visible(d, "Remove sound  ·  dd")?;
        d.capture(&format!("Sound removal is reachable at {width}x{height}"))?;
        d.check(
            "Placed sounds keep a distinct pane focus at both window sizes",
            d.app().pane == Pane::Sounds,
            json!("Sounds"),
            d.snapshot(),
        )?;
        d.key(Key::Enter)?;
        scenarios::footer_anchored(
            d,
            "Exact sound position footer remains visible after resize",
        )?;
        d.capture(&format!("Exact sound command footer at {width}x{height}"))?;
        visible(d, "Position: whole 48 kHz samples · 48,000 = 1 second")?;
        d.key(Key::Escape)?;
        d.command("sounds")?;
    }
    choose(d, &second)?;
    reveal(d, "Change gain  ·  + / −", 240.0)?;
    d.click("Change gain  ·  + / −")?;
    d.key(Key::Escape)?;
    d.check(
        "The sound inspector owns destructive shortcuts while the picture beat stays selected",
        d.app().pane == Pane::Inspector
            && d.app().selected_event.as_ref() == Some(&second)
            && targets(d) == baseline,
        json!({"pane":"Inspector","selected_event":second,"targets":baseline}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    d.check(
        "Sound dd removes its event while preserving the selected picture beat",
        !document(d)?.sounds().contains_key(&second)
            && document(d)?.sounds().contains_key(&first)
            && document(d)?.nodes() == &nodes
            && targets(d) == baseline,
        json!({"removed_sound":second,"retained_sound":first,"targets":baseline}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    // Undo need not resurrect ephemeral selection. Select the second event using
    // the same bounded row navigation as a user before its pointer removal.
    choose(d, &second)?;
    d.check(
        "Undo restores the removable second event",
        document(d)?.sounds().contains_key(&second)
            && d.app().selected_event.as_ref() == Some(&second),
        json!(second),
        d.snapshot(),
    )?;
    let before = d.revision();
    reveal(d, "Remove sound  ·  dd", -240.0)?;
    d.click("Remove sound  ·  dd")?;
    d.changed(&before)?;
    d.check(
        "Pointer removal shares sound deletion and never deletes the retained beat",
        document(d)?.sounds().len() == 1
            && document(d)?.sounds().contains_key(&first)
            && document(d)?.nodes() == &nodes
            && targets(d) == baseline,
        json!({"remaining_sound":first,"targets":baseline}),
        d.snapshot(),
    )?;

    d.command("sequence")?;
    d.check(
        "The sequence command leaves sound focus before an immediate structural edit",
        d.app().pane == Pane::Sequence
            && !d.app().event_focused()
            && d.app().selected_event.is_none()
            && d.app().sound_inspection.is_none()
            && targets(d) == baseline,
        json!({"pane":"Sequence","selected_event":null,"targets":baseline}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("hold 2f")?;
    d.changed(&before)?;
    let routes = document(d)?.sound_routes().clone();
    let routed = event(d, &first)?;
    d.check(
        "Inserting a pause through the sound creates retained root routing",
        routes
            .get(&first)
            .is_some_and(|route| !route.edits.is_empty())
            && routed.offset == AudioSample(137)
            && same_mapping(&routed, &first_recipe)?,
        json!("A nonempty sound route with the original recipe phase and offset"),
        d.snapshot(),
    )?;
    d.command("sounds")?;
    commit(d, "sound-gain -9")?;
    check_gain(d, &first, -9_000)?;
    d.check(
        "Changing a routed sound's gain preserves the exact edit journal",
        document(d)?.sound_routes() == &routes,
        json!(routes),
        d.snapshot(),
    )?;
    rejected(
        d,
        "sound-at 200",
        "Moving a routed sound rejects without losing its journal",
    )?;
    d.capture("Routed sound retains its journal after rejected repositioning")?;
    allowances::run(d, &first, &catalog.1)?;
    Ok(())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Sound-placement replay lost its workspace".into())
}

fn compact_playback(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Preparing/Playing at 960 pixels uses the real transport controls with simulated delivery reports only. It does not start a device or prepare PCM.".into(),
    );
    let before = targets(d);
    let revision = d.revision();
    let selected = selected(d)?;
    let sounds = document(d)?.sounds().clone();
    d.click("Play edit  ·  Space")?;
    d.check(
        "Pointer Play from Sounds starts the edit transport in Preparing",
        d.app().transport.as_ref().is_some_and(|run| {
            run.phase == deadpan_playback::Phase::Preparing && !run.domain().is_sound()
        }) && d.app().pane == Pane::Sounds,
        json!("Preparing edit playback with placed sound focus"),
        d.snapshot(),
    )?;
    // Inspect the click-release frame itself. An extra frame could conceal a
    // control painted after the old stopped-state reservation was allocated.
    playback_layout(d, "Cancel preparation  ·  Space", "Preparing ·")?;
    d.capture("Preparing edit playback with sound inspector at 960x640")?;

    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Missing preparing transport")?;
    let generation = feed
        .restart(run.sample.0)
        .map_err(|error| error.to_string())?;
    let playing = deadpan_playback::Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        phase: deadpan_playback::Phase::Playing,
        sample: Some(run.sample),
        generation: Some(generation),
        error: None,
    };
    d.app_mut().feedback.playback_updates.push_back(playing);
    d.capture("Simulated Playing feedback with sound inspector at 960x640")?;
    d.check(
        "Simulated delivery starts Playing without advancing either editor cursor",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == deadpan_playback::Phase::Playing)
            && targets(d) == before,
        before.clone(),
        d.snapshot(),
    )?;
    playback_layout(d, "Pause  ·  Space", "Playing ·")?;
    d.click("Pause  ·  Space")?;
    visible(d, "Play edit  ·  Space")?;
    visible(d, "Monitor · :monitor")?;
    scenarios::footer_anchored(
        d,
        "Pointer Pause paints the stopped controls on its release frame",
    )?;
    d.settled()?;
    d.check(
        "Pausing the compact playback check retains authored state and sound selection",
        d.app().transport.is_none()
            && d.app().selected_event.as_ref() == Some(&selected)
            && d.app().pane == Pane::Sounds
            && targets(d) == before
            && d.revision() == revision
            && document(d)?.sounds() == &sounds,
        json!({"revision":revision,"selected_event":selected,"targets":before}),
        d.snapshot(),
    )?;
    Ok(())
}

fn playback_layout(d: &mut Driver<'_>, button: &str, status: &str) -> Result<(), String> {
    visible(d, button)?;
    visible(d, status)?;
    visible(d, "Monitor · :monitor")?;
    visible(d, "SOUND EVENT")?;
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Playback lost its picture label")?;
    let viewer = d.rect(&label)?;
    let sounds = d.rect("Placed sounds pane")?;
    for label in [button, status, "Monitor · :monitor"] {
        let paint = scenarios::text_paint_visibility(d, label);
        let positioned = paint.iter().all(|item| {
            let Some(bounds) = item["bounds"].as_array() else {
                return false;
            };
            let coordinates = bounds.iter().filter_map(Value::as_f64).collect::<Vec<_>>();
            coordinates.len() == 4
                && coordinates[0] >= f64::from(viewer.min.x)
                && coordinates[2] <= f64::from(viewer.max.x)
                && coordinates[1] >= f64::from(viewer.max.y)
                && coordinates[3] <= f64::from(sounds.min.y)
        });
        d.check(
            "Live playback controls fit below the picture and above placed sounds",
            positioned,
            json!({"label":label,"viewer":format!("{viewer:?}"),"sounds_top":sounds.min.y}),
            json!(paint),
        )?;
    }
    scenarios::footer_anchored(
        d,
        "Live playback keeps the compact workspace footer anchored",
    )
}

fn selected(d: &Driver<'_>) -> Result<SoundId, String> {
    d.app()
        .selected_event
        .clone()
        .ok_or_else(|| "No placed sound selected".into())
}

fn focus_entry(d: &mut Driver<'_>, id: &SoundId, baseline: &Value) -> Result<(), String> {
    for pointer in [false, true] {
        d.click("Browse  :source")?;
        d.settled()?;
        d.check(
            "Sound-pane entry starts with Original and no selected event",
            d.app().view == View::Source
                && d.app().pane == Pane::Sources
                && d.app().selected_event.is_none(),
            json!({"context":"Source","pane":"Sources","selected_event":null}),
            d.snapshot(),
        )?;
        if pointer {
            d.click("Placed sounds pane")?;
        } else {
            d.key_modified(Key::Tab, Modifiers::SHIFT)?;
        }
        d.check(
            "Tab and heading entry activate placed sounds without changing either cursor or beat",
            d.app().pane == Pane::Sounds
                && d.app().view == View::Sequence
                && d.app().event_focused()
                && d.app().selected_event.as_ref() == Some(id)
                && targets(d) == *baseline,
            json!({"entry":if pointer {"heading"} else {"Shift Tab"},"targets":baseline}),
            d.snapshot(),
        )?;
        d.key(Key::Enter)?;
        d.check(
            "The first Enter after entering Sounds edits the selected sound",
            d.app().command_open && d.app().command.starts_with("sound-at "),
            json!("Open exact sound position command"),
            d.snapshot(),
        )?;
        d.key(Key::Escape)?;
        d.settled()?;
    }
    let revision = d.revision();
    let sounds = document(d)?.sounds().clone();
    let source_cursor = d.app().source_cursor;
    d.command("source")?;
    d.check(
        "The source command clears sound selection and moves focus to Original",
        d.app().view == View::Source
            && d.app().pane == Pane::Sources
            && !d.app().event_focused()
            && d.app().selected_event.is_none()
            && d.app().sound_inspection.is_none(),
        json!({"context":"Source","pane":"Sources","selected_event":null}),
        d.snapshot(),
    )?;
    d.key(Key::L)?;
    let mut advanced = baseline.clone();
    advanced["context"] = json!("Source");
    advanced["source_cursor"] = json!(source_cursor + 1);
    d.check(
        "The first l after source advances Original instead of nudging a retained sound",
        targets(d) == advanced
            && d.revision() == revision
            && document(d)?.sounds() == &sounds
            && !d.app().service.is_busy(),
        advanced,
        d.snapshot(),
    )?;
    d.key(Key::H)?;
    d.command("sounds")?;
    d.settled()?;
    d.check(
        "Original h reverses navigation without history or sound changes",
        targets(d) == *baseline && d.revision() == revision && document(d)?.sounds() == &sounds,
        json!({"revision":revision,"sounds":sounds,"targets":baseline}),
        d.snapshot(),
    )?;
    let edit_cursor = d.app().sequence_cursor;
    d.click("Next  l")?;
    let mut next_picture = baseline.clone();
    next_picture["sequence_cursor"] = json!(edit_cursor + 1);
    d.check(
        "Picture Next leaves sound focus and moves the edit cursor without nudging a sound",
        d.app().pane == Pane::Viewer
            && d.app().selected_event.is_none()
            && d.app().sound_inspection.is_none()
            && targets(d) == next_picture
            && d.revision() == revision
            && document(d)?.sounds() == &sounds
            && !d.app().service.is_busy(),
        next_picture,
        d.snapshot(),
    )?;
    d.click("Previous  h")?;
    d.command("sounds")?;
    d.settled()?;
    d.check(
        "Picture Previous restores the edit cursor and sounds can be focused again",
        targets(d) == *baseline
            && d.app().event_focused()
            && d.app().selected_event.as_ref() == Some(id)
            && d.revision() == revision
            && document(d)?.sounds() == &sounds,
        json!({"revision":revision,"sounds":sounds,"targets":baseline}),
        d.snapshot(),
    )?;
    Ok(())
}

fn command_targets(d: &mut Driver<'_>, id: &SoundId, baseline: &Value) -> Result<(), String> {
    choose(d, id)?;
    rejected(
        d,
        "delete",
        "The delete command cannot alias deletion of a focused sound",
    )?;
    d.check(
        "The ambiguous delete command directs users to sound-delete",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains(":sound-delete")),
        json!("Explicit :sound-delete guidance"),
        d.snapshot(),
    )?;
    for (captured, mutation) in [
        (false, "sound-delete"),
        (false, "delete"),
        (true, "sound-gain -9"),
    ] {
        choose(d, id)?;
        let before = d.revision();
        d.app_mut().feedback.hold_project_updates = true;
        d.command(if captured {
            "sound-gain -6"
        } else {
            "sound-gain 0"
        })?;
        d.wait_for(
            "Sound writer finishes while its UI completion is held",
            |app| !app.service.is_busy(),
        )?;
        if !captured {
            d.click("Browse  :source")?;
        }
        d.key(Key::Colon)?;
        d.events(
            "Type a sound mutation before the held revision arrives",
            vec![egui::Event::Text(mutation.into())],
        )?;
        d.check(
            "Command entry captures sound focus before the late completion",
            d.app().command_open && d.app().sound_command_target.is_some() == captured,
            json!({"captured_sound":captured,"revision":before}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&before)?;
        d.check(
            "Late sound completion leaves the open command's original target intact",
            d.app().command_open
                && d.app().sound_command_target.is_some() == captured
                && d.app().selected_event.as_ref() == Some(id),
            json!({"captured_sound":captured,"selected_event":id}),
            d.snapshot(),
        )?;
        let committed_revision = d.revision();
        let sounds = document(d)?.sounds().clone();
        d.key(Key::Enter)?;
        d.wait_for(
            "Changed or absent sound command target rejects explicitly",
            |app| !app.service.is_busy() && app.error.is_some(),
        )?;
        d.check(
            if captured {
                "A captured sound command rejects the newly committed revision"
            } else if mutation == "delete" {
                "The delete alias cannot adopt a late selected event or cut the retained beat"
            } else {
                "A command opened without sound focus cannot adopt a late selected event"
            },
            !d.app().command_open
                && d.revision() == committed_revision
                && document(d)?.sounds() == &sounds
                && targets(d) == *baseline,
            json!({"revision":committed_revision,"sounds":sounds,"targets":baseline}),
            d.snapshot(),
        )?;
        if mutation == "delete" {
            d.check(
                "Late delete alias rejection retains explicit sound-delete guidance",
                d.app()
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains(":sound-delete")),
                json!("Explicit :sound-delete guidance"),
                d.snapshot(),
            )?;
        }
        commit(d, "sound-gain -3")?;
    }
    Ok(())
}

fn choose(d: &mut Driver<'_>, id: &SoundId) -> Result<(), String> {
    let index = document(d)?
        .sounds()
        .keys()
        .position(|candidate| candidate == id)
        .ok_or("Cannot select an absent sound event")?;
    d.command("sounds")?;
    d.chord(&[Key::Num6, Key::Num4, Key::K])?;
    for _ in 0..index {
        d.key(Key::J)?;
    }
    Ok(())
}

fn event(d: &Driver<'_>, id: &SoundId) -> Result<SoundEvent, String> {
    document(d)?
        .sounds()
        .get(id)
        .cloned()
        .ok_or_else(|| format!("Missing placed sound {id:?}"))
}

fn targets(d: &Driver<'_>) -> Value {
    let snapshot = d.snapshot();
    json!({
        "context":snapshot["context"],"selected_source":snapshot["selected_source"],
        "source_cursor":snapshot["source_cursor"],"sequence_cursor":snapshot["sequence_cursor"],
        "selected_beat":snapshot["selected_beat"],"sequence_scope":snapshot["sequence_scope"],
        "original_selection":snapshot["original_selection"],"visual_selection":snapshot["visual_selection"],
        "duration":snapshot["duration"],
    })
}

fn commit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let before = d.revision();
    d.command(command)?;
    d.changed(&before)
}

fn check_offset(d: &mut Driver<'_>, id: &SoundId, offset: i64, label: &str) -> Result<(), String> {
    let recipe = event(d, id)?;
    d.check(
        label,
        recipe.offset == AudioSample(offset),
        json!(offset),
        json!(recipe),
    )
}

fn audible(d: &Driver<'_>, id: &SoundId) -> Result<std::ops::Range<AudioSample>, String> {
    d.app()
        .workspace
        .as_ref()
        .ok_or("No workspace for sound onset")?
        .plan
        .root_sound(id)
        .map(|sound| sound.audible_samples())
        .map_err(|error| error.to_string())
}

fn check_onset(d: &mut Driver<'_>, id: &SoundId, onset: i64, label: &str) -> Result<(), String> {
    let samples = audible(d, id)?;
    d.check(
        label,
        samples.start == AudioSample(onset),
        json!(onset),
        json!(samples),
    )
}

fn same_mapping(left: &SoundEvent, right: &SoundEvent) -> Result<bool, String> {
    Ok(left.mapping.start_frames() == right.mapping.start_frames()
        && left
            .mapping
            .duration_frames(FrameDuration::ZERO)
            .map_err(|error| error.to_string())?
            == right
                .mapping
                .duration_frames(FrameDuration::ZERO)
                .map_err(|error| error.to_string())?
        && left
            .mapping
            .selection_frames(FrameDuration::ZERO)
            .map_err(|error| error.to_string())?
            == right
                .mapping
                .selection_frames(FrameDuration::ZERO)
                .map_err(|error| error.to_string())?)
}

fn check_gain(d: &mut Driver<'_>, id: &SoundId, gain: i32) -> Result<(), String> {
    let recipe = event(d, id)?;
    d.check(
        "Authored sound gain is exact in millidecibels",
        recipe.gain_millidecibels == gain,
        json!(gain),
        json!(recipe),
    )
}

fn rejected(d: &mut Driver<'_>, command: &str, label: &str) -> Result<(), String> {
    let before = d.revision();
    let sounds = document(d)?.sounds().clone();
    let routes = document(d)?.sound_routes().clone();
    let nodes = document(d)?.nodes().clone();
    d.command(command)?;
    d.wait_for("Rejected sound edit reports its failure", |app| {
        !app.service.is_busy() && (app.project_error.is_some() || app.error.is_some())
    })?;
    d.check(
        label,
        d.revision() == before
            && document(d)?.sounds() == &sounds
            && document(d)?.sound_routes() == &routes
            && document(d)?.nodes() == &nodes,
        json!({"revision":before,"sounds":sounds,"routes":routes}),
        d.snapshot(),
    )
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Sound-placement resize has no root viewport")?
        .inner_rect = Some(rect);
    Ok(())
}

fn visible(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Placed-sound controls paint fully inside their clip and viewport",
        !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
        json!(label),
        json!(paint),
    )
}

fn reveal(d: &mut Driver<'_>, label: &str, direction: f32) -> Result<(), String> {
    for attempt in 0..=3 {
        let paint = scenarios::text_paint_visibility(d, label);
        if !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true) {
            return Ok(());
        }
        if attempt == 3 {
            break;
        }
        let heading = d.rect("Selected sound inspector pane")?;
        let point = heading.center() + egui::vec2(0.0, 100.0);
        d.events(
            &format!("Scroll sound inspector to reveal {label}"),
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, direction),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        for _ in 0..8 {
            d.step("Sound inspector scroll settles", false)?;
        }
    }
    visible(d, label)
}
