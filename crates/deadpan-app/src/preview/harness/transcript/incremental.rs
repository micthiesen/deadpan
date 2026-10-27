//! Production partial-rail paint/input with explicitly synthetic range delivery.

use super::*;
use deadpan_cli::transcription::incremental::RangeProgress;

fn has_button(d: &Driver<'_>, label: &str) -> bool {
    d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.role() == egui::accesskit::Role::Button
            && access.label().as_deref() == Some(label)
            && !access.is_hidden()
    })
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Partial transcript delivery is synthetic UI state over the real Original. This checks production Metal paint, focus, keyboard routing and correction authority; the separately retained Whisper/Silero witness proves actual inference/cancellation/cache behavior.".into());
    let revision = d.revision();
    let (_, transcript, index) = synthetic(d)?;
    let corrections = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .corrections
        .clone();
    d.command("source")?;
    d.chord(&[Key::G, Key::G])?;
    let edit_cursor = d.app().sequence_cursor;
    d.app_mut()
        .transcription
        .replay_partial_recognition(Some(RangeProgress {
            transcript: transcript.clone(),
            coverage: std::iter::once(0..28_800).collect(),
            total_samples: 60_800,
            completed_samples: 28_800,
            cached_ranges: 0,
            cache_failures: 0,
        }));
    d.step("Receive incomplete real-source range display", true)?;
    d.settled()?;
    let coverage = scenarios::text_paint_visibility(d, "Analyzed 1.8 / 3.8 s");
    let status = scenarios::text_paint_visibility(d, "Transcribing · 47%");
    let incomplete = d
        .app()
        .workspace
        .as_ref()
        .is_some_and(|workspace| workspace.transcript.is_none());
    let ready = d.app().transcription_ready();
    d.check("Partial words show exact coverage without enabling full analysis or uncovered words",
        incomplete && !ready && has_button(d, "alpha") && has_button(d, "beta ≈")
            && !has_button(d, "gamma") && !coverage.is_empty()
            && coverage.iter().all(|item| item["fully_visible"] == true)
            && !status.is_empty() && status.iter().all(|item| item["fully_visible"] == true)
            && d.revision() == revision,
        json!({"coverage":"1.8 / 3.8 s","covered_words":["alpha","beta ≈"],"uncovered_word":false,"editorial_ready":false}),
        json!({"coverage":coverage,"status":status,"alpha":has_button(d,"alpha"),"beta":has_button(d,"beta ≈"),"gamma":has_button(d,"gamma"),"editorial_ready":ready,"no_full_transcript":incomplete}))?;
    d.capture("Incomplete transcript keeps uncovered words and editorial objects unavailable")?;

    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.step("Resize partial transcript to minimum workspace", true)?;
    scenarios::reveal_original_rail(d, "beta ≈", true)?;
    d.harness.get_by_label("alpha").focus();
    d.step("Focus first completed word", true)?;
    let pane = d.app().pane;
    d.key(Key::Tab)?;
    d.check(
        "Normal Tab retains the specified visible-pane traversal during partial recognition",
        d.app().pane != pane
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(pane_id(d.app().pane))),
        json!({"pane_changes":true,"focused_pane_heading":true}),
        json!({"before":format!("{pane:?}"),"after":format!("{:?}",d.app().pane)}),
    )?;
    // Normal Tab cycles panes (§7.2). Accessibility focus selects a native
    // word button; Enter must activate it rather than the retained pane.
    d.harness.get_by_label("beta ≈").focus();
    d.step("Accessibility focuses the completed approximate word", true)?;
    let focused = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some("beta ≈") && access.is_focused()
    });
    let hit = super::super::sound_placement::control_hits(d, &["beta ≈"]);
    d.check(
        "The completed word retains native focus and its full compact hit target",
        focused && hit[0]["complete_hit"] == true && hit[0]["enabled"] == true,
        json!({"focused":"beta ≈","complete_hit":true}),
        json!({"focused":focused,"controls":hit}),
    )?;
    // A later completed range can prepend words and renumber recognizer
    // segments. The retained word must keep its keyboard identity.
    let mut words = transcript.words().to_vec();
    for word in &mut words {
        word.segment += 1;
    }
    words.insert(
        0,
        deadpan_analysis::Word {
            text: "earlier".into(),
            start_cs: 0,
            end_cs: 5,
            probability: 0.95,
            segment: 0,
        },
    );
    let prepended =
        Transcript::new(transcript.audio(), words).map_err(|error| error.to_string())?;
    d.app_mut()
        .transcription
        .replay_partial_recognition(Some(RangeProgress {
            transcript: prepended,
            coverage: std::iter::once(0..28_800).collect(),
            total_samples: 60_800,
            completed_samples: 28_800,
            cached_ranges: 0,
            cache_failures: 0,
        }));
    d.step(
        "Receive earlier words while a completed word has keyboard focus",
        true,
    )?;
    let retained_focus = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some("beta ≈") && access.is_focused()
    });
    d.check(
        "Prepending completed words and renumbering segments retains the focused word",
        retained_focus && has_button(d, "earlier"),
        json!({"focused":"beta ≈","earlier_word":true}),
        json!({"retained_focus":retained_focus,"earlier_word":has_button(d,"earlier")}),
    )?;
    d.key(Key::Enter)?;
    d.settled()?;
    // Pane focus changes after a jump can alter the rail's available height.
    // Wait only for the production focus-reveal behavior, without wheel input.
    for _ in 0..6 {
        let hit = super::super::sound_placement::control_hits(d, &["beta ≈"]);
        if hit[0]["complete_hit"] == true {
            break;
        }
        d.step(
            "Focused completed word remains revealed after activation",
            false,
        )?;
    }
    let after_hit = super::super::sound_placement::control_hits(d, &["beta ≈"]);
    let after_focus = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some("beta ≈") && access.is_focused()
    });
    d.check(
        "The activated partial word remains fully revealed without pointer scrolling",
        after_focus && after_hit[0]["complete_hit"] == true,
        json!({"focused":true,"complete_hit":true,"additional_wheel_input":false}),
        json!({"focused":after_focus,"controls":after_hit}),
    )?;
    let expected = picture_at(
        &index,
        transcript.seconds(100).map_err(|error| error.to_string())?,
    )
    .ok_or("Completed word outside source picture")? as u64;
    let seconds = deadpan_analysis::picture_seconds(&index, expected as usize)
        .ok_or("Missing selected PTS")?;
    let origin = deadpan_core::ExactRatio::new(
        i128::from(transcript.audio().origin),
        i128::from(transcript.audio().sample_rate),
    )
    .map_err(|error| error.to_string())?;
    let priority = seconds
        .checked_sub(origin)
        .and_then(|seconds| seconds.checked_mul(deadpan_core::ExactRatio::integer(16_000)))
        .map_err(|error| error.to_string())?
        .floor()
        .max(0);
    let priority = u64::try_from(priority).map_err(|error| error.to_string())?;
    d.check("Enter jumps to the exact Original picture and reprioritizes analysis without moving the edit",
        d.app().source_cursor == expected && d.app().sequence_cursor == edit_cursor
            && d.app().transcription.replay_analysis_priority() == priority
            && d.revision() == revision && d.app().transport.is_none(),
        json!({"source_cursor":expected,"edit_cursor":edit_cursor,"analysis_sample":priority,"revision":revision}),
        json!({"source_cursor":d.app().source_cursor,"edit_cursor":d.app().sequence_cursor,"analysis_sample":d.app().transcription.replay_analysis_priority(),"revision":d.revision()}))?;
    d.capture("Completed approximate word focused at minimum size after exact keyboard jump")?;

    let sheet = corrections
        .as_ref()
        .ok_or("Original correction identity missing")?;
    let old_version = sheet.version();
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let changed = workspace.with_corrections(
        Arc::new(crate::project::OriginalCorrections {
            key: sheet.key.clone(),
            stored: sheet.stored.clone(),
            error: Some("replay unreadable corrections at unchanged version".into()),
        }),
        workspace.transcript.clone(),
        workspace.speech_activity.clone(),
    );
    d.app_mut().workspace = Some(Arc::new(changed));
    d.step(
        "Deliver correction error without changing numeric version",
        true,
    )?;
    scenarios::reveal_original_rail(d, "Completed words wait for corrections", false)?;
    let warning = scenarios::text_paint_visibility(d, "Completed words wait for corrections");
    d.check("A same-version correction read error immediately withdraws prepared word buttons",
        !has_button(d,"alpha") && !has_button(d,"beta ≈") && !warning.is_empty()
            && warning.iter().all(|item|item["fully_visible"]==true)
            && d.app().workspace.as_ref().and_then(|workspace|workspace.corrections.as_ref()).is_some_and(|sheet|sheet.version()==old_version)
            && d.revision()==revision,
        json!({"word_buttons":false,"warning_visible":true,"correction_version":old_version,"revision":revision}),
        json!({"alpha":has_button(d,"alpha"),"beta":has_button(d,"beta ≈"),"warning":warning,"revision":d.revision()}))?;
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let restored = workspace.with_corrections(
        Arc::clone(sheet),
        workspace.transcript.clone(),
        workspace.speech_activity.clone(),
    );
    d.app_mut().workspace = Some(Arc::new(restored));
    d.app_mut().transcription.replay_partial_recognition(None);
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.step(
        "Restore ordinary model-offer state after partial delivery replay",
        true,
    )?;
    d.wait_for(
        "Incomplete UI state leaves no complete saved transcript",
        |app| app.transcription.status_name() == "needs_model",
    )?;
    Ok(())
}
