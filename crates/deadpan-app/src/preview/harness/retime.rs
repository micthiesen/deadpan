//! Production command and inspector input for structural speed editing.

use super::*;
use deadpan_core::{
    ExactRatio, HoldAudio, HoldVideo, NodeKind, PitchPolicy, ProjectFrame, RetimePurpose,
};
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let initial = d.app().workspace.as_ref().ok_or("No project")?.clone();
    let original = d.app().selected_beat.clone().ok_or("No selected beat")?;
    let revision = d.revision();
    let initial_length = d.app().sequence_length();
    let expected = crate::project::retime::resolve(
        &initial,
        &original,
        ExactRatio::new(3, 4).map_err(|error| error.to_string())?,
        PitchPolicy::Preserve,
        false,
    )?;
    scenarios::reveal_inspector_button(d, "Change speed…  :retime")?;
    d.click("Change speed…  :retime")?;
    d.capture("Speed entry previews exact duration and pitch before applying")?;
    let hint = expected.describe(PitchPolicy::Preserve);
    let hint_paint = scenarios::text_paint_visibility(d, &hint);
    d.check(
        "Inspector speed entry teaches command and quantized duration without a revision",
        d.app().command_open
            && d.app().command == "retime 0.75 pitch=preserve"
            && d.app().pane == Pane::Inspector
            && !hint_paint.is_empty()
            && hint_paint
                .iter()
                .all(|paint| paint["fully_visible"] == true)
            && d.revision() == revision,
        json!({"command":"retime 0.75 pitch=preserve", "preview":hint, "revision":revision}),
        json!({"state":d.snapshot(),"hint_paint":hint_paint}),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels speed entry without changing the selected source",
        !d.app().command_open
            && d.revision() == revision
            && d.app().selected_beat.as_ref() == Some(&original),
        json!("unchanged Source and history"),
        d.snapshot(),
    )?;

    d.command("retime 0.75 pitch=preserve")?;
    d.changed(&revision)?;
    let retimed = d.app().selected_beat.clone().ok_or("Retime not selected")?;
    d.capture("Retime remains an editable speed stage over the Original")?;
    d.check("One speed command wraps the unchanged Original and selects its saved Retime",
        retimed != original && d.app().sequence_length() == expected.after.frames() as u64
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.nodes()[&original] == initial.document.nodes()[&original]
                    && matches!(&workspace.document.nodes()[&retimed].kind, NodeKind::Retime { child, pitch: PitchPolicy::Preserve, purpose: RetimePurpose::Edit, .. } if child == &original)
            }), json!({"frames":expected.after.frames(), "pitch":"preserve", "input":original.as_str()}), d.snapshot())?;

    let before_update = d.revision();
    scenarios::reveal_inspector_button(d, "Change speed…  Enter")?;
    d.click("Change speed…  Enter")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Replace speed and pitch through native-style command text editing",
        vec![egui::Event::Text("retime 0.5 pitch=tape".into())],
    )?;
    d.capture("Tape-speed preview retains the original input extent")?;
    d.key(Key::Enter)?;
    d.changed(&before_update)?;
    d.check("Pointer entry updates the same Retime from its retained input, without compounding its prior speed",
        d.app().selected_beat.as_ref() == Some(&retimed) && d.app().sequence_length() == initial_length * 2
            && d.app().workspace.as_ref().is_some_and(|workspace| matches!(&workspace.document.nodes()[&retimed].kind, NodeKind::Retime { pitch: PitchPolicy::FollowSpeed, .. })),
        json!({"frames":initial_length * 2, "pitch":"tape", "selected":retimed.as_str()}), d.snapshot())?;

    let before_nested = d.revision();
    d.command("wrap-retime 2 pitch=preserve")?;
    d.changed(&before_nested)?;
    let outer = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Outer Retime not selected")?;
    d.check("Explicit wrap-retime creates a second editable stage", outer != retimed && d.app().sequence_length() == initial_length
        && d.app().workspace.as_ref().is_some_and(|workspace| matches!(&workspace.document.nodes()[&outer].kind, NodeKind::Retime { child, pitch: PitchPolicy::Preserve, .. } if child == &retimed)),
        json!({"frames":initial_length, "nested":true}), d.snapshot())?;
    for _ in 0..3 {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
    }
    d.check(
        "Three undos restore the protected full Original baseline",
        d.app().sequence_length() == initial_length
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.nodes() == initial.document.nodes() && !workspace.can_undo
            }),
        json!("exact Original structure, protected baseline"),
        d.snapshot(),
    )?;
    d.command("source")?;
    let before_source = d.revision();
    d.command("retime 2 pitch=tape")?;
    d.capture("Original context keeps speed changes non-destructive")?;
    d.check(
        "A speed command in Original never modifies the project",
        d.revision() == before_source && d.app().sequence_length() == initial_length,
        json!(before_source),
        d.snapshot(),
    )?;
    nested_pause(d)
}

