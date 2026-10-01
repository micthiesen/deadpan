//! Original source range, explicit Hold application and native modal ownership.

use super::*;
use deadpan_core::{AudioSample, HoldAudio, HoldRecipe, SourceAudio};
use egui::{Key, Modifiers};

const IN: &str = "Room tone In sample";
const OUT: &str = "Room tone Out sample";
const APPLY: &str = "Apply room tone  ·  Enter";
const CANCEL: &str = "Cancel  ·  Esc";
const PLAY: &str = "Play source  ·  Space";
const LOOP: &str = "Loop source  ·  Shift+Space";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Source and committed Sequence audition use injected typed delivery updates, with real descriptor preparation and project writes. This replay does not prepare PCM, open an output device, establish acoustic quality or certify speech absence.".into());
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num7, Key::L])?;
    let before = d.revision();
    d.command("hold 11f")?;
    d.changed(&before)?;
    let target = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No inserted Hold selected")?;
    let original_hold = hold(d, &target)?;
    let silent_revision = d.revision();
    let silent_nodes = document(d)?.nodes().clone();
    d.check(
        "The room-tone workflow starts from a selected eleven-frame silent Hold",
        original_hold.duration.frames() == 11 && original_hold.audio == HoldAudio::Silence,
        json!({"target":target,"frames":11,"audio":"silence"}),
        d.snapshot(),
    )?;
    d.command("room-tone")?;
    d.wait_for("Missing copied source reports a bounded error", |app| {
        !app.service.is_busy() && app.error.is_some()
    })?;
    d.check(
        "Opening without a copied Original range cannot invent a source or edit",
        d.app().room_tone.is_none()
            && d.revision() == silent_revision
            && document(d)?.nodes() == &silent_nodes,
        json!({"revision":silent_revision,"room_tone":null}),
        d.snapshot(),
    )?;

    d.command("source")?;
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
    d.check(
        "Original v, counted motion and y capture a half-open source range without history",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..20))
            && !d.app().moment.active
            && d.revision() == silent_revision,
        json!({"ordinals":[10,20],"revision":silent_revision}),
        d.snapshot(),
    )?;
    let expected = copied_source(d)?;
    d.command("sequence")?;
    d.settled()?;
    let editor = editor_state(d);
    d.check(
        "Returning from Original retains the intended Hold",
        d.app().selected_beat.as_ref() == Some(&target),
        json!(target),
        d.snapshot(),
    )?;
    open(d)?;
    d.check(
        "Room tone prepares exact Original samples and never rounds through project frames",
        prepared(d)?.source == expected && d.revision() == silent_revision,
        json!({"source":expected,"revision":silent_revision}),
        d.snapshot(),
    )?;
    layout(d, "Prepared source selection before application")?;
    audition(d, &editor, &silent_revision)?;

    // Native text owns edit letters, punctuation and composition. Cancelling
    // this malformed draft must not perform its embedded editor shortcuts.
    field(d, IN, "dd + j y")?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels native field text without deleting, moving or applying",
        d.app().room_tone.is_none()
            && d.revision() == silent_revision
            && document(d)?.nodes() == &silent_nodes
            && editor_state(d) == editor,
        json!({"revision":silent_revision,"editor":editor}),
        d.snapshot(),
    )?;
    open(d)?;
    let start = expected.span.start().ticks + 1;
    let end = start + 12_000;
    field(d, IN, &start.to_string())?;
    d.key(Key::Tab)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Native Tab moves from In to Out without changing editor panes",
        vec![egui::Event::Text(end.to_string())],
    )?;
    d.check(
        "Editing samples invalidates the prior descriptor without writing history",
        d.app()
            .room_tone
            .as_ref()
            .is_some_and(|draft| draft.prepared.is_none())
            && d.revision() == silent_revision
            && d.rect(APPLY).is_err(),
        json!("dirty source range, Apply disabled"),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    wait_prepared(d)?;
    d.check(
        "The first Enter on changed sample fields prepares without applying",
        prepared(d)?.source.span.start().ticks == start
            && prepared(d)?.source.span.end().ticks == end
            && prepared(d)?.audition.duration_samples() == AudioSample(12_000)
            && d.revision() == silent_revision,
        json!({"samples":[start,end],"duration_samples":12000,"revision":silent_revision}),
        d.snapshot(),
    )?;
    let chosen = prepared(d)?.source.clone();
    ime(d, IN, &start.to_string(), &silent_revision)?;
    d.click("Prepare range")?;
    wait_prepared(d)?;
    d.check(
        "IME editing retains the exact prepared selection",
        prepared(d)?.source == chosen,
        json!(chosen),
        d.snapshot(),
    )?;
    layout(d, "Exact native sample range ready for explicit Apply")?;
    d.click_at(IN, field_rect(d, IN)?.center())?;
    d.key(Key::Enter)?;
    d.changed(&silent_revision)?;
    let applied = d.revision();
    let changed_hold = hold(d, &target)?;
    let mut expected_hold = original_hold.clone();
    expected_hold.audio = HoldAudio::RoomTone {
        source: chosen.clone(),
    };
    let mut expected_nodes = silent_nodes.clone();
    expected_nodes
        .get_mut(&target)
        .ok_or("Captured Hold disappeared")?
        .kind = NodeKind::Hold {
        recipe: expected_hold.clone(),
    };
    d.check(
        "Explicit Apply changes only captured Hold audio in one transaction",
        d.app().room_tone.is_none()
            && changed_hold == expected_hold
            && document(d)?.nodes() == &expected_nodes
            && editor_state(d) == editor,
        json!({"target":target,"source":chosen,"frames":11,"editor":editor}),
        d.snapshot(),
    )?;
    saved_layout(d, &chosen)?;
    compact_workspace_scale(d)?;
    committed_audition(d, &target, &expected_hold, &editor, &applied)?;
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.check(
        "One undo restores exact silence and picture structure",
        document(d)?.nodes() == &silent_nodes,
        json!(silent_nodes),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&undone)?;
    d.check(
        "Redo restores the authored room-tone range",
        hold(d, &target)? == expected_hold,
        json!(expected_hold),
        d.snapshot(),
    )?;
    open(d)?;
    d.check(
        "Reopening room tone uses its saved range rather than the earlier copied range",
        prepared(d)?.source == chosen,
        json!(chosen),
        d.snapshot(),
    )?;
    let before_cancel = d.revision();
    d.click("Use copied Original range")?;
    wait_prepared(d)?;
    d.check(
        "Replacing a saved draft range with the copied Original selection is explicit and unsaved",
        prepared(d)?.source == expected && d.revision() == before_cancel,
        json!({"source":expected,"revision":before_cancel}),
        d.snapshot(),
    )?;
    d.click(CANCEL)?;
    d.check(
        "Pointer Cancel leaves saved room tone and history intact",
        d.app().room_tone.is_none() && d.revision() == before_cancel,
        json!(before_cancel),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("hold-silence")?;
    d.changed(&before)?;
    d.check(
        "Hold silence restores only this Hold's digital-silence policy",
        hold(d, &target)? == original_hold && document(d)?.nodes() == &silent_nodes,
        json!(original_hold),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Undo of Hold silence restores the exact approved source range",
        hold(d, &target)? == expected_hold,
        json!(expected_hold),
        d.snapshot(),
    )?;
    stale_range(d, &chosen)?;
    stale_target(d, &target)?;
    Ok(())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Room-tone replay lost its workspace".into())
}
fn hold(d: &Driver<'_>, target: &NodeId) -> Result<HoldRecipe, String> {
    match &document(d)?
        .nodes()
        .get(target)
        .ok_or("Captured Hold is absent")?
        .kind
    {
        NodeKind::Hold { recipe } => Ok(recipe.clone()),
        _ => Err("Captured room-tone target is no longer a Hold".into()),
    }
}
fn prepared<'a>(d: &'a Driver<'_>) -> Result<&'a crate::project::PreparedRoomTone, String> {
    d.app()
        .room_tone
        .as_ref()
        .and_then(|draft| draft.prepared.as_ref())
        .ok_or_else(|| "Room-tone source descriptor is not ready".into())
}
fn wait_prepared(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Captured room-tone source range is prepared", |app| {
        !app.service.is_busy()
            && app
                .room_tone
                .as_ref()
                .is_some_and(|draft| draft.prepared.is_some())
    })
}
fn open(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("room-tone")?;
    wait_prepared(d)
}
fn editor_state(d: &Driver<'_>) -> Value {
    let snapshot = d.snapshot();
    json!({"source_cursor":snapshot["source_cursor"],"sequence_cursor":snapshot["sequence_cursor"],
        "selected_beat":snapshot["selected_beat"],"sequence_scope":snapshot["sequence_scope"],
        "duration":snapshot["duration"],"copied_moment":snapshot["copied_moment"]})
}
fn field(d: &mut Driver<'_>, label: &str, text: &str) -> Result<(), String> {
    d.click_at(label, field_rect(d, label)?.center())?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Edit native room-tone sample field",
        vec![egui::Event::Text(text.into())],
    )
}
fn field_rect(d: &Driver<'_>, label: &str) -> Result<egui::Rect, String> {
    let matches = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| {
            let access = node.accesskit_node();
            access.role() == egui::accesskit::Role::TextInput
                && access.label().as_deref() == Some(label)
                && !access.is_disabled()
                && !access.is_hidden()
        })
        .map(|node| node.rect())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [rect] if rect.is_positive() => Ok(*rect),
        _ => Err(format!(
            "Expected one native field {label:?}, found {}",
            matches.len()
        )),
    }
}
fn copied_source(d: &Driver<'_>) -> Result<SourceAudio, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No Original workspace")?;
    let asset = d
        .app()
        .selected_source
        .as_ref()
        .ok_or("No selected Original")?;
    let source = workspace
        .sources
        .get(asset)
        .ok_or("Original is not registered")?;
    let snapshot = source.receipt.snapshot();
    let video = snapshot
        .video()
        .ok_or("Original has no measured picture index")?
        .index()
        .index();
    let rate = snapshot
        .audio()
        .ok_or("Original has no measured audio")?
        .stream()
        .sample_rate;
    // Derive this integral-boundary fixture witness directly from retained PTS,
    // independently of the room-tone service's conversion helper.
    let samples = |ordinal| -> Result<i64, String> {
        let ticks = video
            .interval(SourceFrameId(ordinal))
            .map_err(|error| error.to_string())?
            .0;
        let base = video.time_base();
        let value = i128::from(ticks) * i128::from(base.numerator()) * i128::from(rate);
        let denominator = i128::from(base.denominator());
        if value % denominator != 0 {
            return Err("Room-tone replay requires integral fixture boundaries".into());
        }
        i64::try_from(value / denominator).map_err(|error| error.to_string())
    };
    let clock = deadpan_core::SourceTimeBase::new(1, rate).map_err(|error| error.to_string())?;
    let span = deadpan_core::SourceSpan::new(
        deadpan_core::SourceTimestamp {
            ticks: samples(10)?,
            time_base: clock,
        },
        deadpan_core::SourceTimestamp {
            ticks: samples(20)?,
            time_base: clock,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(SourceAudio {
        asset: asset.clone(),
        span,
    })
}

fn layout(d: &mut Driver<'_>, caption: &str) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing root viewport")?
            .inner_rect = Some(rect);
        d.step("Paint room-tone sheet on its first resized frame", true)?;
        for label in [
            PLAY,
            LOOP,
            "Prepare range",
            APPLY,
            CANCEL,
            "Source samples · 48000 Hz · Out excluded",
            "Listen for quiet words. Choose non-speech material; nothing is detected or normalized automatically.",
        ] {
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Room-tone controls and their keys paint within the short viewport",
                !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
                json!({"label":label,"viewport":[width,height]}),
                json!(paint),
            )?;
        }
        for label in [IN, OUT] {
            let field = field_rect(d, label)?;
            d.check(
                "Both native sample fields remain reachable in the sheet",
                rect.contains_rect(field),
                json!({"field":label,"viewport":[width,height]}),
                json!(format!("{field:?}")),
            )?;
        }
        d.capture(&format!("{caption} at {width}x{height}"))?;
    }
    Ok(())
}

