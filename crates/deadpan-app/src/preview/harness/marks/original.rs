use super::*;
use deadpan_core::{SemanticInstruction as I, SourceMoment, SourceStream};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    // A finished backward Edit selection must survive every Original action.
    d.command("sequence")?;
    goto(d, 25)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num5, Key::H])?;
    d.key(Key::V)?;
    let edit_cursor = d.app().sequence_cursor;
    let edit_scope = d.app().sequence_scope.clone();
    let selected = d.app().selected_beat.clone();
    let visual = d.app().capture_visual_selection()?;
    d.command("source")?;
    navigate(d, 30)?;
    d.settled()?;
    let asset = d.app().selected_source.clone().ok_or("Original missing")?;
    let index = d.app().workspace.as_ref().unwrap().sources[&asset]
        .video_index
        .as_ref()
        .ok_or("Original index missing")?
        .clone();
    let retained_edit = |app: &DeadpanApp| {
        app.sequence_cursor == edit_cursor
            && app.sequence_scope == edit_scope
            && app.selected_beat == selected
            && app.capture_visual_selection().as_ref() == Ok(&visual)
    };
    d.chord(&[Key::Q, Key::O])?;
    d.check(
        "Original recording names its clock without changing the retained Edit selection",
        d.app().macros.recording_name() == Some('o')
            && d.app().view == View::Source
            && d.app()
                .message
                .as_ref()
                .is_some_and(|message| message.contains("on Original"))
            && retained_edit(d.app()),
        json!({"macro":"o","clock":"Original","Edit":edit_cursor,"visual":visual}),
        d.snapshot(),
    )?;
    let recording_hint = d.app().message.clone().ok_or("Recording hint missing")?;
    painted(d, &recording_hint)?;
    for hint in [
        "cancel recording",
        "Original bounds",
        "+ letter: save mark",
        "+ letter: jump mark",
    ] {
        painted(d, hint)?;
    }
    let unsupported = [
        "clear selection",
        "start range",
        "cut range",
        "group bounds",
    ]
    .into_iter()
    .flat_map(|text| scenarios::text_paint_visibility(d, text))
    .collect::<Vec<_>>();
    d.check(
        "Original recording teaches its supported keys without inactive Edit actions",
        unsupported.is_empty(),
        json!([]),
        json!(unsupported),
    )?;
    d.capture("Recording marks on Original with an independent Edit selection")?;
    let pane = d.app().pane;
    let revision = d.revision();
    d.chord(&[Key::M, Key::V])?;
    d.changed(&revision)?;
    macro_idle(d)?;
    d.check("A recorded Original mark save preserves pane, both cursors, selected beat and oriented Edit selection",
        d.app().view == View::Source && d.app().pane == pane && d.app().source_cursor == 30 && retained_edit(d.app()),
        json!({"Original":30,"Edit":edit_cursor,"visual":visual}),d.snapshot())?;
    d.chord(&[Key::Num5, Key::L])?;
    jump(d, Key::V)?;
    macro_idle(d)?;
    d.key(Key::Q)?;
    d.wait_for("Original macro o is saved", |app| {
        !app.service.is_busy()
            && !app.macros.recording()
            && app.copied.entries().any(|(slot, value)| {
                slot == 'o' && matches!(value, crate::preview::copied::Content::Macro(_))
            })
    })?;
    let expected = vec![
        I::SetMark { letter: 'v' },
        I::MoveFrames {
            forward: true,
            count: std::num::NonZeroU32::new(5).ok_or("count")?,
        },
        I::JumpMark { letter: 'v' },
    ];
    d.check(
        "Original recording stores semantic mark, motion and jump intent without a saved position",
        macro_program(d, 'o').as_ref() == Some(&expected)
            && d.app().source_cursor == 30
            && retained_edit(d.app()),
        json!({"instructions":expected,"Original":30,"Edit":edit_cursor}),
        json!({"program":macro_program(d,'o'),"state":d.snapshot()}),
    )?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Source, 35)?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    cursor(d, View::Source, 30)?;
    d.check(
        "A recorded Original jump keeps the ordinary back and forward trail",
        retained_edit(d.app()),
        json!({"back":35,"forward":30,"Edit":edit_cursor}),
        d.snapshot(),
    )?;
    navigate(d, 40)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'o', None)?;
    d.changed(&revision)?;
    macro_idle(d)?;
    let mark = mark(d, 'v')?;
    let exact = matches!(&mark.boundary.coordinate,Anchor::Source { asset:marked,moment:SourceMoment::Timestamp { stream:SourceStream::Video,timestamp } }
        if marked==&asset && timestamp.time_base==index.time_base() && timestamp.ticks==index.frames()[40].pts);
    d.check(
        "Native @ re-resolves the Original mark at invocation PTS and returns without moving Edit",
        exact
            && d.app().source_cursor == 40
            && d.app().view == View::Source
            && retained_edit(d.app()),
        json!({"Original":40,"PTS":index.frames()[40].pts,"Edit":edit_cursor,"visual":visual}),
        json!({"mark":mark,"state":d.snapshot()}),
    )?;
    d.capture("Replayed Original mark at the new invocation with Edit retained")?;

    // A navigation-only body makes a stale continuation observable.
    d.chord(&[Key::Q, Key::N, Key::L, Key::Q])?;
    d.wait_for("Original motion macro n is saved", |app| {
        !app.service.is_busy()
            && !app.macros.recording()
            && app.copied.entries().any(|(slot, value)| {
                slot == 'n' && matches!(value, crate::preview::copied::Content::Macro(_))
            })
    })?;
    navigate(d, 40)?;
    let revision = d.revision();
    let update = withheld_navigation(d)?;
    d.app_mut().source_cursor = 80;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().source_cursor = 40;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release Original navigation after away-and-back pointer observations",
        false,
    )?;
    macro_idle(d)?;
    d.check("A delayed Original macro cannot reclaim navigation after its source cursor moved away and back",
        d.revision()==revision && d.app().source_cursor==40 && retained_edit(d.app())
            && d.app().message.as_ref().is_some_and(|message|message.contains("context changed")),
        json!({"Original":40,"Edit":edit_cursor,"history":"unchanged","late_cursor":"not applied"}),d.snapshot())?;
    d.capture("Original macro navigation retained after a delayed completion")?;

    // Two real headless mark commits can replace the one pending UI update.
    let workspace = d.app().workspace.as_ref().unwrap().clone();
    let original = d
        .app()
        .capture_mark()?
        .original_context()
        .ok_or("Original capture missing")?;
    let parent = edit_scope.resolve(&workspace)?.owner.clone();
    let bank_version = d
        .app()
        .copied
        .bank_version()
        .ok_or("Register bank missing")?;
    let headless_pane = d.app().pane;
    let first = RevisionId::new("original-metadata-first").map_err(|error| error.to_string())?;
    let second = RevisionId::new("original-metadata-second").map_err(|error| error.to_string())?;
    let remote = |before: RevisionId,
                  after: RevisionId,
                  letter: char,
                  ordinal: u64|
     -> Result<Value, String> {
        let mut original = original.clone();
        original.ordinal = deadpan_core::SourceFrameId(ordinal);
        let request = deadpan_cli::macros::Request {
            protocol: deadpan_cli::macros::PROTOCOL_VERSION,
            project_id: workspace.document.project_id().clone(),
            expected_revision: before,
            expected_bank_version: bank_version,
            dry_run: false,
            operation: deadpan_cli::macros::Operation::Apply {
                scoped: None,
                program: Arc::new(
                    deadpan_core::SemanticProgram::new(vec![I::SetMark { letter }])
                        .map_err(|error| error.to_string())?,
                ),
                parent: parent.clone(),
                cursor: ProjectFrame(
                    i64::try_from(edit_cursor).map_err(|error| error.to_string())?,
                ),
                selected_child: selected.clone(),
                visual_selection: visual.clone(),
                original: Some(original),
                new_revision: Some(after),
            },
        };
        deadpan_cli::live_project::dispatch_short(
            &workspace.path,
            Some(request.project_id.clone()),
            deadpan_cli::live_project::ShortOperation::Macro {
                request: Box::new(request),
            },
        )
        .map_err(|error| error.to_string())
    };
    d.app_mut().feedback.hold_project_updates = true;
    let result1 = remote(
        workspace.document.revision_id().clone(),
        first.clone(),
        'Q',
        45,
    )?;
    let result2 = remote(first, second.clone(), 'W', 46)?;
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Deliver only the latest of two successful headless Original mark saves",
        false,
    )?;
    d.wait_for("Both headless mark saves are visible", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id() == &second)
    })?;
    d.settled()?;
    d.check("Coalesced headless mark-only saves preserve the native Original and oriented Edit selection",
        result1["committed"]==true && result2["committed"]==true
            && d.app().source_cursor==40 && d.app().view==View::Source && d.app().pane==headless_pane && retained_edit(d.app())
            && has_mark(d,'Q') && has_mark(d,'W'),
        json!({"commits":2,"Original":40,"Edit":edit_cursor,"visual":visual,"cursor_continuation":false}),
        json!({"state":d.snapshot(),"first":result1,"second":result2}))?;
    d.capture("Two headless Original mark saves retain the independent native selection")?;
    let revision = d.revision();
    let previous = macro_program(d, 'x');
    d.chord(&[Key::Q, Key::X])?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels Original recording without clearing its independent Edit range",
        !d.app().macros.recording()
            && d.revision() == revision
            && macro_program(d, 'x') == previous
            && d.app().view == View::Source
            && d.app().source_cursor == 40
            && retained_edit(d.app()),
        json!({"recording":false,"history":"unchanged","Edit":edit_cursor,"visual":visual}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Q, Key::X])?;
    let update = withheld_navigation(d)?;
    d.key(Key::Escape)?;
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release the real navigation receipt after cancelling Original recording",
        false,
    )?;
    macro_idle(d)?;
    d.check(
        "Escape revokes a pending Original call even with no Original selection to clear",
        !d.app().macros.recording()
            && d.revision() == revision
            && macro_program(d, 'x') == previous
            && d.app().view == View::Source
            && d.app().source_cursor == 40
            && retained_edit(d.app()),
        json!({"Original":40,"recording":false,"late_cursor":"not applied","Edit":edit_cursor}),
        d.snapshot(),
    )
}

fn withheld_navigation(d: &mut Driver<'_>) -> Result<crate::project::ProjectUpdate, String> {
    d.app_mut().feedback.hold_project_updates = true;
    super::super::macros::execute(d, 'n', None)?;
    let identity = d
        .app()
        .macros
        .pending_identity()
        .ok_or("Missing pending macro identity")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && update
                .saved_macro
                .as_ref()
                .is_some_and(|receipt| receipt.id == identity)
        {
            return Ok(update);
        }
        if Instant::now() >= deadline {
            return Err("Deferred Original macro receipt did not arrive".into());
        }
        d.step(
            "Withhold delivery of the real Original navigation receipt",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}
