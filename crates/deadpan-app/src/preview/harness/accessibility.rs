//! Native accessibility through the production router and AccessKit tree:
//! a valid focus on every start-screen Tab, spoken pane summaries, live
//! status notices, Help and Models announcements and focus, and the Reduce
//! motion / Increase contrast style with a visible focus indicator.
//!
//! This inspects the tree macOS receives. VoiceOver speech itself, physical
//! key delivery and system settings are outside an offscreen replay.

use egui::Key;

use super::super::accessibility::{self, Preferences};
use super::*;

/// One AccessKit node, as the platform adapter receives it.
fn node(d: &Driver<'_>, matches: impl Fn(&Value) -> bool) -> Option<Value> {
    d.harness
        .root()
        .children_recursive()
        .take(4096)
        .map(|node| {
            let access = node.accesskit_node();
            let rect = access
                .bounding_box()
                .map(|_| node.rect())
                .map(|rect| [rect.min.x, rect.min.y, rect.max.x, rect.max.y]);
            json!({
                "label": access.label(),
                "value": access.value(),
                "role": format!("{:?}", access.role()),
                "live": format!("{:?}", access.live()),
                "focused": access.is_focused(),
                "rect": rect,
            })
        })
        .find(|value| matches(value))
}

fn labelled(d: &Driver<'_>, label: &str) -> Option<Value> {
    node(d, |value| value["label"].as_str() == Some(label))
}

fn focused(d: &Driver<'_>) -> Option<Value> {
    node(d, |value| value["focused"] == true)
}

fn live(value: &Value) -> &str {
    value["live"].as_str().unwrap_or_default()
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

const PANES: [&str; 5] = [
    "Original and sounds pane",
    "Picture viewer: no project open",
    "Current group beat outline pane",
    "Selected beat inspector pane",
    "Placed sounds pane",
];

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    start_screen(d)?;
    d.click("Choose video…  ⌘N")?;
    d.wait_for("Original initialized and displayed", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|w| matches!(w.single_source, Some(SingleSourceState::Ready { .. })))
            && app.presentation.has_displayed()
            && !app.presentation.loading()
    })?;
    d.command("sequence")?;
    d.settled()?;
    panes(d)?;
    notices(d)?;
    sheets(d)?;
    display(d)
}

/// The start screen draws no Inspector or Placed sounds. Every Tab must leave
/// AccessKit focus on a control that exists; the macOS adapter aborts the
/// app otherwise.
fn start_screen(d: &mut Driver<'_>) -> Result<(), String> {
    let viewer = labelled(d, "Picture viewer: no project open");
    d.check(
        "The empty viewer names the start keys",
        viewer
            .as_ref()
            .is_some_and(|viewer| text(viewer, "value").contains("Command-N chooses a video")),
        json!("Picture viewer: no project open · Command-N chooses a video"),
        json!(viewer),
    )?;
    let mut order = Vec::new();
    for _ in 0..4 {
        d.key(Key::Tab)?;
        let focus = focused(d);
        order.push(json!({"pane":format!("{:?}", d.app().pane),"focus":focus}));
        let label = focus.as_ref().map(|focus| text(focus, "label"));
        d.check(
            "Start-screen Tab focuses a drawn pane",
            label
                .as_deref()
                .is_some_and(|label| PANES[..3].contains(&label)),
            json!(&PANES[..3]),
            json!(order),
        )?;
    }
    // A focus left on a pane this layout does not draw (Placed sounds on the
    // start screen) moves to a drawn pane instead of an unnamed node.
    d.app_mut().pane = Pane::Sounds;
    d.harness
        .ctx
        .memory_mut(|memory| memory.request_focus(super::super::pane_id(Pane::Sounds)));
    d.step("Stale focus on an undrawn pane", false)?;
    let focus = focused(d);
    d.check(
        "Focus on an undrawn pane moves to a drawn pane",
        focus
            .as_ref()
            .is_some_and(|focus| PANES[..3].contains(&text(focus, "label").as_str()))
            && d.app().pane != Pane::Sounds,
        json!({"focus":"a drawn pane","pane":"not Sounds"}),
        json!({"focus":focus,"pane":format!("{:?}", d.app().pane)}),
    )?;
    d.capture("Start screen after Tab traversal")
}

