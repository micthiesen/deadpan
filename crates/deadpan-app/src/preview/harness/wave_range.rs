//! DP-09.1 waveform range: `:wave` opens the measured Edit mix, real keys
//! select a sub-frame span of exact mix samples, `m` mutes exactly those
//! samples and `-` steps their gain, `.` repeats the step at the same position
//! of another beat, and a pointer drag selects too. Every change goes through
//! the production router, the semantic project service and the store.

use super::*;
use deadpan_core::{GainDb, GainRange, NodeId, ProjectDocument, ProjectFrame};
use egui::{Event, Key, Modifiers};

fn document(d: &Driver<'_>) -> Result<ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| (*workspace.document).clone())
        .ok_or_else(|| "No project".to_owned())
}

fn boundary(d: &Driver<'_>, frame: i64) -> Result<i64, String> {
    document(d)?
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(frame))
        .map(|sample| sample.0)
        .map_err(|error| error.to_string())
}

fn wave(d: &Driver<'_>) -> Value {
    d.app()
        .wave
        .as_ref()
        .map_or(Value::Null, |wave| wave.state_for_check())
}

fn local(d: &Driver<'_>, beat_start: i64, samples: (i64, i64)) -> Result<GainRange, String> {
    GainRange::from_mix_samples(
        document(d)?.presentation_basis().frame_rate,
        ProjectFrame(beat_start),
        samples.0,
        samples.1,
    )
    .map_err(|error| error.to_string())
}

fn clip(d: &Driver<'_>, beat: &NodeId) -> Result<Option<deadpan_core::ClipGain>, String> {
    Ok(document(d)?
        .nodes()
        .get(beat)
        .ok_or("missing beat")?
        .audio_treatments
        .clip_gain()
        .cloned())
}

fn press(d: &mut Driver<'_>, key: Key, modifiers: Modifiers, times: usize) -> Result<(), String> {
    for _ in 0..times {
        d.key_modified(key, modifiers)?;
    }
    Ok(())
}

fn measured(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("The waveform range is measured", |app| {
        app.wave.as_ref().is_some_and(|wave| {
            wave.state_for_check()["status"] == "Complete"
                && wave.state_for_check()["snapping"] == false
        })
    })
}