fn audition(d: &mut Driver<'_>, editor: &Value, revision: &str) -> Result<(), String> {
    let chosen = prepared(d)?.clone();
    let picture = d.app().presentation.diagnostic_snapshot();
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    for looping in [false, true] {
        // Leave the numeric fields before using source-audition shortcuts.
        d.click(if looping { LOOP } else { PLAY })?;
        d.check("Source audition uses the selected range's local clock with no implicit context",
            d.app().transport.as_ref().is_some_and(|run| {
                matches!(run.domain(), crate::transport::Domain::AudioRange(range) if range.span() == chosen.source.span)
                    && run.phase == deadpan_playback::Phase::Preparing
                    && run.sample == AudioSample(0) && run.window().start() == AudioSample(0)
                    && run.window().end() == chosen.audition.duration_samples()
                    && run.window().looping() == looping
            }), json!({"samples":[0,chosen.audition.duration_samples().0],"looping":looping}), d.snapshot())?;
        let generation = feed.restart(0).map_err(|error| error.to_string())?;
        let heard = if looping {
            chosen.audition.duration_samples().0 + 5
        } else {
            5
        };
        let run = d
            .app()
            .transport
            .as_ref()
            .ok_or("No prepared source audition")?;
        let update = deadpan_playback::Update {
            ticket: run.ticket,
            session: run.session,
            project_id: run.project.clone(),
            revision_id: run.revision.clone(),
            content: run.content.clone(),
            phase: deadpan_playback::Phase::Playing,
            sample: Some(AudioSample(heard)),
            generation: Some(generation),
            error: None,
        };
        d.app_mut()
            .feedback
            .playback_updates
            .push_back(update.clone());
        d.step("Deliver selected-source audition on its local clock", true)?;
        d.check(
            "Source delivery leaves the main picture, Original/Edit cursors and history unchanged",
            editor_state(d) == *editor
                && d.revision() == revision
                && d.app().presentation.diagnostic_snapshot() == picture
                && d.app()
                    .transport
                    .as_ref()
                    .is_some_and(|run| run.content_sample() == Ok(AudioSample(5))),
            json!({"editor":editor,"revision":revision,"content_sample":5}),
            d.snapshot(),
        )?;
        d.key(Key::Space)?;
        d.app_mut().feedback.playback_updates.push_back(update);
        d.step("Late source delivery after modal pause", true)?;
        d.check(
            "Space pauses source audition and late delivery cannot resume it",
            d.app().transport.is_none() && editor_state(d) == *editor && d.revision() == revision,
            json!({"stopped":true,"revision":revision}),
            d.snapshot(),
        )?;
    }
    // The same logical shortcut must request looping, not trigger an editor
    // audition or alter the copied Original selection.
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    d.check(
        "Modal Shift Space starts only the selected source loop",
        d.app().transport.as_ref().is_some_and(|run| {
            matches!(run.domain(), crate::transport::Domain::AudioRange(_))
                && run.window().looping()
        }),
        json!("looping selected source"),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    Ok(())
}

fn saved_layout(d: &mut Driver<'_>, source: &SourceAudio) -> Result<(), String> {
    let samples = format!(
        "Samples [{}..{}) · {} Hz",
        source.span.start().ticks,
        source.span.end().ticks,
        source.span.start().time_base.denominator()
    );
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing root viewport")?
            .inner_rect = Some(rect);
        d.step("Paint saved room-tone inspector after resize", true)?;
        let labels = [
            samples.as_str(),
            "Crossfades stay inside this pause.",
            "Room tone…  ·  :room-tone",
            "Use silence  ·  :hold-silence",
        ];
        // Reveal the whole fact/action group, not just its first line.
        // Retained accessibility nodes alone prove no paint.
        for attempt in 0..6 {
            if labels.iter().all(|label| {
                let paint = scenarios::text_paint_visibility(d, label);
                !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true)
            }) {
                break;
            }
            let point = d.rect("Selected beat inspector pane")?.center() + egui::vec2(0.0, 100.0);
            d.events(
                &format!("Reveal saved source samples in Hold inspector, attempt {attempt}"),
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -120.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    },
                ],
            )?;
            for _ in 0..8 {
                d.step("Saved Hold inspector scroll settles", false)?;
            }
        }
        for label in labels {
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Saved Hold inspector paints exact source samples and room-tone actions",
                !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
                json!({"label":label,"viewport":[width,height]}),
                json!(paint),
            )?;
        }
        d.capture(&format!(
            "Saved room tone in Hold inspector at {width}x{height}"
        ))?;
        if width == 960.0 {
            compact_workspace_picture(d)?;
            compact_room_tone_background(d)?;
        }
        gain_inspector_reachable(d, width, height)?;
    }
    Ok(())
}

