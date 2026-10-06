//! Specification §7.4 dot-repeat of creative edits made outside a recording:
//! `:retime`, `:pitch`, `,h`, `+`/`-` over an Edit range, ranged captions and
//! cutaways, `:hold-duration` and `:gag-set` each commit as one semantic
//! instruction, so `.` repeats them with their exact parameters on a new
//! selection, and a recorded macro replays speed and a ranged gain step. Every step goes
//! through the production router, the semantic project service and the
//! store, with one Undo per edit.

use super::*;
use deadpan_core::{ExactRatio, GainDb, GainRange, NodeId, NodeKind, PitchPolicy, ProjectDocument};
use egui::{Event, Key, Modifiers};

fn document(d: &Driver<'_>) -> Result<ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| (*workspace.document).clone())
        .ok_or_else(|| "No project".to_owned())
}

fn selected(d: &Driver<'_>) -> Result<NodeId, String> {
    d.app()
        .selected_beat
        .clone()
        .ok_or_else(|| "No beat is selected".to_owned())
}

fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let before = d.revision();
    d.command(command)?;
    d.changed(&before)?;
    d.settled()
}

fn undo(d: &mut Driver<'_>, times: usize) -> Result<(), String> {
    for _ in 0..times {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
        d.settled()?;
    }
    Ok(())
}

fn stroke(key: Key, modifiers: Modifiers, text: Option<&str>) -> Vec<Event> {
    let mut events = vec![key_event(key, modifiers, true)];
    if let Some(text) = text {
        events.push(Event::Text(text.into()));
    }
    events.push(key_event(key, modifiers, false));
    events
}

fn dot(d: &mut Driver<'_>, what: &str) -> Result<(), String> {
    let before = d.revision();
    d.events(what, stroke(Key::Period, Modifiers::NONE, Some(".")))?;
    d.changed(&before)?;
    d.settled()
}

/// Move the Edit cursor to an absolute frame.
fn at(d: &mut Driver<'_>, frame: u32) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        let mut keys: Vec<Key> = frame
            .to_string()
            .bytes()
            .map(|digit| match digit {
                b'0' => Key::Num0,
                b'1' => Key::Num1,
                b'2' => Key::Num2,
                b'3' => Key::Num3,
                b'4' => Key::Num4,
                b'5' => Key::Num5,
                b'6' => Key::Num6,
                b'7' => Key::Num7,
                b'8' => Key::Num8,
                _ => Key::Num9,
            })
            .collect();
        keys.push(Key::L);
        d.chord(&keys)?;
    }
    d.settled()
}

/// Select `[start, start + frames)` with `v`.
fn range(d: &mut Driver<'_>, start: u32, frames: u32) -> Result<(), String> {
    at(d, start)?;
    d.key(Key::V)?;
    let mut keys: Vec<Key> = frames
        .to_string()
        .bytes()
        .map(|digit| match digit {
            b'1' => Key::Num1,
            b'2' => Key::Num2,
            b'3' => Key::Num3,
            b'4' => Key::Num4,
            b'5' => Key::Num5,
            b'6' => Key::Num6,
            b'7' => Key::Num7,
            b'8' => Key::Num8,
            b'9' => Key::Num9,
            _ => Key::Num0,
        })
        .collect();
    keys.push(Key::L);
    d.chord(&keys)?;
    d.settled()
}

fn retime_of(document: &ProjectDocument, node: &NodeId) -> Option<(i64, i64, PitchPolicy)> {
    match &document.nodes().get(node)?.kind {
        NodeKind::Retime {
            duration,
            mapping,
            pitch,
            ..
        } => Some((duration.frames(), mapping.duration().frames(), *pitch)),
        _ => None,
    }
}