fn nested_pause(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    let before = d.revision();
    d.command("retime 0.5 pitch=tape")?;
    d.changed(&before)?;
    let inner = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Inner tape stage missing")?;
    let before = d.revision();
    d.command("wrap-retime 1.5 pitch=tape")?;
    d.changed(&before)?;
    let outer = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Outer tape stage missing")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num7, Key::L])?;
    d.settled()?;
    let baseline = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Nested tape workspace missing")?
        .clone();
    let original_cursor = d.app().source_cursor;
    let left = baseline
        .plan
        .picture(ProjectFrame(16))
        .map_err(|error| error.to_string())?;
    let (asset, _) = left
        .picture
        .follow_point()
        .ok_or("Nested tape left picture has no source")?;
    let index = baseline
        .sources
        .get(asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("Nested tape Original index missing")?;
    let measured = left
        .picture
        .select_source_frame(index)
        .map_err(|error| error.to_string())?;
    let expected_video = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let before = d.revision();
    d.command("hold 3f")?;
    d.changed(&before)?;
    d.settled()?;
    let changed = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Nested pause workspace missing")?
        .clone();
    let hold = changed
        .document
        .nodes()
        .iter()
        .find_map(|(id, node)| {
            (!baseline.document.nodes().contains_key(id))
                .then_some(node)
                .and_then(|node| {
                    if let NodeKind::Hold { recipe } = &node.kind {
                        Some((id.clone(), recipe.clone()))
                    } else {
                        None
                    }
                })
        })
        .ok_or("Nested tape pause did not save a Hold")?;
    let slope = |id: &deadpan_core::NodeId| -> Result<ExactRatio, String> {
        let clock = changed.document.derived_clocks().get(id);
        let NodeKind::Retime {
            duration,
            mapping,
            pitch: PitchPolicy::FollowSpeed,
            ..
        } = &changed.document.nodes()[id].kind
        else {
            return Err(format!("Tape stage {id} was replaced"));
        };
        let (start, end, extent) = clock.map_or(
            (
                ExactRatio::integer(mapping.start().0),
                ExactRatio::integer(mapping.end().0),
                ExactRatio::integer(duration.frames()),
            ),
            |clock| (clock.child_start, clock.child_end, clock.extent),
        );
        end.checked_sub(start)
            .and_then(|span| span.checked_div(extent))
            .map_err(|error| error.to_string())
    };
    let inner_slope = slope(&inner)?;
    let outer_slope = slope(&outer)?;
    let frozen = (17..20).all(|frame| {
        changed
            .plan
            .picture(ProjectFrame(frame))
            .is_ok_and(|picture| {
                picture
                    .picture
                    .select_source_frame(index)
                    .is_ok_and(|frame| frame == measured)
                    && picture.picture_context.as_deref() == hold.1.picture_context.as_ref()
            })
    });
    d.check(
        "A native pause under both live tape stages adds exactly three frames and freezes the measured left picture",
        changed.plan.duration().frames() == baseline.plan.duration().frames() + 3
            && inner_slope == ExactRatio::new(1, 2).map_err(|error| error.to_string())?
            && outer_slope == ExactRatio::new(3, 2).map_err(|error| error.to_string())?
            && hold.1.duration.frames() == 3 && hold.1.audio == HoldAudio::Silence
            && hold.1.video == expected_video
            && hold.1.picture_context.as_ref().is_some_and(|context| {
                context.canvases.len() == 1 && context.canvases[0].width == 320
                    && context.canvases[0].height == 180 && context.canvases[0].layers.is_empty()
            }) && frozen
            && d.app().source_cursor == original_cursor && d.app().sequence_cursor == 17,
        json!({"delta":3,"stage_slopes":["1/2","3/2"],"video":expected_video,"audio":"silence","captured_canvas":[320,180],"captured_layers":[],"Original":original_cursor}),
        json!({"state":d.snapshot(),"hold":hold,"slopes":[inner_slope,outer_slope],"frozen":frozen}),
    )?;
    let description = d
        .app()
        .inspector_description()
        .ok_or("Nested speed inspector missing")?;
    let speed_paint = scenarios::text_paint_visibility(d, "3/2×");
    d.check("The clocked inspector shows exact tape speed and offers a qualified update",
        description.fields.contains(&("Speed", "3/2×".into()))
            && description.parameter.as_ref().is_some_and(|(_, command)| command == "retime 3/2 pitch=tape")
            && description.note.contains("exact input span")
            && speed_paint.iter().any(|paint| paint["fully_visible"] == true),
        json!({"Speed":"3/2×","command":"retime 3/2 pitch=tape","explanation":"exact input span"}),
        json!({"fields":description.fields,"parameter":description.parameter,"note":description.note,"paint":speed_paint}))?;
    d.capture("Silent freeze inside two live tape-speed stages")?;
    clocked_update(d, &changed, &outer, original_cursor)?;
    let same_contents = |actual: &deadpan_core::ProjectDocument,
                         expected: &deadpan_core::ProjectDocument| {
        let mut value = serde_json::to_value(actual).expect("valid replay document");
        value["revision_id"] = serde_json::to_value(expected.revision_id()).expect("revision");
        value == serde_json::to_value(expected).expect("valid expected document")
    };
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "One Undo restores the complete pre-pause document, clocks and bindings",
        same_contents(
            &d.app().workspace.as_ref().unwrap().document,
            &baseline.document,
        ) && d.app().source_cursor == original_cursor,
        json!("exact pre-pause document"),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key_modified(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "One Redo restores the complete nested pause document, clocks and bindings",
        same_contents(
            &d.app().workspace.as_ref().unwrap().document,
            &changed.document,
        ) && d.app().source_cursor == original_cursor,
        json!("exact inserted document"),
        d.snapshot(),
    )?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num5, Key::L])?;
    let before = d.revision();
    let length = d.app().sequence_length();
    d.command("hold 1f video=black")?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "A later black pause also preserves the Original cursor and adds one exact frame",
        d.app().sequence_length() == length + 1
            && d.app().source_cursor == original_cursor
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace
                    .plan
                    .picture(ProjectFrame(25))
                    .is_ok_and(|picture| picture.picture == deadpan_plan::Picture::Background)
            }),
        json!({"delta":1,"Edit":25,"Original":original_cursor,"picture":"background"}),
        d.snapshot(),
    )?;
    let workspace = d.app().workspace.as_ref().unwrap();
    let description = d
        .app()
        .inspector_description()
        .ok_or("Speed inspector missing")?;
    let resolution = crate::project::retime::resolve(
        workspace,
        &outer,
        ExactRatio::ONE,
        PitchPolicy::FollowSpeed,
        false,
    );
    d.check(
        "An integral outer stage with fractional descendants still offers exact tape-speed preview",
        !workspace.document.derived_clocks().contains_key(&outer)
            && workspace.clocked_retimes.contains(&outer)
            && description.parameter.is_some()
            && description.note.contains("exact input span")
            && resolution.is_ok(),
        json!({"own_clock":false,"retained_descendant_clocks":true,"change_speed":true,"preview":"qualified"}),
        json!({"fields":description.fields,"parameter":description.parameter,"note":description.note,"preview":format!("{resolution:?}")}),
    )?;
    d.capture("Later black pause inside the same two tape stages")
}