fn compact_workspace_picture(d: &mut Driver<'_>) -> Result<(), String> {
    scenarios::viewer_visible(d)?;
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Missing copied-range picture")?;
    let viewer = d.rect(&label)?;
    let mut labels = vec![
        "Copied Original [10..20)",
        "Paste after  p",
        "Paste before  P",
        "Monitor · :monitor",
        "PLACED SOUNDS 0 · ,s place",
        "Loop selection  ·  Shift+Space",
        "Loop context",
    ];
    let action = if let Some(run) = &d.app().transport {
        if run.phase == deadpan_playback::Phase::Preparing {
            labels.extend(["Cancel preparation  ·  Space", "Preparing ·"]);
            "Cancel preparation  ·  Space"
        } else {
            labels.extend(["Pause  ·  Space", "Playing ·"]);
            "Pause  ·  Space"
        }
    } else {
        labels.push("Play edit  ·  Space");
        "Play edit  ·  Space"
    };
    let hits = super::sound_placement::control_hits(
        d,
        &[
            "Paste after  p",
            "Paste before  P",
            "Loop selection  ·  Shift+Space",
            "Monitor · :monitor",
            action,
        ],
    );
    let paint = labels
        .iter()
        .map(|label| (*label, scenarios::text_paint_visibility(d, label)))
        .collect::<Vec<_>>();
    d.check(
        "Copied Original controls and empty Sounds retain at least 140 points of picture",
        viewer.height() >= 140.0
            && hits.iter().all(|hit| {
                hit["complete_hit"] == true
                    && hit["enabled"]
                        == (hit["label"] != "Monitor · :monitor" || d.app().transport.is_none())
            })
            && paint.iter().all(|(_, items)| {
                !items.is_empty() && items.iter().all(|item| item["fully_visible"] == true)
            }),
        json!({"minimum_picture_height":140,"fully_painted":labels}),
        json!({"viewer_height":viewer.height(),"paint":paint,"hits":hits}),
    )?;
    scenarios::footer_anchored(
        d,
        "Copied-range compact workspace keeps its footer anchored",
    )
}

