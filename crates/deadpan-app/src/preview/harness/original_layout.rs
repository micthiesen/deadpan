//! Compact Original layout through production input, media and project services.

use super::*;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Original layout uses the real qualified picture and durable sound placement. Preparing/Playing uses injected typed delivery updates; no audio device, PCM preparation, physical keyboard, OS IME, or listening result is claimed.".into(),
    );
    if long_transport(d) {
        d.check(
            "Long transport paths are admitted before replay; complete paths need a second Monitor row at compact width",
            !d.app().keymap_error
                && d.app().keymap_status().starts_with("Custom editor keys")
                && d.app().keymap_status().contains("Documents/keymap.json")
                && d.app().editor_key(EditorKey::Playback) == "F7 F8 F9 F10 F11 F12"
                && d.app().editor_key(EditorKey::Audition) == "F15 F16 F17 F18 F19 F20",
            json!({"playback":"F7 F8 F9 F10 F11 F12","audition":"F15 F16 F17 F18 F19 F20",
                "minimum_picture_height":100,"layout_reason":"Keep complete action paths and hit areas; Monitor moves to a second row."}),
            json!({"status":d.app().keymap_status(),"playback":d.app().editor_key(EditorKey::Playback),
                "audition":d.app().editor_key(EditorKey::Audition)}),
        )?;
    }
    d.command("sequence")?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::L])?;
    select_original(d)?;
    d.check(
        "Layout starts with distinct nonzero clocks, a selected Original range and its copy",
        d.app().source_cursor == 20
            && d.app().sequence_cursor == 2
            && d.app().moment.range() == Some(10..20)
            && d.app()
                .copied
                .original()
                .is_some_and(|copy| copy.ordinals == (10..20))
            && sound_count(d)? == 0,
        json!({"Original":20,"edit":2,"selection":[10,20],"copy":[10,20],"sounds":0}),
        d.snapshot(),
    )?;
    layout_sizes(d, 0)?;
    resize(d, 960.0, 640.0, 1.0)?;
    d.step("Empty compact Original before Sounds entry", true)?;
    d.settled()?;
    focus_summary(d, false, 0)?;

    // Registration and placement run through the actual dialog, qualification
    // worker and project service. No fabricated row or document is installed.
    resize(d, 1280.0, 820.0, 1.0)?;
    d.step("Default workspace for real sound registration", true)?;
    d.settled()?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    d.click("Add sound…  ⌘I")?;
    d.wait_for("Real sound is qualified in the catalog", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.settled()?;
    let catalog = d.app().sound_rows[0].clone();
    d.click(&catalog.1)?;
    d.step("Paint selected catalog sound controls", true)?;
    let before = d.revision();
    d.click("Place at edit cursor  ·  ,s")?;
    d.changed(&before)?;
    d.check(
        "The populated summary is backed by one committed sound at the retained edit cursor",
        sound_count(d)? == 1 && d.app().sequence_cursor == 2,
        json!({"sounds":1,"edit":2}),
        d.snapshot(),
    )?;
    select_original(d)?;
    layout_sizes(d, 1)?;
    resize(d, 960.0, 640.0, 1.0)?;
    d.step("Populated compact Original before Sounds entry", true)?;
    d.settled()?;
    focus_summary(d, false, 1)?;
    focus_summary(d, true, 1)?;
    transport(d)?;
    catalog_focus(d, &catalog.1)?;
    d.check(
        "Original layout replay leaves no transport or queued delivery running",
        d.app().transport.is_none() && d.app().feedback.playback_updates.is_empty(),
        json!("stopped transport and empty delivery queue"),
        d.snapshot(),
    )
}

fn transport_label(d: &Driver<'_>, action: &str, binding: EditorKey) -> String {
    format!("{action}  ·  {}", d.app().editor_key(binding))
}

fn long_transport(d: &Driver<'_>) -> bool {
    d.report.name == "original-layout-long"
}