fn panes(d: &mut Driver<'_>) -> Result<(), String> {
    for _ in 0..5 {
        if d.app().pane == Pane::Viewer {
            break;
        }
        d.key(Key::Tab)?;
    }
    let viewer = node(d, |value| {
        value["role"] == "Image" && text(value, "label").starts_with("Showing sequence frame")
    });
    d.check(
        "The focused viewer speaks Your edit position and beat",
        viewer.as_ref().is_some_and(|viewer| {
            text(viewer, "value").starts_with("Your edit, frame 1 of ")
                && text(viewer, "value").contains("Beat 1 of 1")
                && live(viewer) == "Polite"
        }) && d.app().pane == Pane::Viewer,
        json!({"value":"Your edit, frame 1 of … Beat 1 of 1 …","live":"Polite"}),
        json!({"viewer":viewer,"pane":format!("{:?}", d.app().pane)}),
    )?;
    d.key(Key::L)?;
    d.settled()?;
    let stepped = node(d, |value| {
        text(value, "label") == "Showing sequence frame 2"
    });
    d.check(
        "Stepping a frame changes the spoken viewer position",
        stepped
            .as_ref()
            .is_some_and(|viewer| text(viewer, "value").starts_with("Your edit, frame 2 of ")),
        json!("Your edit, frame 2 of …"),
        json!(stepped),
    )?;
    held_motion(d)?;
    let lavender = viewer_focus_ring(d);
    d.check(
        "The focused viewer paints its lavender focus ring and FOCUS cue",
        lavender && focus_cue_visible(d),
        json!({"ring":true,"focus_cue":true}),
        json!({"ring":lavender,"focus_cue":scenarios::text_paint_visibility(d, "Focus: Viewer")}),
    )?;
    for _ in 0..5 {
        if d.app().pane == Pane::Sequence {
            break;
        }
        d.key(Key::Tab)?;
    }
    let beats = labelled(d, "Current group beat outline pane");
    let viewer = node(d, |value| {
        value["role"] == "Image" && text(value, "label").starts_with("Showing")
    });
    d.check(
        "Only the focused Beats pane speaks the selected beat",
        beats.as_ref().is_some_and(|beats| {
            text(beats, "value").starts_with("Beat 1 of 1: ")
                && text(beats, "value").contains("120 frames")
                && live(beats) == "Polite"
                && beats["focused"] == true
        }) && viewer.as_ref().is_some_and(|viewer| live(viewer) == "Off"),
        json!({"beats":"Beat 1 of 1: … 120 frames …","live":"Polite","viewer_live":"Off"}),
        json!({"beats":beats,"viewer":viewer}),
    )?;
    d.capture("Beats pane focused")
}

/// A held motion key changes the viewer silently; its release announces the
/// final position once.
fn held_motion(d: &mut Driver<'_>) -> Result<(), String> {
    let key = |pressed, repeat| egui::Event::Key {
        key: Key::L,
        physical_key: None,
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    };
    d.events("Hold l", vec![key(true, false)])?;
    for _ in 0..3 {
        d.events("Hold l (repeat)", vec![key(true, true)])?;
    }
    let viewer = |d: &Driver<'_>| {
        node(d, |value| {
            value["role"] == "Image" && text(value, "label").starts_with("Showing sequence frame")
        })
    };
    let held = viewer(d);
    d.check(
        "A held motion key updates the viewer value without speaking",
        held.as_ref().is_some_and(|viewer| {
            live(viewer) == "Off" && !text(viewer, "value").starts_with("Your edit, frame 2 of ")
        }),
        json!({"live":"Off"}),
        json!(held),
    )?;
    d.events("Release l", vec![key(false, false)])?;
    d.settled()?;
    let released = viewer(d);
    d.check(
        "Releasing the key makes the final position speak once",
        released
            .as_ref()
            .is_some_and(|viewer| live(viewer) == "Polite"),
        json!({"live":"Polite"}),
        json!(released),
    )
}