fn compact_room_tone_background(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let editor = editor_state(d);
    let bounds = |d: &Driver<'_>| {
        scenarios::text_paint_visibility(d, "PLACED SOUNDS 0 · ,s place")
            .into_iter()
            .map(|text| text["bounds"].clone())
            .collect::<Vec<_>>()
    };
    let target = |d: &Driver<'_>| {
        d.app()
            .target
            .as_ref()
            .map(|target| (target.target.width(), target.target.height()))
    };
    let entry_bounds = bounds(d);
    let entry_target = target(d);
    let picture = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    open(d)?;
    d.settled()?;
    for open in [true, false] {
        if !open {
            d.key(Key::Escape)?;
            d.settled()?;
        }
        d.check(
            "Opening and closing room tone preserves the compact background allocation",
            !entry_bounds.is_empty() && bounds(d) == entry_bounds
                && entry_target.is_some() && target(d) == entry_target
                && d.app().presentation.diagnostic_snapshot()["displayed"] == picture
                && d.app().room_tone.is_some() == open
                && d.revision() == revision && editor_state(d) == editor,
            json!({"sheet_open":open,"heading_bounds":entry_bounds,"target":entry_target,"picture":picture}),
            json!({"sheet_open":d.app().room_tone.is_some(),"heading_bounds":bounds(d),"target":target(d),"state":d.snapshot()}),
        )?;
    }
    Ok(())
}