fn stopped_picture_bound(d: &Driver<'_>) -> f32 {
    if long_transport(d) { 100.0 } else { 140.0 }
}

fn select_original(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.key(Key::Escape)?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for("Original layout copy is durable", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })?;
    d.settled()
}

fn layout_sizes(d: &mut Driver<'_>, count: usize) -> Result<(), String> {
    let minimum_picture = stopped_picture_bound(d);
    let state = editor_state(d);
    let pane = d.app().pane;
    let revision = d.revision();
    let picture = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    for (width, height, scale) in [
        (960.0, 640.0, 1.0),
        (960.0, 640.0, 2.0),
        (1280.0, 820.0, 1.0),
        (960.0, 640.0, 1.0),
    ] {
        idle(d)?;
        resize(d, width, height, scale)?;
        d.step(
            &format!("First Original resize: {width}x{height} at {scale}x with {count} sounds"),
            true,
        )?;
        retained_submission(d, &picture)?;
        d.check(
            "The first layout-only resize preserves pane, clocks, selection, copy, beat and revision",
            editor_state(d) == state && d.app().pane == pane && d.revision() == revision
                && d.harness.ctx.content_rect().size() == egui::vec2(width, height)
                && (d.harness.ctx.pixels_per_point() - scale).abs() < f32::EPSILON,
            json!({"state":state,"pane":format!("{pane:?}"),"revision":revision,"size":[width,height],"scale":scale}),
            d.snapshot(),
        )?;
        original_layout(d, count, minimum_picture, "Play Original", None)?;
        d.settled()?;
        d.capture(&format!(
            "Original layout: {width}x{height} at {scale}x with {count} placed sounds"
        ))?;
        original_layout(d, count, minimum_picture, "Play Original", None)?;
    }
    Ok(())
}

