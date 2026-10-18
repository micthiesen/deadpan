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
    d.check("The inspector shows the exact live speed and explains its clocked editing restriction",
        description.fields.contains(&("Speed", "3/2×".into()))
            && description.parameter.is_none()
            && description.note.contains("exact pause timing")
            && speed_paint.iter().any(|paint| paint["fully_visible"] == true),
        json!({"Speed":"3/2×","change_speed":false,"explanation":"exact pause timing"}),
        json!({"fields":description.fields,"parameter":description.parameter,"note":description.note,"paint":speed_paint}))?;
    d.capture("Silent freeze inside two live tape-speed stages")?;
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
    let resolution = crate::project::retime::resolve(workspace, &outer, ExactRatio::ONE, false);
    d.check(
        "An integral outer stage still explains its retained fractional descendants and refuses speed preview",
        !workspace.document.derived_clocks().contains_key(&outer)
            && workspace.clocked_retimes.contains(&outer)
            && description.parameter.is_none()
            && description.note.contains("exact pause timing")
            && resolution.as_ref().is_err_and(|message| message.contains("exact pause timing")),
        json!({"own_clock":false,"retained_descendant_clocks":true,"change_speed":false,"preview":"refused"}),
        json!({"fields":description.fields,"parameter":description.parameter,"note":description.note,"preview":format!("{resolution:?}")}),
    )?;
    d.capture("Later black pause inside the same two tape stages")
}
