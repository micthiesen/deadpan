//! Specification §8.3 "Premature sound (J-cut)" and "Lingering sound
//! (L-cut)", and §6.4 `:roll +2f`: `:jcut` / `:lcut` at a cut between two
//! beats from different Original moments roll the linked cut while a cutaway
//! keeps every picture, through the production router, semantic project
//! service and store, with one Undo; `.` repeats it at another cut.

use super::*;
use deadpan_core::{NodeKind, ProjectFrame};
use egui::{Event, Key, Modifiers};

/// The Original picture shown at each Edit frame.
fn pictures(d: &Driver<'_>, frames: &[i64]) -> Result<Vec<u64>, String> {
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let index = workspace
        .sources
        .values()
        .find_map(|source| source.video_index.clone())
        .ok_or("No picture index")?;
    frames
        .iter()
        .map(|frame| {
            workspace
                .plan
                .picture(ProjectFrame(*frame))
                .map_err(|e| e.to_string())?
                .picture
                .select_source_frame(&index)
                .map(|selected| selected.identity.0)
                .map_err(|e| e.to_string())
        })
        .collect()
}

fn lengths(d: &Driver<'_>) -> Vec<u64> {
    d.app().beat_rows.iter().map(|row| row.frames).collect()
}

fn at(d: &mut Driver<'_>, frame: u32) -> Result<(), String> {
    let mut keys = vec![Key::G, Key::G];
    for digit in frame.to_string().chars() {
        keys.push(Key::from_name(&digit.to_string()).ok_or("digit")?);
    }
    keys.push(Key::L);
    d.chord(&keys)?;
    d.settled()
}

fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let before = d.revision();
    d.command(command)?;
    d.changed(&before)?;
    d.settled()
}