fn original_layout(
    d: &mut Driver<'_>,
    count: usize,
    minimum_picture: f32,
    playback: &str,
    status: Option<&str>,
) -> Result<(), String> {
    scenarios::viewer_visible(d)?;
    let viewer = picture_rect(d)?;
    let picture_paint = picture_paint(d, viewer)?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing Original layout workspace")?;
    let original_clock = format!(
        "Original {} f",
        workspace
            .original_duration
            .ok_or("Missing Original duration")?
            .frames(),
    );
    let edit_clock = format!("Your edit {} f", workspace.plan.duration().frames());
    let playback = transport_label(d, playback, EditorKey::Playback);
    let audition = transport_label(d, "Loop selection", EditorKey::Audition);
    let select = format!("Select moment  {}", d.app().editor_key(EditorKey::Visual));
    let copy = format!("Copy moment  {}", d.app().editor_key(EditorKey::Copy));
    let cancel = format!(
        "Cancel selection  {}",
        d.app().editor_key(EditorKey::Escape)
    );
    let mut labels = vec![
        "Start  gg",
        "Previous  h",
        "Next  l",
        "End  G",
        "In 10 · Out 20 excluded · 10 original frames",
        select.as_str(),
        copy.as_str(),
        cancel.as_str(),
        audition.as_str(),
        "Monitor · :monitor",
        playback.as_str(),
        original_clock.as_str(),
        edit_clock.as_str(),
    ];
    // The copied-register explanation follows the primary selection controls
    // in a scrollable inspector. At compact height its state is checked by
    // editor_state, while the visible endpoint fields and actions stay above
    // the fold. The full-height layout also exposes the explanation.
    if d.harness.ctx.content_rect().height() >= 700.0 {
        labels.push("Copied Original [10..20)");
    }
    if let Some(status) = status {
        labels.extend([status, "Loop context"]);
    } else if long_transport(d) {
        labels.push("Loop context");
    }
    let paint = labels
        .iter()
        .map(|label| (*label, scenarios::text_paint_visibility(d, label)))
        .collect::<Vec<_>>();
    let mut hits = sound_placement::control_hits(
        d,
        &[
            "Start  gg",
            "Previous  h",
            "Next  l",
            "End  G",
            select.as_str(),
            copy.as_str(),
            cancel.as_str(),
            audition.as_str(),
            "Monitor · :monitor",
            playback.as_str(),
        ],
    );
    // "Your edit" also names a disabled root breadcrumb. Scope these two
    // controls to the enabled tab row immediately above the picture.
    hits.extend([
        viewer_tab(d, "Original", viewer),
        viewer_tab(d, "Your edit", viewer),
    ]);
    d.check(
        "Original retains a usable painted picture with complete navigation, selection, transport and clock labels",
        picture_paint.valid && picture_paint.height >= minimum_picture
            && paint.iter().all(|(_, items)| !items.is_empty()
                && items.iter().all(|item| item["fully_visible"] == true))
            && hits.iter().all(|hit| hit["complete_hit"] == true
                && hit["enabled"] == (hit["label"] != "Monitor · :monitor" || d.app().transport.is_none())),
        json!({"minimum_picture_height":minimum_picture,"complete_labels":labels}),
        json!({"viewer_height":viewer.height(),"picture_height":picture_paint.height,
            "picture_paint":picture_paint.details,"paint":paint,"hits":hits}),
    )?;
    if long_transport(d) && status.is_none() && d.harness.ctx.content_rect().height() < 700.0 {
        let play = d.rect(&playback)?;
        let audition = d.rect(&audition)?;
        let monitor = d.rect("Monitor · :monitor")?;
        d.check(
            "Long stopped action paths retain whole hit areas above the separate Monitor row",
            monitor.top() >= play.bottom() && monitor.top() >= audition.bottom()
                && !monitor.intersects(play) && !monitor.intersects(audition),
            json!({"Monitor":"second row below both complete action controls","minimum_picture_height":100}),
            json!({"play":format!("{play:?}"),"audition":format!("{audition:?}"),"monitor":format!("{monitor:?}")}),
        )?;
    }
    if d.harness.ctx.content_rect().height() < 700.0 {
        summary(d, count)?;
    } else {
        d.check(
            "Default-height Original retains the full Sounds panel beneath Beats",
            d.rect("Placed sounds pane")?.top()
                > d.rect("Current group beat outline pane")?.bottom(),
            json!("Separate full Sounds panel below Beats"),
            d.widgets(),
        )?;
    }
    scenarios::footer_anchored(d, "Original controls leave the footer fully anchored")
}

fn viewer_tab(d: &Driver<'_>, label: &str, viewer: egui::Rect) -> Value {
    let controls = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            if access.role() != egui::accesskit::Role::Button
                || access.label().as_deref() != Some(label)
                || access.is_disabled()
                || access.is_hidden()
                || access.bounding_box().is_none()
            {
                return None;
            }
            let rect = node.rect();
            (rect.is_positive()
                && rect.bottom() <= viewer.top()
                && rect.left() >= viewer.left()
                && rect.right() <= viewer.right())
            .then_some(rect)
        })
        .collect::<Vec<_>>();
    let Some(rect) = controls.first().filter(|_| controls.len() == 1).copied() else {
        return json!({"label":label,"enabled":false,"complete_hit":false,"rects":format!("{controls:?}")});
    };
    let paint = scenarios::text_paint_visibility(d, label)
        .into_iter()
        .filter(|item| {
            let Some(bounds) = item["bounds"].as_array() else {
                return false;
            };
            let coordinates = bounds.iter().filter_map(Value::as_f64).collect::<Vec<_>>();
            item["text"] == label
                && coordinates.len() == 4
                && coordinates[0] >= f64::from(rect.left())
                && coordinates[2] <= f64::from(rect.right())
                && coordinates[1] >= f64::from(rect.top())
                && coordinates[3] <= f64::from(rect.bottom())
        })
        .collect::<Vec<_>>();
    let complete = d.harness.ctx.content_rect().contains_rect(rect)
        && d.harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(text)
                if text.galley.text() == label && rect.contains_rect(text.visual_bounding_rect()))
                && clipped.clip_rect.contains_rect(rect)
        })
        && !paint.is_empty()
        && paint.iter().all(|item| item["fully_visible"] == true);
    json!({"label":label,"enabled":true,"complete_hit":complete,"rect":format!("{rect:?}"),"paint":paint})
}