fn viewer_focus_ring(d: &Driver<'_>) -> bool {
    let Some(viewer) = node(d, |value| {
        value["role"] == "Image" && value["live"] == "Polite"
    }) else {
        return false;
    };
    let rect = egui::Rect::from_min_max(
        egui::pos2(
            viewer["rect"][0].as_f64().unwrap_or_default() as f32,
            viewer["rect"][1].as_f64().unwrap_or_default() as f32,
        ),
        egui::pos2(
            viewer["rect"][2].as_f64().unwrap_or_default() as f32,
            viewer["rect"][3].as_f64().unwrap_or_default() as f32,
        ),
    );
    d.harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Rect(painted)
            if painted.stroke.color == style::LAVENDER
                && painted.stroke.width >= 1.0
                && (painted.rect.min - rect.min).length() < 2.0
                && (painted.rect.max - rect.max).length() < 2.0)
    })
}

fn focus_cue_visible(d: &Driver<'_>) -> bool {
    scenarios::text_paint_visibility(d, "Focus: ")
        .iter()
        .any(|text| text["fully_visible"] == true)
}

fn notices(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d
        .app()
        .workspace
        .as_ref()
        .map(|w| w.document.revision_id().clone());
    d.command("hold 15f")?;
    d.wait_for("Pause saved", |app| {
        app.workspace
            .as_ref()
            .map(|w| w.document.revision_id().clone())
            != revision
            && !app.service.is_busy()
    })?;
    d.settled()?;
    let saved = node(d, |value| {
        text(value, "value").contains("pause") && value["role"] == "Label" && value["live"] != "Off"
    });
    d.check(
        "A saved edit's notice is a polite live region",
        saved.as_ref().is_some_and(|saved| live(saved) == "Polite"),
        json!({"live":"Polite","value":"…pause…"}),
        json!({"notice":saved,"message":d.app().message}),
    )?;
    d.command("notacommand")?;
    d.settled()?;
    let refused = node(d, |value| {
        text(value, "value").starts_with("Could not complete action")
    });
    d.check(
        "A refusal interrupts as an assertive live region",
        refused
            .as_ref()
            .is_some_and(|refused| live(refused) == "Assertive"),
        json!({"live":"Assertive","value":"Could not complete action: …"}),
        json!(refused),
    )?;
    let saved = d
        .app()
        .workspace
        .as_ref()
        .map(|w| w.document.revision_id().clone());
    d.key(Key::U)?;
    d.wait_for("Pause undone", |app| {
        app.workspace
            .as_ref()
            .map(|w| w.document.revision_id().clone())
            != saved
            && !app.service.is_busy()
    })?;
    d.settled()
}