fn clocked_update(
    d: &mut Driver<'_>,
    before: &crate::project::Workspace,
    target: &deadpan_core::NodeId,
    original_cursor: u64,
) -> Result<(), String> {
    let size = d.harness.ctx.content_rect().size();
    resize(d, egui::vec2(960.0, 640.0))?;
    let speed = ExactRatio::new(7, 8).map_err(|error| error.to_string())?;
    let resolved =
        crate::project::retime::resolve(before, target, speed, PitchPolicy::FollowSpeed, false)?;
    let old_clock = before
        .document
        .derived_clocks()
        .get(target)
        .ok_or("Expected exact outer clock")?;
    let input = old_clock
        .child_end
        .checked_sub(old_clock.child_start)
        .map_err(|error| error.to_string())?;
    let expected_frames = input
        .checked_div(speed)
        .and_then(ExactRatio::round_even)
        .map_err(|error| error.to_string())?;
    let revision = d.revision();
    let bank = d.app().copied.bank_version();
    for policy in ["preserve", "+3st"] {
        d.key(Key::Colon)?;
        d.events(
            "Preview an unsupported clocked pitch policy",
            vec![egui::Event::Text(format!("retime 7/8 pitch={policy}"))],
        )?;
        d.check(
            "Clocked pitch hint refuses the same unsupported policy before Apply",
            d.app().retime_hint().is_some_and(|hint| hint.is_err()) && d.revision() == revision,
            json!({"policy":policy,"preview":"refused","revision":revision}),
            d.snapshot(),
        )?;
        d.key(Key::Enter)?;
        d.settled()?;
        d.check(
            "Clocked speed updates refuse unqualified pitch processing without writes",
            d.revision() == revision
                && d.app().source_cursor == original_cursor
                && d.app().copied.bank_version() == bank
                && d.app()
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.document == before.document),
            json!({"policy":policy,"revision":revision,"Original":original_cursor,"bank":bank}),
            d.snapshot(),
        )?;
    }
    scenarios::reveal_inspector_button(d, "Change speed…  Enter")?;
    d.click("Change speed…  Enter")?;
    d.check(
        "Clocked inspector pointer entry uses the exact retained speed",
        d.app().command_open
            && d.app().command == "retime 3/2 pitch=tape"
            && d.revision() == revision,
        json!("retime 3/2 pitch=tape"),
        d.snapshot(),
    )?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Preview an exact clocked speed update",
        vec![egui::Event::Text("retime 7/8 pitch=tape".into())],
    )?;
    let hint = resolved.describe(PitchPolicy::FollowSpeed);
    d.check(
        "Clocked speed hint resolves from the exact child interval before submission",
        d.app().retime_hint() == Some(Ok(hint.clone()))
            && i128::from(resolved.after.frames()) == expected_frames
            && resolved.input == input
            && d.revision() == revision,
        json!({"hint":hint,"input":input,"output":expected_frames}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    let after = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Updated speed workspace missing")?
        .clone();
    let clock = after
        .document
        .derived_clocks()
        .get(target)
        .ok_or("Updated exact clock missing")?;
    let descendants_unchanged =
        before.document.derived_clocks().iter().all(|(id, clock)| {
            id == target || after.document.derived_clocks().get(id) == Some(clock)
        });
    let expected_speed = input
        .checked_div(ExactRatio::integer(resolved.after.frames()))
        .map_err(|error| error.to_string())?;
    let expected_command = format!(
        "retime {}/{} pitch=tape",
        expected_speed.numerator(),
        expected_speed.denominator()
    );
    d.check(
        "Pointer Apply retains the live stage, exact input, descendant clocks and independent Original",
        d.app().selected_beat.as_ref() == Some(target) && d.app().source_cursor == original_cursor
            && d.app().sequence_length() == resolved.after.frames() as u64
            && clock.child_start == old_clock.child_start && clock.child_end == old_clock.child_end
            && descendants_unchanged
            && d.app().inspector_description().and_then(|description| description.parameter).is_some_and(|(_, command)| command == expected_command),
        json!({"output":resolved.after.frames(),"input":input,"prefill":expected_command,"descendants":"unchanged","Original":original_cursor}), d.snapshot(),
    )?;
    d.capture("Exact tape-speed update keeps its pause and retained child interval")?;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.check(
        "Shift-Tab focuses the updated speed inspector before parameter entry",
        d.app().pane == Pane::Inspector,
        json!("Inspector"),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Keyboard Enter opens the same exact post-update speed",
        d.app().command_open && d.app().command == expected_command,
        json!(expected_command),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    let mut actual = serde_json::to_value(&*d.app().workspace.as_ref().unwrap().document)
        .map_err(|error| error.to_string())?;
    actual["revision_id"] =
        serde_json::to_value(before.document.revision_id()).map_err(|error| error.to_string())?;
    d.check(
        "One Undo restores the exact pre-speed clocks, bindings and pause recipe",
        actual == serde_json::to_value(&*before.document).map_err(|error| error.to_string())?
            && d.app().source_cursor == original_cursor,
        json!("exact pre-speed document"),
        d.snapshot(),
    )?;
    resize(d, size)
}

fn resize(d: &mut Driver<'_>, size: egui::Vec2) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing speed replay viewport")?
        .inner_rect = Some(rect);
    d.step("Resize the exact speed inspector", true)?;
    d.settled()
}