fn summary(d: &mut Driver<'_>, count: usize) -> Result<(), String> {
    let label = if count == 0 {
        "PLACED SOUNDS 0 · ,s place".to_owned()
    } else {
        format!("PLACED SOUNDS {count} · :sounds")
    };
    let sounds = d.rect("Placed sounds pane")?;
    let beats = d.rect("Current group beat outline pane")?;
    let paint = scenarios::text_paint_visibility(d, &label);
    let complete_hit = d.harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text().contains(&label))
            && clipped.clip_rect.contains_rect(sounds)
    });
    d.check(
        "Compact Original shows the actual placed-sound count in a separate painted focus target",
        sound_count(d)? == count
            && !sounds.intersects(beats)
            && (sounds.center().y - beats.center().y).abs() <= 14.0
            && d.harness.ctx.content_rect().contains_rect(sounds)
            && complete_hit
            && !paint.is_empty()
            && paint.iter().all(|item| item["fully_visible"] == true),
        json!({"label":label,"actual_count":count,"separate_focus_target":true}),
        json!({"sounds":format!("{sounds:?}"),"beats":format!("{beats:?}"),"paint":paint}),
    )
}

fn focus_summary(d: &mut Driver<'_>, keyboard: bool, count: usize) -> Result<(), String> {
    // :source preserves Viewer focus when already browsing Original. Select
    // the real Sources heading before testing its reverse-Tab neighbor.
    d.command("source")?;
    d.settled()?;
    if keyboard {
        let state = editor_state(d);
        let revision = d.revision();
        d.click("Original and sounds pane")?;
        d.check(
            "The Original pane heading establishes Sources focus before reverse Tab",
            d.app().pane == Pane::Sources
                && d.harness
                    .ctx
                    .memory(|memory| memory.has_focus(pane_id(Pane::Sources)))
                && d.app().view == View::Source
                && editor_state(d) == state
                && d.revision() == revision,
            json!({"pane":"Sources","view":"Source","state":state,"revision":revision}),
            d.snapshot(),
        )?;
    }
    idle(d)?;
    let state = editor_state(d);
    let revision = d.revision();
    let retained = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    if keyboard {
        d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    } else {
        d.click("Placed sounds pane")?;
    }
    retained_submission(d, &retained)?;
    d.check(
        "Sounds summary entry changes view and focus on its input frame while preserving both clocks and copied time",
        d.app().view == View::Sequence && d.app().pane == Pane::Sounds
            && d.harness.ctx.memory(|memory| memory.has_focus(pane_id(Pane::Sounds)))
            && editor_state(d) == state && d.revision() == revision,
        json!({"view":"Sequence","pane":"Sounds","state":state,"revision":revision,"keyboard":keyboard}),
        d.snapshot(),
    )?;
    if count > 0 {
        let paint = scenarios::text_paint_visibility(d, "PLACED SOUNDS");
        let card = d
            .harness
            .root()
            .children_recursive()
            .find_map(|node| {
                let access = node.accesskit_node();
                let label = access.label()?;
                (access.role() == egui::accesskit::Role::Button && label.starts_with("♫  "))
                    .then(|| label.to_string())
            })
            .ok_or("Opening populated Sounds did not paint a sound row")?;
        let hits = sound_placement::control_hits(d, &[&card]);
        d.check(
            "Populated summary entry restores the actual sound list on the first input frame",
            d.rect("Placed sounds pane")?.top()
                > d.rect("Current group beat outline pane")?.bottom()
                && !paint.is_empty()
                && paint.iter().all(|item| item["fully_visible"] == true)
                && hits
                    .iter()
                    .all(|hit| hit["complete_hit"] == true && hit["enabled"] == true),
            json!("Visible enabled sound row below Beats"),
            json!({"paint":paint,"hits":hits}),
        )?;
    }
    d.settled()?;
    idle(d)?;
    d.click("Original")?;
    replacing_tab(d, View::Source)?;
    d.check(
        "Returning through the Original tab retains its selected time and independent edit cursor",
        editor_state(d) == state && d.revision() == revision && d.app().pane == Pane::Viewer,
        json!({"state":state,"revision":revision,"pane":"Viewer"}),
        d.snapshot(),
    )?;
    let minimum_picture = stopped_picture_bound(d);
    original_layout(d, count, minimum_picture, "Play Original", None)
}