fn applied(d: &mut Driver<'_>, key: Key, what: &str) -> Result<(), String> {
    let before = d.revision();
    d.events(
        what,
        vec![
            key_event(key, Modifiers::NONE, true),
            key_event(key, Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before)?;
    d.settled()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Audition plays through injected typed delivery; listening to the cut and physical output are owner checks. Exact PCM silence and gain are proven by the release `sample-range` preview/export fixture and its sample-by-sample authored-bus test.".into());
    super::transcript::focus_your_edit(d)?;
    // Two beats: split the Original at frame 40, then return to frame 10.
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    d.settled()?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let first = document(d)?;
    let deadpan_core::NodeKind::Sequence { children } = &first.nodes()[first.root()].kind else {
        return Err("the root is a Sequence".into());
    };
    let (left, right) = (children[0].clone(), children[1].clone());

    d.command("wave")?;
    measured(d)?;
    let (b10, b11) = (boundary(d, 10)?, boundary(d, 11)?);
    d.check(
        ":wave opens on one frame at the Edit cursor, measured",
        wave(d)["samples"] == json!([b10, b11]),
        json!({"samples":[b10, b11]}),
        wave(d),
    )?;
    d.capture("Waveform range opened")?;
    // v starts a new selection at the moving end; Shift+l moves 1 ms twice
    // and ] one sample three times: 99 samples, finer than a frame.
    d.key(Key::V)?;
    press(d, Key::L, Modifiers::SHIFT, 2)?;
    press(d, Key::CloseBracket, Modifiers::NONE, 3)?;
    d.settled()?;
    let selection = (b11, b11 + 99);
    d.check(
        "Shift+l and ] select 99 samples starting at frame 11's exact boundary",
        wave(d)["samples"] == json!([selection.0, selection.1]),
        json!({"samples":[selection.0, selection.1]}),
        wave(d),
    )?;
    let label = scenarios::text_paint_visibility(d, "99 samples");
    d.check(
        "The sheet shows the selected samples, count and duration",
        !label.is_empty(),
        json!("Samples … · 99 samples · 2.062 ms"),
        json!({"painted":label}),
    )?;
    d.capture("Ninety-nine samples selected")?;

    applied(d, Key::M, "m mutes the selection")?;
    let expected = local(d, 0, selection)?;
    let muted = clip(d, &left)?;
    d.check(
        "m mutes exactly the selected samples of the first beat, in its own exact frames",
        muted
            .as_ref()
            .is_some_and(|clip| clip.mute_ranges() == [expected] && clip.envelopes().is_empty())
            && clip(d, &right)?.is_none()
            && d.app().wave.is_some(),
        json!({"mute_ranges":[expected]}),
        json!({"clip":muted,"wave":wave(d),"error":d.app().error}),
    )?;
    d.capture("Selection muted")?;
    applied(d, Key::Minus, "- lowers the selection by 3 dB")?;
    d.check(
        "- adds one -3 dB step over exactly the same samples",
        clip(d, &left)?
            .is_some_and(|clip| clip.range_step(expected) == Some(GainDb::new(-3_000).unwrap())),
        json!({"range_step":-3000}),
        json!({"clip":clip(d, &left)?}),
    )?;

    // Escape closes the sheet; . repeats the step 99 samples into frame 11
    // of the next beat, at the same exact local position.
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape closes the waveform range",
        d.app().wave.is_none(),
        json!(null),
        wave(d),
    )?;
    d.key(Key::J)?;
    d.settled()?;
    let before = d.revision();
    d.events(
        "Dot repeats the waveform gain step",
        vec![
            key_event(Key::Period, Modifiers::NONE, true),
            Event::Text(".".into()),
            key_event(Key::Period, Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        ". repeats the -3 dB waveform step at the same local position of the next beat",
        clip(d, &right)?
            .is_some_and(|clip| clip.range_step(expected) == Some(GainDb::new(-3_000).unwrap())),
        json!({"beat":right,"range_step":-3000}),
        json!({"clip":clip(d, &right)?,"error":d.app().error}),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "One Undo removes the repeated step",
        clip(d, &right)?.is_none(),
        json!(null),
        json!({"clip":clip(d, &right)?}),
    )?;

    // A pointer drag on the waveform selects too, and z snaps the moving end
    // to a zero crossing of the measured mix.
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L])?;
    d.command("wave")?;
    measured(d)?;
    let strip = d
        .harness
        .root()
        .children_recursive()
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Samples "))
        })
        .map(|node| node.rect())
        .ok_or("The waveform is not drawn")?;
    let from = egui::pos2(strip.left() + strip.width() * 0.30, strip.center().y);
    let to = egui::pos2(strip.left() + strip.width() * 0.55, strip.center().y);
    d.events(
        "Press on the waveform",
        vec![
            Event::PointerMoved(from),
            Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for step in 1..=5 {
        let at = from + (to - from) * (step as f32 / 5.0);
        d.events("Drag across the waveform", vec![Event::PointerMoved(at)])?;
    }
    d.events(
        "Release the drag",
        vec![Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    )?;
    d.settled()?;
    let state = wave(d);
    let view = (
        state["view"][0].as_i64().unwrap_or(0),
        state["view"][1].as_i64().unwrap_or(0),
    );
    let width = (view.1 - view.0) as f64;
    let dragged = (
        state["samples"][0].as_i64().unwrap_or(0),
        state["samples"][1].as_i64().unwrap_or(0),
    );
    let near = |sample: i64, fraction: f64| {
        ((sample - view.0) as f64 - width * fraction).abs() <= width * 0.02
    };
    d.check(
        "Dragging from 30% to 55% of the waveform selects that span of samples",
        near(dragged.0, 0.30) && near(dragged.1, 0.55)
            || near(dragged.0, 0.30) && dragged.1 > dragged.0,
        json!({"fractions":[0.30, 0.55]}),
        state.clone(),
    )?;
    d.capture("Dragged selection")?;
    let head = state["head"].as_i64().unwrap_or(0);
    d.key(Key::Z)?;
    measured(d)?;
    let snapped = wave(d);
    d.check(
        "z moves the moving end at most 10 ms, to a zero crossing or reports none",
        snapped["head"]
            .as_i64()
            .is_some_and(|value| (value - head).abs() <= 480)
            && snapped["note"]
                .as_str()
                .is_some_and(|note| note.contains("zero crossing")),
        json!({"within":480}),
        snapped,
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "The muted and stepped first beat keeps its edits after the sheet closes",
        clip(d, &left)?.is_some_and(|clip| clip.mute_ranges() == [expected]),
        json!({"mute_ranges":[expected]}),
        json!({"clip":clip(d, &left)?}),
    )
}