fn compact_workspace_scale(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let editor = editor_state(d);
    d.command("sounds")?;
    d.settled()?;
    let picture = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    let identities = compact_control_ids(d)?;
    for (width, height, scale) in [
        (960.0, 640.0, 1.0),
        (960.0, 640.0, 2.0),
        (960.0, 640.0, 1.0),
        (1280.0, 820.0, 1.0),
    ] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        let viewport = input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing root viewport during compact workspace scale change")?;
        viewport.inner_rect = Some(rect);
        viewport.native_pixels_per_point = Some(scale);
        for frame in 0..3 {
            d.step(
                &format!("Compact workspace {width}x{height} at {scale}x, frame {frame}"),
                true,
            )?;
            d.check(
                "Scale changes preserve pane/control IDs, focused Sounds and retained picture identity",
                d.harness.ctx.content_rect().size() == egui::vec2(width, height)
                    && (d.harness.ctx.pixels_per_point() - scale).abs() < f32::EPSILON
                    && compact_control_ids(d)? == identities
                    && d.app().pane == Pane::Sounds
                    && d.harness.ctx.memory(|memory| memory.has_focus(pane_id(Pane::Sounds)))
                    && d.app().presentation.diagnostic_snapshot()["displayed"] == picture
                    && d.revision() == revision && editor_state(d) == editor,
                json!({"size":[width,height],"scale":scale,"identities":identities,"picture":picture}),
                json!({"identities":compact_control_ids(d)?,"picture":d.app().presentation.diagnostic_snapshot(),"state":d.snapshot()}),
            )?;
            scenarios::footer_anchored(d, "Scale change keeps the copied-range footer anchored")?;
        }
        d.settled()?;
        if width == 960.0 {
            compact_workspace_picture(d)?;
            super::sound_placement::empty_heading(d)?;
        } else {
            scenarios::viewer_visible(d)?;
            d.check(
                "Default size restores the empty Sounds panel below Beats with the same identity",
                d.rect("Placed sounds pane")?.top()
                    > d.rect("Current group beat outline pane")?.bottom(),
                json!("Separate full-height empty Sounds panel"),
                d.widgets(),
            )?;
        }
    }
    d.command("sequence")?;
    d.settled()
}