fn transport(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.settled()?;
    idle(d)?;
    let state = editor_state(d);
    let revision = d.revision();
    let retained = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    let control = d.rect(&transport_label(d, "Play Original", EditorKey::Playback))?;
    let position = control.center();
    d.events(
        "Press Original Play before a same-coordinate scale change",
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    resize(d, 960.0, 640.0, 2.0)?;
    d.events(
        "Release Original Play at the same logical point with 2x native scale",
        vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    )?;
    retained_submission(d, &retained)?;
    d.check(
        "Original Play enters Preparing on the scale-change release frame at the unchanged logical point",
        d.app().transport.as_ref().is_some_and(|run| {
            run.phase == deadpan_playback::Phase::Preparing
                && matches!(run.domain(), crate::transport::Domain::Original(_))
        }) && editor_state(d) == state
            && d.revision() == revision
            && d.harness.ctx.content_rect().size() == egui::vec2(960.0, 640.0)
            && (d.harness.ctx.pixels_per_point() - 2.0).abs() < f32::EPSILON,
        json!({"phase":"Preparing","state":state,"revision":revision,"scale":2,
            "size":[960,640],"press_and_release":[position.x,position.y],"pressed_control":format!("{control:?}")}),
        d.snapshot(),
    )?;
    original_layout(d, 1, 100.0, "Cancel preparation", Some("Preparing ·"))?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Missing Original preparation")?;
    let generation = feed
        .restart(run.sample.0)
        .map_err(|error| error.to_string())?;
    let playing = deadpan_playback::Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase: deadpan_playback::Phase::Playing,
        sample: Some(run.sample),
        generation: Some(generation),
        error: None,
    };
    d.app_mut().feedback.playback_updates.push_back(playing);
    d.step("First injected Playing feedback in compact Original", true)?;
    d.check(
        "Playing delivery at the captured sample preserves independent editor clocks and selection",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == deadpan_playback::Phase::Playing)
            && editor_state(d) == state
            && d.revision() == revision,
        json!({"phase":"Playing","state":state,"revision":revision}),
        d.snapshot(),
    )?;
    original_layout(d, 1, 100.0, "Pause", Some("Playing ·"))?;
    // Navigation revokes the simulated run and its resume authority. It also
    // exercises the compact summary-to-list transition while playback is live.
    d.command("sounds")?;
    d.check(
        "Entering Sounds stops Original playback without moving either retained cursor",
        d.app().transport.is_none()
            && d.app().resume.is_none()
            && d.app().view == View::Sequence
            && d.app().pane == Pane::Sounds
            && editor_state(d) == state
            && d.revision() == revision,
        json!({"stopped":true,"resume":null,"state":state}),
        d.snapshot(),
    )?;
    d.command("source")?;
    d.settled()?;
    let minimum_picture = stopped_picture_bound(d);
    original_layout(d, 1, minimum_picture, "Play Original", None)?;
    native_transport_activations(d)
}

