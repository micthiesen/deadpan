//! Specification §8.1, §8.3, §8.4 and §7.5: One More Time and Nothing
//! Happens gags, `:repeat` gaps, `,m` mute, the `,r` reaction picker,
//! `:audio-lag` and the `:sound-cut` bed drop, each through the production
//! router, project service and store, with one Undo each.

use super::*;
use deadpan_core::{AudioEdgePolicy, HoldAudio, NodeKind, SourceAudioMapping};
use egui::Key;

fn document(d: &Driver<'_>) -> Result<std::sync::Arc<deadpan_core::ProjectDocument>, String> {
    Ok(d.app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone())
}

/// Select the beat over Edit [30, 42) after splitting there.
fn select_split_beat(d: &mut Driver<'_>) -> Result<NodeId, String> {
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    let row = d
        .app()
        .beat_rows
        .iter()
        .find(|row| Some(&row.id) == d.app().selected_beat.as_ref())
        .ok_or("No selected beat")?;
    if (row.start, row.frames) != (30, 12) {
        return Err(format!(
            "expected the beat over [30, 42), selected [{}, {})",
            row.start,
            row.start + row.frames
        ));
    }
    Ok(row.id.clone())
}

fn undo(d: &mut Driver<'_>, duration: u64, label: &str) -> Result<(), String> {
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.check(
        label,
        d.app().sequence_length() == duration,
        json!(duration),
        json!(d.app().sequence_length()),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    // Copy Original pictures 90..96 into register r for the reaction picker
    // and the Nothing Happens room tone.
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::Num9, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num6, Key::L])?;
    d.key_modified(Key::Quote, egui::Modifiers::SHIFT)?;
    d.key(Key::R)?;
    d.key(Key::Y)?;
    d.wait_for("Original moment copied", |app| {
        !app.service.is_busy() && !app.copied.is_pending() && app.copied.original().is_some()
    })?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::S])?;
    d.settled()?;
    d.chord(&[Key::Num1, Key::Num2, Key::L, Key::S])?;
    d.settled()?;
    let duration = d.app().sequence_length();

    // One More Time: three plays, gaps of 12 then 6 frames.
    select_split_beat(d)?;
    let before = d.revision();
    d.command("gag one-more-time plays=3 gap=12f shorten=6f")?;
    d.changed(&before)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let group = d.app().selected_beat.clone().ok_or("No gag group")?;
    let node = &workspace.document.nodes()[&group];
    let gaps = match &node.kind {
        NodeKind::Sequence { children } => {
            children
                .first()
                .and_then(|repeat| match &workspace.document.nodes()[repeat].kind {
                    NodeKind::Repeat {
                        iterations, gap, ..
                    } => {
                        let branch = workspace
                            .document
                            .gap_overrides()
                            .get(repeat)
                            .and_then(|branches| branches.get(&iterations.at(1)?).cloned())?;
                        let branch = match &workspace.document.nodes()[&branch].kind {
                            NodeKind::Hold { recipe } => recipe.duration.frames(),
                            _ => 0,
                        };
                        Some((iterations.len(), gap.as_ref()?.duration.frames(), branch))
                    }
                    _ => None,
                })
        }
        _ => None,
    };
    d.check(
        "One More Time repeats the beat with a shorter held gap each play, as one group",
        node.label == "One More Time · v1 · 3 plays, gap 12f shortening by 6f"
            && gaps == Some((3, 12, 6))
            && d.app().sequence_length() == duration + 24 + 18
            && d.app().message.as_deref() == Some("Applied One More Time."),
        json!({"label":"One More Time · v1 · 3 plays, gap 12f shortening by 6f","plays_gaps":[3,12,6],"frames":duration + 42}),
        json!({"label":node.label,"plays_gaps":gaps,"frames":d.app().sequence_length(),"message":d.app().message,"error":d.app().error}),
    )?;
    d.capture("One More Time gag")?;
    undo(d, duration, "One undo removes the whole One More Time gag")?;

    // :repeat with a gap ladder wraps the plain beat and escalates it.
    select_split_beat(d)?;
    let before = d.revision();
    d.command("repeat 3 gap=6f gap-step=-2f gain-step=3dB")?;
    d.changed(&before)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let repeat = d.app().selected_beat.clone().ok_or("No Repeat")?;
    let wrapped = matches!(
        &workspace.document.nodes()[&repeat].kind,
        NodeKind::Repeat { iterations, gap: Some(gap), escalation: Some(escalation), .. }
            if iterations.len() == 3 && gap.duration.frames() == 6
                && escalation.gain_step.millidecibels() == 3000
    );
    d.check(
        "One :repeat command wraps the beat with a shortening gap and a gain step",
        wrapped && d.app().sequence_length() == duration + 24 + 10,
        json!({"plays":3,"gaps":[6,4],"gain_step_mdb":3000,"frames":duration + 34}),
        json!({"frames":d.app().sequence_length(),"message":d.app().message,"error":d.app().error}),
    )?;
    let gaps = scenarios::text_paint_visibility(d, "6 f · 4 f");
    d.check(
        "The inspector lists each gap between plays",
        !gaps.is_empty() && gaps.iter().all(|paint| paint["fully_visible"] == true),
        json!("6 f · 4 f fully painted"),
        json!(gaps),
    )?;
    d.capture("Repeat with a gap ladder")?;
    // gap= defines every gap: one length resets the ladder, 0 removes all.
    for (command, frames, gap) in [
        ("repeat gap=5f", duration + 24 + 10, Some(5)),
        ("repeat gap=0", duration + 24, None),
    ] {
        let before = d.revision();
        d.command(command)?;
        d.changed(&before)?;
        d.settled()?;
        let workspace = d.app().workspace.clone().ok_or("No project")?;
        let (gaps, branches) = match &workspace.document.nodes()[&repeat].kind {
            NodeKind::Repeat { gap, .. } => (
                gap.as_ref().map(|gap| gap.duration.frames()),
                workspace
                    .document
                    .gap_overrides()
                    .get(&repeat)
                    .map_or(0, |branches| branches.len()),
            ),
            _ => (None, usize::MAX),
        };
        d.check(
            &format!(":{command} replaces the whole gap ladder"),
            gaps == gap && branches == 0 && d.app().sequence_length() == frames,
            json!({"gap":gap,"branches":0,"frames":frames}),
            json!({"gap":gaps,"branches":branches,"frames":d.app().sequence_length(),"message":d.app().message,"error":d.app().error}),
        )?;
    }
    let before = d.revision();
    d.command("repeat gap=0")?;
    d.wait_for("Unchanged gaps settle", |app| !app.service.is_busy())?;
    d.check(
        "Restating the current gaps makes no edit",
        d.revision() == before,
        json!({"revision":before}),
        json!({"revision":d.revision(),"message":d.app().message,"error":d.app().error}),
    )?;
    for _ in 0..2 {
        let applied = d.revision();
        d.key(Key::U)?;
        d.changed(&applied)?;
    }
    undo(
        d,
        duration,
        "One undo removes the wrap, gaps and escalation",
    )?;

    // ,m toggles the beat's mute; with a Visual range it mutes that range.
    let beat = select_split_beat(d)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&before)?;
    let muted = document(d)?.nodes()[&beat]
        .audio_treatments
        .clip_gain()
        .is_some_and(|gain| gain.muted());
    d.check(
        ",m mutes the selected beat without changing timing",
        muted && d.app().sequence_length() == duration,
        json!({"muted":true,"frames":duration}),
        json!({"muted":muted,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, duration, "One undo unmutes the beat")?;
    select_split_beat(d)?;
    d.chord(&[Key::V, Key::Num5, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&before)?;
    let ranges: Vec<(String, String)> = document(d)?.nodes()[&beat]
        .audio_treatments
        .clip_gain()
        .map(|gain| {
            gain.mute_ranges()
                .iter()
                .map(|range| (format!("{:?}", range.start()), format!("{:?}", range.end())))
                .collect()
        })
        .unwrap_or_default();
    let expected = vec![(
        format!("{:?}", deadpan_core::ExactRatio::integer(0)),
        format!("{:?}", deadpan_core::ExactRatio::integer(5)),
    )];
    d.check(
        ",m over a Visual range inside the beat adds exactly that mute range",
        ranges == expected,
        json!(expected),
        json!({"ranges":ranges,"error":d.app().error}),
    )?;
    d.capture("Mute range from ,m")?;
    d.key(Key::Escape)?;
    undo(d, duration, "One undo removes the mute range")?;

    // ,r opens the cutaway command on a register holding an Original moment.
    select_split_beat(d)?;
    d.chord(&[Key::Comma, Key::R])?;
    d.step("Paint the reaction picker", true)?;
    let hint = scenarios::text_paint_visibility(d, "r [90..96)");
    d.check(
        ",r opens :cutaway on an Original register and lists the available moments",
        d.app().command_open
            && d.app().command.starts_with("cutaway register=")
            && !hint.is_empty(),
        json!({"command":"cutaway register=…","hint":"r [90..96)"}),
        json!({"command":d.app().command,"open":d.app().command_open,"hint":hint}),
    )?;
    d.capture("Reaction picker")?;
    let before = d.revision();
    d.events(
        "Replace the register and apply the reaction cutaway",
        vec![
            key_event(Key::Backspace, egui::Modifiers::NONE, true),
            key_event(Key::Backspace, egui::Modifiers::NONE, false),
            egui::Event::Text("r".into()),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    d.changed(&before)?;
    let cutaways: usize = document(d)?
        .nodes()
        .values()
        .map(|node| node.cutaways.len())
        .sum();
    d.check(
        "Enter shows register r over the selected beat with timing unchanged",
        cutaways == 1 && d.app().sequence_length() == duration,
        json!({"cutaways":1,"frames":duration}),
        json!({"cutaways":cutaways,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    undo(d, duration, "One undo removes the reaction cutaway")?;

    // Nothing Happens: room tone from register r, then true silence.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("gag nothing-happens register=r tone=12f silence=9f")?;
    d.changed(&before)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let group = d.app().selected_beat.clone().ok_or("No gag group")?;
    let parts: Vec<(i64, &'static str)> = match &workspace.document.nodes()[&group].kind {
        NodeKind::Sequence { children } => children
            .iter()
            .filter_map(|child| match &workspace.document.nodes()[child].kind {
                NodeKind::Hold { recipe } => Some((
                    recipe.duration.frames(),
                    match recipe.audio {
                        HoldAudio::RoomTone { .. } => "room tone",
                        HoldAudio::Silence => "silence",
                        HoldAudio::Tail { .. } => "tail",
                        HoldAudio::Reverse { .. } => "reverse",
                        HoldAudio::Tone { .. } => "tone",
                    },
                )),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    d.check(
        "Nothing Happens holds the picture with room tone, then true silence, as one group",
        parts == [(12, "room tone"), (9, "silence")]
            && d.app().sequence_length() == duration + 21,
        json!({"parts":[[12,"room tone"],[9,"silence"]],"frames":duration + 21}),
        json!({"parts":parts,"frames":d.app().sequence_length(),"message":d.app().message,"error":d.app().error}),
    )?;
    d.capture("Nothing Happens gag")?;
    undo(
        d,
        duration,
        "One undo removes the whole Nothing Happens gag",
    )?;

    // :audio-lag offsets the selected beat's sound from its picture.
    let beat = select_split_beat(d)?;
    let before = d.revision();
    d.command("audio-lag +50ms")?;
    d.changed(&before)?;
    d.settled()?;
    let lagged = document(d)?;
    let (host, _) = deadpan_core::cutaway_host(&lagged, &beat).ok_or("No Source host")?;
    let offset = match &lagged.nodes()[&host].kind {
        NodeKind::Source { source } => Some(source.audio_offset.0),
        _ => None,
    };
    let link = scenarios::text_paint_visibility(d, "50.0 ms late");
    d.check(
        ":audio-lag plays the beat's sound 2,400 samples late with the picture and timing unchanged",
        offset == Some(2_400)
            && d.app().sequence_length() == duration
            && d
                .app()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("Slip and Trim no longer apply"))
            && !link.is_empty()
            && link.iter().all(|paint| paint["fully_visible"] == true),
        json!({"offset":2400,"frames":duration,"inspector":"Sound offset 50.0 ms late","message":"… Slip and Trim no longer apply …"}),
        json!({"offset":offset,"frames":d.app().sequence_length(),"inspector":link,"message":d.app().message,"error":d.app().error}),
    )?;
    d.capture("Audio lag")?;
    undo(d, duration, "One undo realigns the sound")?;

    // An off-center stare saved as a framing preset applies to another beat.
    let stare = select_split_beat(d)?;
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens", |app| app.camera.is_some())?;
    d.chord(&[Key::Plus, Key::Plus, Key::L, Key::L, Key::K])?;
    let before = d.revision();
    d.click("Apply  Enter")?;
    d.changed(&before)?;
    let framing = document(d)?.nodes()[&stare].framing.clone();
    let off_center = format!("{framing:?}");
    d.command("framing-save s")?;
    d.wait_for("Framing preset saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let first = d.app().selected_beat.clone().ok_or("No first beat")?;
    let before = d.revision();
    d.command("macro s")?;
    d.changed(&before)?;
    let applied = document(d)?.nodes()[&first].framing.clone();
    d.check(
        ":framing-save keeps an off-center stare that :macro applies to another beat",
        framing.is_some()
            && applied == framing
            && first != stare
            && d.app().sequence_length() == duration,
        json!({"framing":off_center,"applied_to_other_beat":true,"frames":duration}),
        json!({"applied":format!("{applied:?}"),"saved":off_center,"frames":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    d.capture("Framing preset applied")?;
    let applied_revision = d.revision();
    d.key(Key::U)?;
    d.changed(&applied_revision)?;
    let applied_revision = d.revision();
    d.key(Key::U)?;
    d.changed(&applied_revision)?;

    // :sound-cut ends a placed sound at the Edit cursor (bed drop).
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    scenarios::choose_sound_interpretation(
        d,
        Some(crate::project::AudioLayoutInterpretation::StereoLeftRight),
    )?;
    d.click("Add sound…  ⌘I")?;
    d.wait_for("Sound is qualified in the real catalog", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::L])?;
    d.settled()?;
    let catalog = d.app().sound_rows[0].clone();
    d.click(&catalog.1)?;
    d.step("Paint controls for the selected catalog sound", true)?;
    let before = d.revision();
    d.click("Place at edit cursor  ,s")?;
    d.changed(&before)?;
    let id = d.app().selected_event.clone().ok_or("No placed sound")?;
    // The sound runs about five frames from frame 2; cut it at frame 4.
    d.click("Current group beat outline pane")?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::L])?;
    d.settled()?;
    d.command("sounds")?;
    d.settled()?;
    let before = d.revision();
    d.command("sound-cut")?;
    d.changed(&before)?;
    let event = document_event(d, &id)?;
    // The onset is frame 2's sample boundary (3,203 of 3,203.2 samples), so
    // the absolute selected end, not the local one, is exactly frame 4.
    let rate = document(d)?.presentation_basis().frame_rate;
    let cut = match event.mapping {
        SourceAudioMapping::SelectedPlacement { .. } => event
            .mapping
            .selection_frames_with_offset(deadpan_core::FrameDuration::ZERO, event.offset, rate)
            .ok()
            .map(|selection| format!("{:?}", selection.end)),
        _ => None,
    };
    let expected = format!("{:?}", deadpan_core::ExactRatio::integer(4));
    d.check(
        ":sound-cut ends the placed sound at the Edit cursor with a hard edge",
        cut.as_deref() == Some(expected.as_str())
            && event.end_edge == AudioEdgePolicy::Hard
            && d.app().sequence_length() == duration,
        json!({"selection_end_frames":expected,"end_edge":"Hard","frames":duration}),
        json!({"selection_end_frames":cut,"end_edge":format!("{:?}", event.end_edge),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.capture("Bed drop")?;
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    let restored = document_event(d, &id)?;
    d.check(
        "One undo restores the whole sound",
        matches!(restored.mapping, SourceAudioMapping::Duration { .. })
            && restored.end_edge == AudioEdgePolicy::Automatic,
        json!("natural-rate mapping, automatic end"),
        json!(format!("{:?}", restored.mapping)),
    )?;
    Ok(())
}

fn document_event(
    d: &Driver<'_>,
    id: &deadpan_core::SoundId,
) -> Result<deadpan_core::SoundEvent, String> {
    document(d)?
        .sounds()
        .get(id)
        .cloned()
        .ok_or_else(|| "The placed sound is missing".into())
}