fn compact_control_ids(d: &Driver<'_>) -> Result<Vec<String>, String> {
    [
        "Placed sounds pane",
        "Current group beat outline pane",
        "Paste after  p",
        "Paste before  P",
    ]
    .into_iter()
    .map(|label| {
        let ids = d
            .harness
            .root()
            .children_recursive()
            .filter_map(|node| {
                let access = node.accesskit_node();
                if access.role() == egui::accesskit::Role::Button
                    && access.label().as_deref() == Some(label)
                    && !access.is_hidden()
                {
                    Some(format!("{:?}", access.id()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        match ids.as_slice() {
            [id] => Ok(id.clone()),
            _ => Err(format!("Expected one {label:?} identity, found {ids:?}")),
        }
    })
    .collect()
}

fn resize_workspace(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing root viewport")?
        .inner_rect = Some(rect);
    d.step("Resize copied-range workspace for transport layout", true)?;
    d.settled()
}

fn gain_inspector_reachable(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let revision = d.revision();
    let nodes = document(d)?.nodes().clone();
    let editor = editor_state(d);
    for label in ["−  ·  -", "+  ·  +", "Mute", "Edit envelope… · :gain"] {
        // Use the accessibility input route, not direct egui memory or scroll
        // mutation. Normal workspace Tab intentionally cycles visible panes.
        {
            let root = d.harness.root();
            let controls = root
                .children_recursive()
                .filter(|node| {
                    let access = node.accesskit_node();
                    access.role() == egui::accesskit::Role::Button
                        && access.label().as_deref() == Some(label)
                        && !access.is_disabled()
                        && !access.is_hidden()
                })
                .collect::<Vec<_>>();
            let [control] = controls.as_slice() else {
                return Err(format!(
                    "Expected one accessible gain inspector button {label:?}, found {}",
                    controls.len()
                ));
            };
            control.focus();
        }
        for _ in 0..3 {
            d.step(
                "Accessibility focus reveals the saved Hold's gain control",
                false,
            )?;
        }
        let rect = d.rect(label)?;
        let focused = d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.label().as_deref() == Some(label) && access.is_focused()
        });
        let paint = scenarios::text_paint_visibility(d, label);
        let complete_hit = d.harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text() == label)
                && clipped.clip_rect.contains_rect(rect)
        });
        d.check(
            "Accessibility focus reveals the complete gain inspector control without an edit",
            focused
                && d.harness.ctx.content_rect().contains_rect(rect)
                && complete_hit
                && !paint.is_empty()
                && paint.iter().all(|item| item["fully_visible"] == true)
                && d.revision() == revision
                && document(d)?.nodes() == &nodes
                && editor_state(d) == editor,
            json!({"label":label,"viewport":[width,height],"fully_painted":true,"revision":revision}),
            json!({"focused":focused,"complete_hit":complete_hit,"rect":format!("{rect:?}"),"paint":paint,"state":d.snapshot()}),
        )?;
    }

    d.command("gain")?;
    d.wait_for("Captured Hold opens its full gain keyboard editor", |app| {
        !app.service.is_busy()
            && app
                .gain
                .as_ref()
                .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })?;
    d.key(Key::Tab)?;
    let trim_focused = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.role() == egui::accesskit::Role::TextInput
            && access.label().as_deref() == Some("Whole beat trim · dB")
            && access.is_focused()
    });
    d.check(
        "The captured Hold's gain command opens native keyboard fields without changing history",
        trim_focused
            && d.app().gain.is_some()
            && d.revision() == revision
            && document(d)?.nodes() == &nodes
            && editor_state(d) == editor,
        json!({"focused":"Whole beat trim · dB","revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Cancelling the unchanged Hold gain editor retains room tone and entry targeting",
        d.app().gain.is_none()
            && d.revision() == revision
            && document(d)?.nodes() == &nodes
            && editor_state(d) == editor,
        json!({"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    // Restore the saved-inspector view for the next resize using real wheel
    // input only, after the gain focus assertions have already completed.
    let point = d.rect("Selected beat inspector pane")?.center() + egui::vec2(0.0, 100.0);
    d.events(
        "Return saved Hold inspector to its primary controls",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 1000.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Saved Hold inspector restoration settles", false)?;
    }
    Ok(())
}

fn committed_audition(
    d: &mut Driver<'_>,
    target: &NodeId,
    expected: &HoldRecipe,
    editor: &Value,
    revision: &str,
) -> Result<(), String> {
    resize_workspace(d, 960.0, 640.0)?;
    let rate = document(d)?.presentation_basis().frame_rate;
    let committed_revision = document(d)?.revision_id().clone();
    let frame = i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?;
    let start = rate
        .audio_boundary(deadpan_core::ProjectFrame(frame))
        .map_err(|error| error.to_string())?;
    let frames = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing saved room-tone workspace")?
        .plan
        .duration()
        .frames();
    let end = rate
        .audio_boundary(deadpan_core::ProjectFrame(frames))
        .map_err(|error| error.to_string())?;
    d.key(Key::Space)?;
    d.check(
        "Space after Apply auditions the committed edit at the retained Hold cursor",
        d.app().view == View::Sequence
            && d.app().room_tone.is_none()
            && d.app().transport.as_ref().is_some_and(|run| {
                matches!(run.domain(), crate::transport::Domain::Sequence { rate: actual_rate, frames: actual_frames }
                    if *actual_rate == rate && *actual_frames == frames)
                    && run.phase == deadpan_playback::Phase::Preparing
                    && run.revision == committed_revision
                    && run.sample == start
                    && run.window().start() == AudioSample(0)
                    && run.window().end() == end
                    && !run.window().looping()
            })
            && hold(d, target)? == *expected
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"domain":"sequence","start":start,"end":end,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No committed edit audition")?;
    let update = deadpan_playback::Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase: deadpan_playback::Phase::Playing,
        // Five samples advance the exact heard clock inside this same frame.
        sample: Some(AudioSample(
            start.0.checked_add(5).ok_or("Sample overflow")?,
        )),
        generation: Some(generation),
        error: None,
    };
    d.app_mut()
        .feedback
        .playback_updates
        .push_back(update.clone());
    d.step(
        "Simulate typed delivery from the committed Sequence revision",
        true,
    )?;
    d.check(
        "Committed edit delivery keeps approved room tone and both editor clocks in context",
        d.app().transport.as_ref().is_some_and(|run| {
            run.phase == deadpan_playback::Phase::Playing && Some(run.sample) == update.sample
        }) && hold(d, target)? == *expected
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"source":expected.audio,"editor":editor,"revision":revision,"sample":update.sample}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    d.key(Key::Space)?;
    d.app_mut()
        .feedback
        .playback_updates
        .push_back(update.clone());
    d.step(
        "Ignore late committed Sequence delivery after Space pauses",
        true,
    )?;
    d.check(
        "Space pauses committed edit audition without changing the saved Hold or accepting late delivery",
        d.app().transport.is_none()
            && d.app().resume.as_ref().is_some_and(|resume| {
                matches!(resume.domain(), crate::transport::Domain::Sequence { .. })
            })
            && hold(d, target)? == *expected
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"paused":true,"editor":editor,"revision":revision}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    d.key(Key::Space)?;
    d.check(
        "Copied-range Space resumes the exact paused sample with all controls visible",
        d.app().transport.as_ref().is_some_and(|run| {
            run.phase == deadpan_playback::Phase::Preparing
                && Some(run.sample) == update.sample
                && run.revision == committed_revision
        }) && hold(d, target)? == *expected
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"resumed_sample":update.sample,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    let mut resumed = update;
    let resumed_sample = resumed
        .sample
        .ok_or("Resumed delivery lost its exact sample")?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Missing resumed copied-range transport")?;
    resumed.ticket = run.ticket;
    resumed.generation = Some(
        feed.restart(run.sample.0)
            .map_err(|error| error.to_string())?,
    );
    d.app_mut().feedback.playback_updates.push_back(resumed);
    d.step(
        "Resume copied-range playback with a new typed delivery generation",
        true,
    )?;
    d.check(
        "Resumed copied-range delivery enters Playing at the exact heard sample",
        d.app().transport.as_ref().is_some_and(|run| {
            run.phase == deadpan_playback::Phase::Playing
                && run.sample == resumed_sample
                && run.revision == committed_revision
        }) && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"sample":resumed_sample,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    d.key(Key::Space)?;
    d.check(
        "Resumed copied-range audition pauses without an authored edit or cursor change",
        d.app().transport.is_none()
            && d.app().resume.is_some()
            && hold(d, target)? == *expected
            && d.revision() == revision
            && editor_state(d) == *editor,
        json!({"paused":true,"revision":revision,"editor":editor}),
        d.snapshot(),
    )?;
    compact_workspace_picture(d)?;
    resize_workspace(d, 1280.0, 820.0)
}

fn ime(d: &mut Driver<'_>, field_label: &str, text: &str, revision: &str) -> Result<(), String> {
    d.click_at(field_label, field_rect(d, field_label)?.center())?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Room-tone native composition retains Enter and edit keys",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: text.into(),
                active_range_chars: Some(0..text.chars().count()),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME Enter neither applies nor closes the room-tone sheet",
        d.app().room_tone.is_some() && d.revision() == revision && d.app().transport.is_none(),
        json!({"open":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.events(
        "Finish native sample composition without Apply",
        vec![egui::Event::Ime(egui::ImeEvent::Commit(text.into()))],
    )?;
    d.check(
        "A committed numeric field still requires explicit preparation and Apply",
        d.app().room_tone.is_some() && d.revision() == revision,
        json!(revision),
        d.snapshot(),
    )
}

fn stale_range(d: &mut Driver<'_>, chosen: &SourceAudio) -> Result<(), String> {
    open(d)?;
    let revision = d.revision();
    let start = chosen.span.start().ticks;
    d.app_mut().feedback.hold_project_updates = true;
    field(d, IN, &(start + 1).to_string())?;
    d.click("Prepare range")?;
    d.wait_for(
        "First range preparation completes while delivery is held",
        |app| !app.service.is_busy(),
    )?;
    field(d, IN, &(start + 2).to_string())?;
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Release superseded source-range preparation", true)?;
    d.check(
        "A changed field cannot adopt a prepared descriptor for the previous range",
        d.app()
            .room_tone
            .as_ref()
            .is_some_and(|draft| draft.prepared.is_none())
            && d.revision() == revision,
        json!({"prepared":null,"revision":revision}),
        d.snapshot(),
    )?;
    d.click("Prepare range")?;
    wait_prepared(d)?;
    d.check(
        "Fresh preparation recovers with the exact latest sample boundary",
        prepared(d)?.source.span.start().ticks == start + 2 && d.revision() == revision,
        json!(start + 2),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Cancelling the replacement range preserves the previously saved room tone",
        d.app().room_tone.is_none() && d.revision() == revision,
        json!(revision),
        d.snapshot(),
    )?;
    open(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    field(d, IN, &(start + 3).to_string())?;
    d.click("Prepare range")?;
    d.wait_for(
        "Cancelled draft's range preparation finishes off the UI",
        |app| !app.service.is_busy(),
    )?;
    d.key(Key::Escape)?;
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Deliver prepared range after its sheet was cancelled", true)?;
    d.check(
        "A late source descriptor cannot reopen or apply a cancelled room-tone sheet",
        d.app().room_tone.is_none() && d.revision() == revision && d.app().transport.is_none(),
        json!({"room_tone":null,"revision":revision}),
        d.snapshot(),
    )
}

fn stale_target(d: &mut Driver<'_>, target: &NodeId) -> Result<(), String> {
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold-duration 12f")?;
    d.wait_for("Hold edit completes before its UI delivery", |app| {
        !app.service.is_busy()
    })?;
    d.command("room-tone")?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    d.wait_for("Stale Hold draft closes without applying", |app| {
        !app.service.is_busy() && app.room_tone.is_none()
    })?;
    let current = d.revision();
    d.check(
        "A room-tone sheet cannot apply after its captured Hold revision changes",
        hold(d, target)?.duration.frames() == 12 && d.app().room_tone.is_none(),
        json!({"frames":12,"revision":current,"room_tone":null}),
        d.snapshot(),
    )?;
    let revision = current;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold-duration 11f")?;
    d.wait_for("Second Hold edit completes before command entry", |app| {
        !app.service.is_busy()
    })?;
    d.command("source")?;
    d.key(Key::Colon)?;
    d.events(
        "Type room-tone with no eligible Edit Hold context captured",
        vec![egui::Event::Text("room-tone".into())],
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    let current = d.revision();
    d.key(Key::Enter)?;
    d.wait_for(
        "Absent room-tone command target rejects after late completion",
        |app| !app.service.is_busy(),
    )?;
    d.check(
        "A command opened in Original cannot adopt a late selected Hold",
        d.app().room_tone.is_none()
            && d.revision() == current
            && hold(d, target)?.duration.frames() == 11,
        json!({"revision":current,"room_tone":null}),
        d.snapshot(),
    )?;
    Ok(())
}