fn native_transport_activations(d: &mut Driver<'_>) -> Result<(), String> {
    let state = editor_state(d);
    let revision = d.revision();
    for (keyboard, scale) in [(true, 1.0), (false, 2.0)] {
        idle(d)?;
        let label = transport_label(d, "Play Original", EditorKey::Playback);
        if keyboard {
            // Follow the same AccessKit focus route as the native-control
            // ownership replays. Do not set egui focus memory directly.
            d.harness.get_by_label(label.as_str()).focus();
            d.step(
                "Accessibility focus selects Original Play before native Enter",
                true,
            )?;
            let focused = d.harness.root().children_recursive().any(|node| {
                let access = node.accesskit_node();
                access.role() == egui::accesskit::Role::Button
                    && access.label().as_deref() == Some(label.as_str())
                    && access.is_focused()
                    && !access.is_disabled()
                    && !access.is_hidden()
            });
            d.check(
                "The actual Play button owns native Enter before the scale transition",
                focused
                    && native_control_focused(&d.harness.ctx)
                    && d.app().transport.is_none()
                    && editor_state(d) == state
                    && d.revision() == revision,
                json!({"focused_control":label,"state":state,"revision":revision}),
                d.snapshot(),
            )?;
        }
        idle(d)?;
        let retained = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
        let previous_scale = d.harness.ctx.pixels_per_point();
        resize(d, 960.0, 640.0, scale)?;
        if keyboard {
            d.events(
                "Native focused Play Enter and same-point viewport resize to 1x",
                vec![
                    key_event(Key::Enter, Modifiers::NONE, true),
                    key_event(Key::Enter, Modifiers::NONE, false),
                ],
            )?;
        } else {
            d.harness.get_by_label(label.as_str()).click_accesskit();
            d.step(
                "AccessKit Play Click and same-point viewport resize to 2x",
                true,
            )?;
        }
        retained_submission(d, &retained)?;
        d.check(
            "Native Play activation submits only final geometry and enters Preparing on the scale-change frame",
            d.app().transport.as_ref().is_some_and(|run| {
                run.phase == deadpan_playback::Phase::Preparing
                    && matches!(run.domain(), crate::transport::Domain::Original(_))
            }) && editor_state(d) == state && d.revision() == revision
                && previous_scale != scale
                && (d.harness.ctx.pixels_per_point() - scale).abs() < f32::EPSILON
                && d.harness.ctx.content_rect().size() == egui::vec2(960.0, 640.0),
            json!({"activation":if keyboard {"focused Enter"} else {"AccessKit Click"},
                "phase":"Preparing","previous_scale":previous_scale,"scale":scale,
                "size":[960,640],"state":state,"revision":revision}),
            d.snapshot(),
        )?;
        original_layout(d, 1, 100.0, "Cancel preparation", Some("Preparing ·"))?;
        d.command("source")?;
        d.settled()?;
        d.check(
            "Navigation stops native-activated preparation and clears resume without changing editor targets",
            d.app().transport.is_none() && d.app().resume.is_none()
                && editor_state(d) == state && d.revision() == revision,
            json!({"stopped":true,"resume":null,"state":state,"revision":revision}),
            d.snapshot(),
        )?;
        let minimum_picture = stopped_picture_bound(d);
        original_layout(d, 1, minimum_picture, "Play Original", None)?;
    }
    Ok(())
}