fn sheets(d: &mut Driver<'_>) -> Result<(), String> {
    d.click("Keys  ?")?;
    d.settled()?;
    let hint = node(d, |value| {
        text(value, "value").starts_with("j/k · Up/Down scroll")
    });
    d.check(
        "Opening Help announces how to scroll and close it",
        d.app().help_open && hint.as_ref().is_some_and(|hint| live(hint) == "Polite"),
        json!({"help_open":true,"live":"Polite"}),
        json!({"help_open":d.app().help_open,"hint":hint}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.command("models")?;
    d.settled()?;
    let focus = focused(d);
    let dialog = node(d, |value| {
        value["role"] == "Dialog" && value["label"] == "Models"
    });
    let group = node(d, |value| {
        value["role"] == "Group" && text(value, "label").starts_with("English transcription")
    });
    d.check(
        "Models opens as a labelled dialog with focus inside it",
        d.app().models.open
            && dialog.is_some()
            && group.is_some()
            && focus
                .as_ref()
                .is_some_and(|focus| text(focus, "label") == "Close  Esc"),
        json!({"dialog":"Models","group":"English transcription…","focus":"Close  Esc"}),
        json!({"open":d.app().models.open,"dialog":dialog,"group":group,"focus":focus}),
    )?;
    d.capture("Models focused inside the dialog")?;
    d.key(Key::Escape)?;
    d.settled()?;
    // Focus returns to the pane on the frame after the sheet closes.
    for frame in 0..2 {
        d.step(&format!("Models closed {frame}"), false)?;
    }
    let focus = focused(d);
    d.check(
        "Closing Models returns focus to the pane",
        !d.app().models.open
            && focus.as_ref().is_some_and(|focus| {
                PANES.iter().any(|pane| text(focus, "label") == *pane)
                    || text(focus, "label").starts_with("Showing")
            }),
        json!("a pane"),
        json!(focus),
    )
}

/// Increase contrast raises borders and the keyboard focus ring; Reduce
/// motion removes animation. Replays never read the Mac's own settings, so
/// the preferences are applied explicitly here.
fn display(d: &mut Driver<'_>) -> Result<(), String> {
    let accommodated = Preferences {
        reduce_motion: true,
        increase_contrast: true,
    };
    accessibility::apply(&d.harness.ctx, accommodated);
    d.command("models")?;
    d.settled()?;
    let current = d.harness.ctx.global_style();
    let focus_ring = d.harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Rect(painted)
            if painted.stroke.color == style::CURSOR && painted.stroke.width >= 3.0)
    });
    d.check(
        "Increase contrast paints a 3-point focus ring and raised borders; Reduce motion stops animation",
        focus_ring
            && current.animation_time == 0.0
            && current.visuals.widgets.noninteractive.bg_stroke.color == style::HIGH_CONTRAST_BORDER,
        json!({"focus_ring":true,"animation_time":0.0,"border":format!("{:?}", style::HIGH_CONTRAST_BORDER)}),
        json!({"focus_ring":focus_ring,"animation_time":current.animation_time,"border":format!("{:?}", current.visuals.widgets.noninteractive.bg_stroke.color)}),
    )?;
    d.capture("Increase contrast focus ring in Models")?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.capture("Increase contrast workspace")?;
    minimum_window(d)?;
    accessibility::apply(&d.harness.ctx, Preferences::default());
    // Restore the replay's dynamic defaults after the explicit preferences.
    d.harness.ctx.all_styles_mut(|style| {
        let defaults = egui::Style::default();
        style.scroll_animation = defaults.scroll_animation;
    });
    d.settled()
}

fn resize(d: &mut Driver<'_>, size: egui::Vec2) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing accessibility replay viewport")?
        .inner_rect = Some(rect);
    d.step(&format!("Window {} by {}", size.x, size.y), true)
}

/// Increase contrast and Reduce motion at the minimum window: the footer,
/// status and compact layout still settle and stay painted.
fn minimum_window(d: &mut Driver<'_>) -> Result<(), String> {
    let original = d.harness.ctx.content_rect().size();
    resize(d, egui::vec2(960.0, 640.0))?;
    d.settled()?;
    d.key(Key::L)?;
    scenarios::footer_anchored(
        d,
        "Increase contrast footer meets the notice panel at 960 by 640",
    )?;
    d.settled()?;
    let painted = [
        "Edit boundary",
        "frame",
        "command",
        "keys",
        "BEATS",
        "INSPECTOR",
    ]
    .into_iter()
    .map(|needle| (needle, scenarios::text_paint_visibility(d, needle)))
    .collect::<Vec<_>>();
    d.check(
        "Increase contrast keeps footer keys and pane titles painted at 960 by 640",
        painted
            .iter()
            .all(|(_, runs)| runs.iter().any(|run| run["fully_visible"] == true)),
        json!("every needle has a fully visible run"),
        json!(
            painted
                .iter()
                .map(|(needle, runs)| json!({"needle":needle,"runs":runs}))
                .collect::<Vec<_>>()
        ),
    )?;
    d.capture("Increase contrast at the minimum window")?;
    resize(d, original)?;
    d.settled()
}