fn undo(d: &mut Driver<'_>) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Split edits are checked through their structure and the pictures the shared plan selects; that the leading sound is heard there is verified by the j-cut and l-cut release preview/export fixtures, not by listening.".into());
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    // Two beats from different moments: Original [0, 40) then [70, 120).
    at(d, 40)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num3, Key::Num0, Key::L])?;
    edit(d, "delete")?;
    at(d, 40)?;
    let baseline = d.revision();
    let frames = [33, 34, 39, 40, 45, 46];
    let before = pictures(d, &frames)?;
    d.check(
        "The base edit cuts from Original 39 to Original 70 at Edit 40",
        lengths(d) == [40, 50] && before == [33, 34, 39, 70, 75, 76],
        json!({"beats":[40, 50],"pictures":[33, 34, 39, 70, 75, 76]}),
        json!({"beats":lengths(d),"pictures":before}),
    )?;

    edit(d, "jcut 6f")?;
    let after = pictures(d, &frames)?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let cutaways: usize = workspace
        .document
        .nodes()
        .values()
        .map(|node| node.cutaways.len())
        .sum();
    d.check(
        ":jcut 6f starts the second moment's sound 6 frames early while every picture and the duration stay",
        lengths(d) == [34, 56]
            && after == before
            && cutaways == 1
            && d.app().sequence_length() == 90
            && d.app().sequence_cursor == 40
            && d.app().error.is_none(),
        json!({"beats":[34, 56],"pictures":before,"cutaways":1,"frames":90,"cursor":40}),
        json!({"beats":lengths(d),"pictures":after,"cutaways":cutaways,"frames":d.app().sequence_length(),"cursor":d.app().sequence_cursor,"error":d.app().error,"message":d.app().message}),
    )?;
    d.capture("J-cut: the second moment's sound leads its picture")?;
    undo(d)?;
    d.check(
        "One undo restores the hard cut",
        lengths(d) == [40, 50] && d.revision() != baseline,
        json!([40, 50]),
        json!(lengths(d)),
    )?;

    // A second cut inside the second moment, for dot below.
    at(d, 70)?;
    let before_split = d.revision();
    d.key(Key::S)?;
    d.changed(&before_split)?;
    d.settled()?;
    at(d, 40)?;
    edit(d, "lcut 200ms")?;
    let after = pictures(d, &frames)?;
    d.check(
        ":lcut 200ms (6 frames) lets the first moment's sound run on under the second picture",
        lengths(d) == [46, 24, 20] && after == before && d.app().sequence_length() == 90,
        json!({"beats":[46, 24, 20],"pictures":before}),
        json!({"beats":lengths(d),"pictures":after,"error":d.app().error}),
    )?;
    d.step("Paint the inspector of the lingering beat", true)?;

    // Dot repeats the L-cut at the cut under the cursor.
    at(d, 70)?;
    let before = d.revision();
    d.events(
        "Dot repeats the L-cut",
        vec![
            key_event(Key::Period, Modifiers::NONE, true),
            Event::Text(".".into()),
            key_event(Key::Period, Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        ". repeats the 6-frame L-cut at the cut under the cursor",
        lengths(d) == [46, 30, 14] && d.app().sequence_length() == 90,
        json!([46, 30, 14]),
        json!({"beats":lengths(d),"error":d.app().error}),
    )?;
    undo(d)?;
    undo(d)?;
    undo(d)?;

    // Refusals leave history unchanged.
    at(d, 20)?;
    let before = d.revision();
    d.command("jcut 6f")?;
    d.wait_for("The refused split edit reply", |app| {
        !app.macros.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "A split edit away from a cut is refused with guidance and no edit",
        d.revision() == before
            && [d.app().error.as_deref(), d.app().message.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| text.contains("cut between two beats")),
        json!("refused"),
        json!({"error":d.app().error,"message":d.app().message,"revision":d.revision(),"before":before}),
    )?;

    // :roll opens Trim on the selected beat with Roll active.
    at(d, 20)?;
    d.command("roll +2f")?;
    d.settled()?;
    let roll = d.app().trim_state_for_check();
    d.check(
        ":roll +2f opens Trim with Roll active and a 2-frame roll",
        roll.as_ref().is_some_and(|(control, amount)| {
            *control == deadpan_core::SourceTrimControl::Roll && amount == "+2f"
        }),
        json!({"control":"Roll","amount":"+2f"}),
        json!(format!("{roll:?}")),
    )?;
    d.capture("Roll opened from :roll +2f")?;
    d.key(Key::Escape)?;
    d.settled()?;
    let kinds: Vec<_> = d
        .app()
        .workspace
        .as_ref()
        .map(|workspace| {
            d.app()
                .beat_rows
                .iter()
                .map(|row| {
                    matches!(
                        workspace.document.nodes()[&row.id].kind,
                        NodeKind::Retime { .. }
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    d.check(
        "Escape leaves Trim without an edit",
        lengths(d) == [40, 50] && kinds.len() == 2,
        json!([40, 50]),
        json!(lengths(d)),
    )?;
    roles(d)
}

/// Whether the shared plan shows the background at each Edit frame.
fn backgrounds(d: &Driver<'_>, frames: &[i64]) -> Result<Vec<bool>, String> {
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    frames
        .iter()
        .map(|frame| {
            Ok(matches!(
                workspace
                    .plan
                    .picture(ProjectFrame(*frame))
                    .map_err(|e| e.to_string())?
                    .picture,
                deadpan_plan::Picture::Background
            ))
        })
        .collect()
}

fn mute_ranges(d: &Driver<'_>) -> Vec<(i64, i64)> {
    let Some(workspace) = d.app().workspace.as_ref() else {
        return Vec::new();
    };
    let Some(first) = d.app().beat_rows.first() else {
        return Vec::new();
    };
    workspace.document.nodes()[&first.id]
        .audio_treatments
        .clip_gain()
        .map(|clip| {
            clip.mute_ranges()
                .iter()
                .map(|range| (range.start().floor() as i64, range.end().floor() as i64))
                .collect()
        })
        .unwrap_or_default()
}

/// §6.5 role-only deletes and `:select role=`: Visual `d` under a chosen role
/// silences or blanks one role inside a beat without moving anything.
fn roles(d: &mut Driver<'_>) -> Result<(), String> {
    let pictures_before = pictures(d, &[9, 10, 15, 16])?;
    // A chosen role ends when the Original is shown or the group changes.
    d.command("select role=audio")?;
    d.command("source")?;
    d.settled()?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.settled()?;
    let after_original = d.app().edit_role;
    d.command("select role=video")?;
    at(d, 0)?;
    d.key(Key::Enter)?;
    d.settled()?;
    let after_enter = d.app().edit_role;
    d.key(Key::Backspace)?;
    d.settled()?;
    d.check(
        "Showing the Original or entering a group returns the role to linked",
        after_original == deadpan_core::MediaRole::Linked
            && after_enter == deadpan_core::MediaRole::Linked,
        json!(["Linked", "Linked"]),
        json!([format!("{after_original:?}"), format!("{after_enter:?}")]),
    )?;
    at(d, 10)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num6, Key::L])?;
    d.command("select role=audio")?;
    d.settled()?;
    d.step("Paint the audio role in the status line", true)?;
    let painted = scenarios::text_paint_visibility(d, "AUDIO ROLE");
    d.check(
        ":select role=audio shows the chosen role beside the mode",
        !painted.is_empty(),
        json!("AUDIO ROLE"),
        json!(painted),
    )?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "With the audio role, d silences Edit [10, 16) of the beat and keeps its pictures and time",
        mute_ranges(d) == [(10, 16)]
            && lengths(d) == [40, 50]
            && pictures(d, &[9, 10, 15, 16])? == pictures_before,
        json!({"mute":[[10, 16]],"beats":[40, 50],"pictures":pictures_before}),
        json!({"mute":mute_ranges(d),"beats":lengths(d),"error":d.app().error}),
    )?;
    d.capture("Audio role delete")?;
    undo(d)?;

    at(d, 10)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num6, Key::L])?;
    d.command("select role=video")?;
    d.settled()?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    d.settled()?;
    let shown = backgrounds(d, &[9, 10, 15, 16])?;
    d.check(
        "With the video role, d removes Edit [10, 16)'s picture to the background and keeps its sound and time",
        shown == [false, true, true, false] && lengths(d) == [40, 50] && mute_ranges(d).is_empty(),
        json!({"background":[false, true, true, false],"beats":[40, 50]}),
        json!({"background":shown,"beats":lengths(d),"error":d.app().error}),
    )?;
    d.capture("Video role delete shows the background")?;
    undo(d)?;

    // :delete role= acts once; a range across a cut is refused.
    d.command("select role=linked")?;
    at(d, 36)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num8, Key::L])?;
    let before = d.revision();
    d.command("delete role=audio")?;
    d.wait_for("The refused role delete reply", |app| {
        !app.macros.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "A role-only delete across a cut is refused with guidance and no edit",
        d.revision() == before
            && [d.app().error.as_deref(), d.app().message.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| text.contains("inside one beat")),
        json!("refused"),
        json!({"error":d.app().error,"message":d.app().message}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    at(d, 41)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num4, Key::L])?;
    let before = d.revision();
    d.command("delete role=video")?;
    d.changed(&before)?;
    d.settled()?;
    let shown = backgrounds(d, &[40, 41, 44, 45])?;
    d.check(
        ":delete role=video blanks one range once, with the linked role still selected",
        shown == [false, true, true, false]
            && lengths(d) == [40, 50]
            && d.app().edit_role == deadpan_core::MediaRole::Linked,
        json!([false, true, true, false]),
        json!({"background":shown,"beats":lengths(d)}),
    )?;
    undo(d)?;
    role_repeats(d)
}

/// Root sound events, where audio-only repeats live.
fn sound_count(d: &Driver<'_>) -> usize {
    d.app()
        .workspace
        .as_ref()
        .map_or(0, |workspace| workspace.document.sounds().len())
}

/// §6.5 audio-only and video-only repeats: no time is added.
fn role_repeats(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 41)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num6, Key::L])?;
    edit(d, "repeat 3 role=audio")?;
    let muted = d.app().workspace.as_ref().and_then(|workspace| {
        let second = d.app().beat_rows.get(1)?;
        workspace.document.nodes()[&second.id]
            .audio_treatments
            .clip_gain()
            .map(|clip| {
                clip.mute_ranges()
                    .iter()
                    .map(|range| (range.start().floor() as i64, range.end().floor() as i64))
                    .collect::<Vec<_>>()
            })
    });
    d.check(
        ":repeat 3 role=audio places two later plays of the range's sound and mutes the beat under them, adding no time",
        sound_count(d) == 2
            && muted == Some(vec![(7, 19)])
            && lengths(d) == [40, 50]
            && d.app().sequence_length() == 90,
        json!({"sounds":2,"mute":[[7, 19]],"frames":90}),
        json!({"sounds":sound_count(d),"mute":muted,"beats":lengths(d),"error":d.app().error}),
    )?;
    d.capture("Audio-only repeat as placed sounds")?;
    // Copying the muted beat would leave its repeats behind: refused.
    at(d, 41)?;
    let before = d.revision();
    d.chord(&[Key::Y, Key::Y])?;
    d.wait_for("The refused copy reply", |app| {
        !app.service.is_busy() && !app.copied.is_pending() && !app.macros.is_pending()
    })?;
    d.settled()?;
    d.check(
        "Copying the beat under an audio-only repeat is refused, so no muted copy is made",
        d.revision() == before
            && [d.app().error.as_deref(), d.app().message.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| text.contains("repeated from the Original")),
        json!("refused"),
        json!({"error":d.app().error,"message":d.app().message}),
    )?;
    // Later edits still work and keep the repeats.
    at(d, 44)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "A later Split of the muted beat keeps both repeats",
        sound_count(d) == 2 && lengths(d) == [40, 4, 46],
        json!({"sounds":2,"beats":[40, 4, 46]}),
        json!({"sounds":sound_count(d),"beats":lengths(d),"error":d.app().error}),
    )?;
    undo(d)?;
    undo(d)?;

    d.command("select role=video")?;
    at(d, 41)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num6, Key::L])?;
    let before = d.revision();
    d.key(Key::R)?;
    d.changed(&before)?;
    d.settled()?;
    let shown = pictures(d, &[46, 47, 52, 53])?;
    d.check(
        "With the video role, Visual r loops the range's pictures once more over the next frames while the sound continues",
        shown == [76, 71, 76, 83] && sound_count(d) == 0 && lengths(d) == [40, 50],
        json!({"pictures":[76, 71, 76, 83],"sounds":0}),
        json!({"pictures":shown,"sounds":sound_count(d),"error":d.app().error}),
    )?;
    undo(d)?;
    d.command("select role=linked")?;

    // Past the beat's end: refused unless trimmed there.
    at(d, 85)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num4, Key::L])?;
    let before = d.revision();
    d.command("repeat 3 role=audio")?;
    d.wait_for("The refused role repeat reply", |app| {
        !app.macros.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "A role repeat past the beat's end is refused and names overflow=trim",
        d.revision() == before
            && [d.app().error.as_deref(), d.app().message.as_deref()]
                .into_iter()
                .flatten()
                .any(|text| text.contains("overflow=trim")),
        json!("refused"),
        json!({"error":d.app().error,"message":d.app().message}),
    )?;
    d.key(Key::Escape)?;
    at(d, 85)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num4, Key::L])?;
    edit(d, "repeat 3 role=audio overflow=trim")?;
    d.check(
        "overflow=trim keeps only the part of the repeats inside the beat",
        sound_count(d) == 1 && d.app().sequence_length() == 90,
        json!({"sounds":1,"frames":90}),
        json!({"sounds":sound_count(d),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d)
}