fn catalog_focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    idle(d)?;
    let state = editor_state(d);
    let revision = d.revision();
    let picture = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    d.click(label)?;
    retained_submission(d, &picture)?;
    d.step(
        "Compact Original with the qualified catalog sound focused",
        true,
    )?;
    let play = transport_label(d, "Play sound", EditorKey::Playback);
    let audition = transport_label(d, "Loop sound", EditorKey::Audition);
    let place = transport_label(d, "Place at edit cursor", EditorKey::PlaceSound);
    let hits = sound_placement::control_hits(d, &[&play, &audition, &place]);
    let paint = [&play, &audition, &place].map(|label| scenarios::text_paint_visibility(d, label));
    d.check(
        "Catalog focus leaves the Original picture and both editing clocks intact with complete sound controls",
        d.app().sound_focused() && d.app().view == View::Source
            && editor_state(d) == state && d.revision() == revision
            && d.app().presentation.diagnostic_snapshot()["displayed"] == picture
            && hits.iter().all(|hit| hit["complete_hit"] == true && hit["enabled"] == true)
            && paint.iter().all(|items| !items.is_empty()
                && items.iter().all(|item| item["fully_visible"] == true)),
        json!({"state":state,"revision":revision,"picture":picture}),
        json!({"state":d.snapshot(),"hits":hits,"paint":paint}),
    )?;
    scenarios::viewer_visible(d)?;
    summary(d, 1)?;
    scenarios::footer_anchored(d, "Catalog focus preserves the compact Original footer")?;
    d.command("source")?;
    d.settled()
}

fn replacing_tab(d: &mut Driver<'_>, view: View) -> Result<(), String> {
    let picture = d.app().presentation.diagnostic_snapshot();
    let submissions = frame_submissions(d)?;
    let label = format!(
        "Showing source frame {}",
        u128::from(d.app().source_cursor) + 1
    );
    d.check(
        "The Original tab requests the retained source coordinate without submitting a discarded frame",
        d.app().view == view && submissions == 0 && d.app().target.is_none()
            && picture["decoded"].is_null() && picture["displayed"].is_null()
            && picture["loading"] == true && picture["requested"]["label"] == label,
        json!({"submissions":0,"requested_label":label,"displayed":null}),
        json!({"submissions":submissions,"picture":picture}),
    )?;
    d.settled()?;
    let viewer = picture_rect(d)?;
    let expected = target_size(
        fitted_picture(d, viewer).size(),
        d.harness.ctx.pixels_per_point(),
    );
    let actual = target(d);
    d.check(
        "The returned Original picture uses the final compact target size",
        actual == Some(expected),
        json!(expected),
        json!(actual),
    )
}

fn retained_submission(d: &mut Driver<'_>, retained: &Value) -> Result<(), String> {
    let submissions = frame_submissions(d)?;
    let viewer = picture_rect(d)?;
    let expected = target_size(
        fitted_picture(d, viewer).size(),
        d.harness.ctx.pixels_per_point(),
    );
    let actual = target(d);
    let picture = d.app().presentation.diagnostic_snapshot();
    let paint = picture_paint(d, viewer)?;
    d.check(
        "The transition submits at most one final-size GPU target and preserves the displayed texture's aspect and identity",
        submissions <= 1 && (submissions == 0 || actual == Some(expected))
            && picture["displayed"] == *retained && paint.valid,
        json!({"maximum_submissions":1,"submitted_target":expected,"retained":retained}),
        json!({"submissions":submissions,"actual_target":actual,"picture":picture,"paint":paint.details}),
    )
}

struct PicturePaint {
    height: f32,
    valid: bool,
    details: Value,
}