fn holds(document: &ProjectDocument) -> Vec<i64> {
    document
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            NodeKind::Hold { recipe } => Some(recipe.duration.frames()),
            _ => None,
        })
        .collect()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Speed, pitch and gain are checked as authored recipes; their processed PCM is verified by the deadpan-audio stage tests and the release preview/export fixtures, not by listening.".into());
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    // Three beats: split the Original at frames 40 and 80.
    for frame in [40, 80] {
        at(d, frame)?;
        let before = d.revision();
        d.key(Key::S)?;
        d.changed(&before)?;
        d.settled()?;
    }
    let baseline = document(d)?;
    speed(d)?;
    pause(d)?;
    gain(d)?;
    captions(d)?;
    hold_length(d)?;
    gag_parameters(d)?;
    seams(d)?;
    mute_and_lag(d)?;
    recorded(d)?;
    sting(d)?;
    d.check(
        "Every creative edit undoes back to the three-beat baseline",
        document(d)?.nodes() == baseline.nodes(),
        json!({"frames":120}),
        json!({"frames":d.app().sequence_length()}),
    )
}

/// `:retime 2` on one beat, then `.` on the next beat: each beat runs twice
/// as fast from its own input.
fn speed(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 0)?;
    edit(d, "retime 2 pitch=preserve")?;
    let first = selected(d)?;
    d.check(
        ":retime 2 wraps the first beat at twice the speed",
        retime_of(&document(d)?, &first) == Some((20, 40, PitchPolicy::Preserve))
            && d.app().sequence_length() == 100,
        json!({"duration":20,"input":40,"frames":100}),
        json!({"retime":format!("{:?}", retime_of(&document(d)?, &first)),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.key(Key::J)?;
    d.settled()?;
    let footer = scenarios::text_paint_visibility(d, "repeat: speed 2/1");
    d.check(
        "The footer names the repeatable speed change",
        !footer.is_empty(),
        json!("repeat: speed 2/1× · preserve pitch"),
        json!({"painted":footer,"message":d.app().message}),
    )?;
    dot(d, "Dot repeats the speed change")?;
    let second = selected(d)?;
    d.check(
        ". retimes the newly selected beat by its own 40-frame input",
        second != first
            && retime_of(&document(d)?, &second) == Some((20, 40, PitchPolicy::Preserve))
            && d.app().sequence_length() == 80,
        json!({"duration":20,"frames":80}),
        json!({"retime":format!("{:?}", retime_of(&document(d)?, &second)),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.capture("Speed repeated with dot")?;
    // A pitch shift repeats as a shift at each beat's own speed.
    edit(d, "pitch +3st")?;
    d.key(Key::K)?;
    d.settled()?;
    dot(d, "Dot repeats the pitch shift")?;
    d.check(
        ". shifts the other Retime three semitones at its own speed",
        retime_of(&document(d)?, &first)
            == Some((20, 40, PitchPolicy::Shift { semitones: 3 }))
            && retime_of(&document(d)?, &second)
                == Some((20, 40, PitchPolicy::Shift { semitones: 3 })),
        json!({"pitch":"+3st"}),
        json!({"first":format!("{:?}", retime_of(&document(d)?, &first)),"second":format!("{:?}", retime_of(&document(d)?, &second))}),
    )?;
    undo(d, 4)?;
    d.check(
        "Four undos remove the speed and pitch changes one at a time",
        d.app().sequence_length() == 120,
        json!({"frames":120}),
        json!({"frames":d.app().sequence_length()}),
    )
}

/// `:hold 6f` at one cursor, then `.` at another: two six-frame pauses.
fn pause(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 10)?;
    edit(d, "hold 6f")?;
    at(d, 70)?;
    dot(d, "Dot repeats the pause")?;
    let lengths = holds(&document(d)?);
    d.check(
        ". inserts the same six-frame pause at the new cursor",
        lengths == [6, 6] && d.app().sequence_length() == 132,
        json!({"holds":[6, 6],"frames":132}),
        json!({"holds":lengths,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, 2)
}

/// `+` twice over an Edit range adds +6 dB to that range of its beat only;
/// `.` adds the same +3 dB step over a new range on another beat.
fn gain(d: &mut Driver<'_>) -> Result<(), String> {
    range(d, 50, 10)?;
    let host = selected(d)?;
    for _ in 0..2 {
        let before = d.revision();
        d.key(Key::Plus)?;
        d.changed(&before)?;
        d.settled()?;
    }
    let step = |document: &ProjectDocument, node: &NodeId, start: i64, end: i64| {
        document
            .nodes()
            .get(node)
            .and_then(|node| node.audio_treatments.clip_gain().cloned())
            .and_then(|clip| {
                GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end))
                    .ok()
                    .and_then(|range| clip.range_step(range))
                    .map(|value| (value, clip.trim(), clip.envelopes().len()))
            })
    };
    let raised = step(&document(d)?, &host, 10, 20);
    d.check(
        "+ twice over Edit [50, 60) raises only that range of the middle beat by 6 dB",
        raised == Some((GainDb::new(6_000).unwrap(), GainDb::UNITY, 1))
            && d.app().sequence_length() == 120,
        json!({"range":[10, 20],"step":6000,"trim":0,"envelopes":1}),
        json!({"step":format!("{raised:?}"),"error":d.app().error,"message":d.app().message}),
    )?;
    d.step("Paint the ranged gain", true)?;
    d.capture("Gain over an Edit range")?;
    range(d, 90, 6)?;
    let other = selected(d)?;
    dot(d, "Dot repeats the ranged gain step")?;
    let repeated = step(&document(d)?, &other, 10, 16);
    d.check(
        ". adds the same +3 dB step over the new Edit range of the last beat",
        other != host && repeated == Some((GainDb::new(3_000).unwrap(), GainDb::UNITY, 1)),
        json!({"range":[10, 16],"step":3000}),
        json!({"step":format!("{repeated:?}"),"error":d.app().error}),
    )?;
    undo(d, 3)
}

/// A caption over an Edit range, repeated over another range with `.`.
fn captions(d: &mut Driver<'_>) -> Result<(), String> {
    range(d, 4, 8)?;
    edit(d, "caption Wait for it")?;
    range(d, 44, 8)?;
    dot(d, "Dot repeats the ranged caption")?;
    // Captions belong to the beat's Source host, in that host's own clock;
    // both fragments of the split Original share it.
    let document = document(d)?;
    let mut ranges: Vec<(i64, i64)> = document
        .nodes()
        .values()
        .flat_map(|node| node.captions.iter())
        .filter(|caption| caption.text == "Wait for it")
        .map(|caption| (caption.range.start().0, caption.range.end().0))
        .collect();
    ranges.sort_unstable();
    d.check(
        "A caption over Edit [4, 12) and its dot over [44, 52) cover exactly those ranges",
        ranges == [(4, 12), (44, 52)],
        json!({"ranges":[[4, 12], [44, 52]]}),
        json!({"ranges":ranges,"error":d.app().error}),
    )?;
    d.step("Paint the ranged captions", true)?;
    d.capture("Ranged caption repeated with dot")?;
    undo(d, 2)
}

/// `:hold-duration` on one pause, repeated on another.
fn hold_length(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 20)?;
    edit(d, "hold 4f")?;
    at(d, 104)?;
    edit(d, "hold 4f")?;
    at(d, 20)?;
    edit(d, "hold-duration 9f")?;
    // The second pause now starts five frames later.
    at(d, 109)?;
    dot(d, "Dot repeats the pause length")?;
    let lengths = holds(&document(d)?);
    d.check(
        ". sets the other pause to the same nine frames",
        lengths == [9, 9] && d.app().sequence_length() == 138,
        json!({"holds":[9, 9],"frames":138}),
        json!({"holds":lengths,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, 4)
}

/// A macro records a speed change and a ranged gain step, then replays both
/// on another beat as one edit.
fn recorded(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 0)?;
    d.key(Key::Q)?;
    d.key(Key::A)?;
    edit(d, "retime 1/2 pitch=tape")?;
    // Escape would cancel the draft; the recorded range starts at the
    // slowed beat's start, where the speed change leaves the cursor.
    d.key(Key::V)?;
    d.chord(&[Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let before = d.revision();
    d.key(Key::Minus)?;
    d.changed(&before)?;
    d.settled()?;
    d.key(Key::Q)?;
    d.wait_for("The macro is saved", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    d.settled()?;
    let first = selected(d)?;
    let stepped = |document: &ProjectDocument, node: &NodeId| {
        document
            .nodes()
            .get(node)
            .and_then(|node| node.audio_treatments.clip_gain().cloned())
            .and_then(|clip| {
                GainRange::new(ExactRatio::integer(0), ExactRatio::integer(10))
                    .ok()
                    .and_then(|range| clip.range_step(range))
            })
    };
    d.check(
        "Recording keeps :retime and a ranged - as edits of the recorded beat",
        retime_of(&document(d)?, &first) == Some((80, 40, PitchPolicy::FollowSpeed))
            && stepped(&document(d)?, &first) == Some(GainDb::new(-3_000).unwrap())
            && d.app().sequence_length() == 160,
        json!({"duration":80,"pitch":"tape","step":-3000,"frames":160}),
        json!({"retime":format!("{:?}", retime_of(&document(d)?, &first)),"step":format!("{:?}", stepped(&document(d)?, &first)),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    at(d, 120)?;
    let before = d.revision();
    let mut events = stroke(Key::Num2, Modifiers::SHIFT, Some("@"));
    events.extend(stroke(Key::A, Modifiers::NONE, Some("a")));
    d.events("Run macro a", events)?;
    d.changed(&before)?;
    d.settled()?;
    let last = selected(d)?;
    let document = document(d)?;
    d.check(
        "@a slows the last beat to half speed with tape pitch and lowers its first ten frames 3 dB, as one edit",
        last != first
            && retime_of(&document, &last) == Some((80, 40, PitchPolicy::FollowSpeed))
            && stepped(&document, &last) == Some(GainDb::new(-3_000).unwrap())
            && d.app().sequence_length() == 200,
        json!({"duration":80,"step":-3000,"frames":200}),
        json!({"retime":format!("{:?}", retime_of(&document, &last)),"step":format!("{:?}", stepped(&document, &last)),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, 1)?;
    d.check(
        "One undo removes the whole macro run",
        d.app().sequence_length() == 160,
        json!({"frames":160}),
        json!({"frames":d.app().sequence_length()}),
    )?;
    undo(d, 2)
}

/// `:gag-set` changes an inserted gag's parameters after insertion, the
/// inspector offers it with the current values, and `.` sets the same
/// parameters on another gag of the same recipe.
fn gag_parameters(d: &mut Driver<'_>) -> Result<(), String> {
    let label = |d: &Driver<'_>| -> Result<String, String> {
        let node = selected(d)?;
        Ok(document(d)?.nodes()[&node].label.clone())
    };
    at(d, 0)?;
    edit(d, "gag one-more-time plays=3 gap=12f shorten=3f")?;
    at(d, 181)?;
    edit(d, "gag one-more-time plays=3 gap=12f shorten=3f")?;
    d.check(
        "Two One More Time gags: 3 plays with 12- and 9-frame gaps on the first and last beats",
        d.app().sequence_length() == 322,
        json!({"frames":322}),
        json!({"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    at(d, 0)?;
    d.step("Paint the gag's inspector", true)?;
    let offered = scenarios::text_paint_visibility(d, "Change parameters");
    let parameters = scenarios::text_paint_visibility(d, "plays 3, gap 12f, shorten 3f");
    d.check(
        "The inspector names the gag's recipe and parameters and offers to change them",
        !offered.is_empty() && !parameters.is_empty(),
        json!({"action":"Change parameters…","parameters":"plays 3, gap 12f, shorten 3f"}),
        json!({"action":offered,"parameters":parameters,"label":label(d)?}),
    )?;
    d.capture("Gag parameters in the inspector")?;
    edit(d, "gag-set plays=4")?;
    d.check(
        ":gag-set plays=4 adds a play and its 6-frame gap and repins the label, as one edit",
        label(d)? == "One More Time · v1 · 4 plays, gap 12f shortening by 3f"
            && d.app().sequence_length() == 368,
        json!({"label":"One More Time · v1 · 4 plays, gap 12f shortening by 3f","frames":368}),
        json!({"label":label(d)?,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    // The gag is captured when `:` opens: selecting the other gag before
    // Enter cannot retarget the change.
    let entry = d.revision();
    d.key(Key::Colon)?;
    let other = d
        .app()
        .beat_rows
        .iter()
        .find(|row| row.start == 227)
        .map(|row| row.id.clone())
        .ok_or("No second gag at 227")?;
    d.app_mut().selected_beat = Some(other);
    d.events(
        "Finish :gag-set after the selection changed",
        vec![
            Event::Text("gag-set plays=5".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.settled()?;
    d.check(
        ":gag-set refuses when the selection changes between : and Enter",
        d.revision() == entry && d.app().error.is_some(),
        json!({"revision":"unchanged","error":"captured context changed"}),
        json!({"unchanged":d.revision() == entry,"error":d.app().error}),
    )?;
    at(d, 227)?;
    dot(d, "Dot sets the same parameters on the other gag")?;
    d.check(
        ". gives the other One More Time the same four plays",
        label(d)? == "One More Time · v1 · 4 plays, gap 12f shortening by 3f"
            && d.app().sequence_length() == 414,
        json!({"frames":414}),
        json!({"label":label(d)?,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    // The gag's recipe becomes a preset for every project, and a saved local
    // copy of it keeps its exposed parameters.
    edit_none(d, "gag-save stutter-4")?;
    let saved = d.app().message.clone().unwrap_or_default();
    edit_none(d, "gag-presets")?;
    let listed =
        d.app().help_open && d.app().message.as_deref() == Some("1 saved gag preset; see Help.");
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        ":gag-save keeps the gag's recipe as a preset and :gag-presets lists it",
        saved.starts_with("Saved gag preset stutter-4: One More Time · v1 · 4 plays") && listed,
        json!({"saved":"Saved gag preset stutter-4: One More Time · v1 · 4 plays…","listed":true}),
        json!({"saved":saved,"listed":listed,"error":d.app().error}),
    )?;
    edit_none(d, "recipe-save a")?;
    at(d, 0)?;
    edit(d, "recipe a")?;
    at(d, 0)?;
    edit(d, "gag-set plays=2")?;
    let copy = label(d)?;
    at(d, 279)?;
    edit(d, "gag stutter-4")?;
    let preset = label(d)?;
    d.check(
        "A saved local copy of the gag still takes :gag-set, and :gag stutter-4 inserts the preset",
        copy == "One More Time · v1 · 2 plays, gap 12f shortening by 3f"
            && preset == "One More Time · v1 · 4 plays, gap 12f shortening by 3f"
            && d.app().sequence_length() == 653,
        json!({"copy":"2 plays","preset":"4 plays","frames":653}),
        json!({"copy":copy,"preset":preset,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, 7)
}

/// A command that changes no project revision.
fn edit_none(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    d.command(command)?;
    d.settled()
}

/// `:edge hard plays` cuts a Repeat's play seams without the automatic fade,
/// and `.` does the same on another Repeat.
fn seams(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_core::{AudioBoundaryKind, AudioEdgePolicy};
    let seams = |d: &Driver<'_>| -> Result<(AudioEdgePolicy, AudioEdgePolicy), String> {
        let node = selected(d)?;
        let document = document(d)?;
        let NodeKind::Repeat { child, .. } = &document.nodes()[&node].kind else {
            return Err("The selection is not a Repeat".into());
        };
        let edges = document.nodes()[child].audio_edges;
        Ok((
            edges.get(AudioBoundaryKind::NodeStart),
            edges.get(AudioBoundaryKind::NodeEnd),
        ))
    };
    for frame in [0, 80] {
        at(d, frame)?;
        edit(d, "repeat 2")?;
    }
    at(d, 0)?;
    edit(d, "edge hard plays")?;
    d.check(
        ":edge hard plays cuts the first Repeat's play seams hard",
        seams(d)? == (AudioEdgePolicy::Hard, AudioEdgePolicy::Hard)
            && d.app().sequence_length() == 200,
        json!({"seams":"hard","frames":200}),
        json!({"seams":format!("{:?}", seams(d)?),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    at(d, 80)?;
    dot(d, "Dot cuts the other Repeat's seams hard")?;
    d.check(
        ". cuts the other Repeat's seams hard too",
        seams(d)? == (AudioEdgePolicy::Hard, AudioEdgePolicy::Hard),
        json!({"seams":"hard"}),
        json!({"seams":format!("{:?}", seams(d)?),"error":d.app().error}),
    )?;
    // A split fragment's end, marked as an editorial edge and cut hard.
    at(d, 160)?;
    edit(d, "edge hard end")?;
    let node = selected(d)?;
    let fragment = document(d)?.nodes()[&node].clone();
    d.step("Paint the hard edge in the inspector", true)?;
    let painted = scenarios::text_paint_visibility(d, "Hard sound edges");
    d.check(
        ":edge hard end marks the fragment's end and cuts it hard; the inspector lists it",
        fragment.audio_editorial_edges.end
            && fragment.audio_edges.get(AudioBoundaryKind::NodeEnd) == AudioEdgePolicy::Hard
            && !painted.is_empty(),
        json!({"marked":true,"end":"Hard","inspector":"Hard sound edges"}),
        json!({"marked":fragment.audio_editorial_edges.end,"end":format!("{:?}", fragment.audio_edges.get(AudioBoundaryKind::NodeEnd)),"painted":painted,"error":d.app().error}),
    )?;
    undo(d, 5)
}

/// `,m` mutes a whole beat and `:audio-lag` offsets a beat's sound; `.`
/// repeats each on another beat.
fn mute_and_lag(d: &mut Driver<'_>) -> Result<(), String> {
    let muted = |d: &Driver<'_>| -> Result<bool, String> {
        let node = selected(d)?;
        Ok(document(d)?.nodes()[&node]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| clip.muted()))
    };
    let lag = |d: &Driver<'_>| -> Result<Option<i64>, String> {
        let node = selected(d)?;
        let document = document(d)?;
        Ok(
            deadpan_core::cutaway_host(&document, &node).and_then(|(host, _)| {
                match &document.nodes()[&host].kind {
                    NodeKind::Source { source } => Some(source.audio_offset.0),
                    _ => None,
                }
            }),
        )
    };
    at(d, 0)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&before)?;
    d.settled()?;
    let first = muted(d)?;
    at(d, 40)?;
    dot(d, "Dot mutes the next beat")?;
    d.check(
        ",m mutes the first beat and . mutes the next one",
        first && muted(d)?,
        json!({"first":true,"second":true}),
        json!({"first":first,"second":muted(d)?,"error":d.app().error}),
    )?;
    // Each split fragment keeps its own copy of the Original's Source, so
    // dot offsets the next beat's sound independently.
    edit(d, "audio-lag +80ms")?;
    let repeatable = d.app().semantic.snapshot().is_some_and(|snapshot| {
        matches!(
            snapshot.edit.as_ref().map(|edit| &edit.operation),
            Some(crate::project::semantic::RepeatableEdit::Parameter(
                deadpan_core::SemanticInstruction::SetAudioLag { offset }
            )) if offset.0 == 3_840
        )
    });
    d.check(
        ":audio-lag +80ms offsets the beat's sound by 3,840 samples and becomes the dot edit",
        lag(d)? == Some(3_840) && repeatable,
        json!({"offset":3840,"dot":"SetAudioLag 3840"}),
        json!({"offset":lag(d)?,"dot":repeatable,"error":d.app().error,"message":d.app().message}),
    )?;
    at(d, 80)?;
    dot(d, "Dot offsets the last beat's sound")?;
    d.check(
        ". offsets the last beat's own Source copy by the same 3,840 samples",
        lag(d)? == Some(3_840),
        json!({"offset":3840}),
        json!({"offset":lag(d)?,"error":d.app().error}),
    )?;
    // Unmuting restores the plain recipe, and `.` of the unmute on a beat
    // that was never muted changes nothing and says so.
    at(d, 40)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&before)?;
    d.settled()?;
    let plain = document(d)?.nodes()[&selected(d)?]
        .audio_treatments
        .is_empty();
    at(d, 0)?;
    dot(d, "Dot unmutes the first beat")?;
    let first_plain = document(d)?.nodes()[&selected(d)?]
        .audio_treatments
        .is_empty();
    at(d, 80)?;
    let unchanged = d.revision();
    d.events(
        "Dot unmute on a beat that was never muted",
        stroke(Key::Period, Modifiers::NONE, Some(".")),
    )?;
    d.wait_for("The no-op reply arrives", |app| {
        !app.macros.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Unmute leaves no inert gain stage, and a no-op dot reports \"No edit was made\" without an edit",
        plain
            && first_plain
            && d.revision() == unchanged
            && d.app().error.is_none()
            && d.app().message.as_deref().is_some_and(|message| message.contains("No edit was made")),
        json!({"plain":true,"no_edit":true}),
        json!({"plain":plain,"first_plain":first_plain,"unchanged":d.revision() == unchanged,"error":d.app().error,"message":d.app().message}),
    )?;
    // With an Edit range, :saturate refuses as before and :gain-mute still
    // toggles the whole beat.
    range(d, 44, 6)?;
    let entry = d.revision();
    d.command("saturate 6dB")?;
    d.wait_for("The refusal arrives", |app| {
        !app.macros.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    let refused = d.revision() == entry
        && d.app().error.as_deref().is_some_and(|error| {
            error.contains("clear the Visual selection before changing a beat's gain or saturation")
        });
    edit(d, "gain-mute")?;
    let muted = document(d)?.nodes()[&selected(d)?]
        .audio_treatments
        .clip_gain()
        .is_some_and(|clip| clip.muted() && clip.mute_ranges().is_empty());
    d.check(
        "With a range, :saturate refuses with the previous message and :gain-mute mutes the whole beat",
        refused && muted,
        json!({"saturate":"refused","gain_mute":"whole beat"}),
        json!({"refused":refused,"muted":muted,"error":d.app().error}),
    )?;
    d.key(Key::Escape)?;
    undo(d, 7)
}

/// `:sting` adds Deadpan's own synthesized sting to the sound catalog through
/// the ordinary import, and `,s` places it at the Edit cursor.
fn sting(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sting")?;
    d.wait_for("The sting is qualified in the catalog", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.settled()?;
    let row = d.app().sound_rows[0].clone();
    d.check(
        ":sting imports the bundled sting as a catalog sound",
        row.1.contains("Triumphant sting") && d.app().error.is_none(),
        json!({"catalog":"Triumphant sting.wav"}),
        json!({"catalog":row.1,"error":d.app().error}),
    )?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    at(d, 20)?;
    d.click(&row.1)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::S])?;
    d.changed(&before)?;
    d.settled()?;
    let sounds = document(d)?.sounds().clone();
    let placed = sounds.values().next().cloned();
    d.check(
        ",s places the sting at the Edit cursor as an ordinary sound event",
        sounds.len() == 1
            && placed.as_ref().is_some_and(|event| {
                event.offset
                    == document(d)
                        .ok()
                        .and_then(|document| {
                            document
                                .presentation_basis()
                                .frame_rate
                                .audio_boundary(ProjectFrame(20))
                                .ok()
                        })
                        .unwrap_or(deadpan_core::AudioSample(-1))
            }),
        json!({"sounds":1,"at":"Edit 20"}),
        json!({"sounds":sounds.len(),"event":format!("{placed:?}"),"error":d.app().error}),
    )?;
    d.capture("The synthesized sting placed at the Edit cursor")?;
    undo(d, 1)
}