fn picture_paint(d: &Driver<'_>, viewer: egui::Rect) -> Result<PicturePaint, String> {
    let target = d
        .app()
        .target
        .as_ref()
        .ok_or("Original layout has no retained GPU target")?;
    let raster = egui::vec2(target.target.width() as f32, target.target.height() as f32);
    // Derive the expected placement independently from the texture that is
    // actually painted. The requested canvas may already have different
    // geometry while this older target remains visible.
    let ratio = (viewer.width() / raster.x).min(viewer.height() / raster.y);
    let expected = egui::Rect::from_center_size(viewer.center(), raster * ratio);
    let pixels_per_point = d.harness.ctx.pixels_per_point();
    let viewport = d.harness.ctx.content_rect();
    let rect = |rect: egui::Rect| [rect.min.x, rect.min.y, rect.max.x, rect.max.y];
    let mut height = f32::INFINITY;
    let mut valid = true;
    let mut meshes = Vec::new();
    for clipped in &d.harness.output().shapes {
        let egui::Shape::Mesh(mesh) = &clipped.shape else {
            continue;
        };
        if mesh.texture_id != target.texture {
            continue;
        }
        let bounds = mesh.calc_bounds();
        let centered_fit = bounds.min.distance(expected.min) * pixels_per_point <= 0.5
            && bounds.max.distance(expected.max) * pixels_per_point <= 0.5;
        let aspect_error_pixels =
            (bounds.width() - bounds.height() * raster.x / raster.y).abs() * pixels_per_point;
        let visible = scenarios::picture_contains_rect(clipped.clip_rect, bounds, pixels_per_point)
            && scenarios::picture_contains_rect(viewer, bounds, pixels_per_point)
            && scenarios::picture_contains_rect(viewport, bounds, pixels_per_point);
        valid &= centered_fit && visible && aspect_error_pixels <= 0.5;
        height = height.min(bounds.height());
        meshes.push(json!({"bounds":rect(bounds),"clip":rect(clipped.clip_rect),
            "centered_fit":centered_fit,"visible":visible,"aspect_error_pixels":aspect_error_pixels}));
    }
    if meshes.is_empty() {
        valid = false;
        height = 0.0;
    }
    Ok(PicturePaint {
        height,
        valid,
        details: json!({"target_raster":[raster.x,raster.y],"viewer":rect(viewer),
            "expected_retained_fit":rect(expected),"maximum_edge_error_pixels":0.5,
            "maximum_aspect_error_pixels":0.5,"meshes":meshes}),
    })
}

fn frame_submissions(d: &Driver<'_>) -> Result<usize, String> {
    d.report
        .steps
        .last()
        .ok_or("Missing Original transition frame")?
        .semantic["stages"]
        .as_array()
        .ok_or_else(|| "Missing Original transition stages".to_owned())
        .map(|stages| {
            stages
                .iter()
                .filter(|stage| stage["stage"] == "picture_submitted")
                .count()
        })
}

fn picture_rect(d: &Driver<'_>) -> Result<egui::Rect, String> {
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Original layout lost its picture")?;
    d.rect(&label)
}

fn fitted_picture(d: &Driver<'_>, viewer: egui::Rect) -> egui::Rect {
    d.app()
        .presentation
        .canvas()
        .map_or(viewer, |(width, height)| {
            fit_rect(viewer, width as f32 / height as f32)
        })
}

fn target(d: &Driver<'_>) -> Option<(u32, u32)> {
    d.app()
        .target
        .as_ref()
        .map(|target| (target.target.width(), target.target.height()))
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Renderer is idle before Original layout transition",
        |app| app.renderer.is_idle().is_ok_and(|idle| idle),
    )
}

fn editor_state(d: &Driver<'_>) -> Value {
    let state = d.snapshot();
    json!({
        "selected_source":state["selected_source"],"source_cursor":state["source_cursor"],
        "sequence_cursor":state["sequence_cursor"],"selected_beat":state["selected_beat"],
        "sequence_scope":state["sequence_scope"],"original_selection":state["original_selection"],
        "visual_selection":state["visual_selection"],"copied_moment":state["copied_moment"],
        "edit_selection":state["edit_selection"],"duration":state["duration"],
    })
}

fn sound_count(d: &Driver<'_>) -> Result<usize, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.sounds().len())
        .ok_or_else(|| "Original layout lost its sound document".into())
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32, scale: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    let viewport = input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Original layout resize has no root viewport")?;
    viewport.inner_rect = Some(rect);
    viewport.native_pixels_per_point = Some(scale);
    Ok(())
}
