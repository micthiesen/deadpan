//! Production slice keys, genuine preview pictures and exact reversible commits.

use super::*;
use crate::project::splice::{Prepared, ProposalUpdate};
use deadpan_core::{AudioSample, ProjectFrame};
use deadpan_playback::{ContentIdentity, Phase, Update};
use egui::{Key, Modifiers};

const APPLY: &str = "Place slice · Enter";
const CANCEL: &str = "Cancel · Esc";
const HEADING: &str = "Place slice keyboard controls";
const INTERIOR: &str = "This destination is inside a beat. Use j/k to choose a Sequence seam; no snapping or edit has occurred.";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Place slice covers linked Original copies at exact ordinary Sequence seams. Interior-frame insertion, edited-slice move/copy, replacement, separate picture/audio placement and Repeat/Retime occurrence targeting remain outside this increment.".into());
    d.report.skipped.push("The replay uses genuine source qualification, endpoint decoding, SDR GPU pictures, proposed documents and durable commands. Audio delivery and a concurrent service Undo are explicitly injected. It does not open an audio device or establish acoustic quality.".into());
    let full_original = document(d)?.nodes().clone();
    let baseline = d.revision();
    d.chord(&[Key::G, Key::G, Key::Num6, Key::Num0, Key::L, Key::S])?;
    d.changed(&baseline)?;
    d.chord(&[Key::G, Key::G, Key::Num6, Key::Num1, Key::L])?;
    d.settled()?;
    let entry_nodes = document(d)?.nodes().clone();
    let entry_revision = d.revision();
    let entry_duration = d.app().sequence_length();
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
        Key::Y,
    ])?;
    d.settled()?;
    d.check(
        "Production v and y copy the exact half-open Original range without an edit",
        copied(d) == Some(10..24) && d.revision() == entry_revision,
        json!({"copied":[10,24],"revision":entry_revision}),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.settled()?;
    saved_destination_button(d)?;
    let entry = editor(d);
    let entry_pane = d.app().pane;
    d.command("splice")?;
    d.wait_for(
        "Place slice opens at the captured interior destination",
        |app| app.splice.is_some(),
    )?;
    wait_endpoints(d, 10, 24)?;
    paint_text(d, INTERIOR)?;
    d.key(Key::Enter)?;
    d.check(
        "An interior destination stays explicit and cannot silently snap or commit",
        d.rect(APPLY).is_err() && d.revision() == entry_revision && editor(d) == entry,
        json!({"apply_enabled":false,"editor":entry}),
        d.snapshot(),
    )?;
    d.key(Key::K)?;
    wait_ready(d)?;
    let first = prepared(d)?;
    let first_id = draft(d)?.proposal_for_check().id.clone();
    d.check(
        "Previous seam chooses Edit 60 and prepares one unsaved linked Source",
        first.range.start() == ProjectFrame(60)
            && first.range.end() == ProjectFrame(74)
            && first.plan.duration().frames() == entry_duration as i64 + 14
            && document(d)?.nodes() == &entry_nodes
            && editor(d) == entry,
        json!({"range":[60,74],"saved_unchanged":true}),
        state(d),
    )?;

    d.chord(&[Key::I, Key::Num2])?;
    paint_text(d, "COUNT 2")?;
    d.key(Key::L)?;
    wait_ready(d)?;
    wait_picture(d, 12, "Showing source frame 13")?;
    d.chord(&[Key::O, Key::Num3, Key::L])?;
    wait_ready(d)?;
    wait_endpoints(d, 12, 27)?;
    wait_picture(d, 26, "Showing source frame 27")?;
    d.check(
        "Counted i/o refinement is local and the viewer follows the included endpoint",
        draft(d)?.proposal_for_check().ordinals == (12..27)
            && copied(d) == Some(10..24)
            && d.revision() == entry_revision
            && editor(d) == entry,
        json!({"proposal":[12,27],"copied":[10,24],"displayed_source_ordinal":26}),
        d.snapshot(),
    )?;
    d.chord(&[Key::D, Key::J])?;
    wait_ready(d)?;
    d.check(
        "Next seam chooses the exact group end",
        prepared(d)?.range.start() == ProjectFrame(entry_duration as i64),
        json!(entry_duration),
        state(d),
    )?;
    d.key(Key::K)?;
    wait_ready(d)?;
    let chosen = prepared(d)?;
    let chosen_id = draft(d)?.proposal_for_check().id.clone();
    d.check(
        "Each refinement receives fresh authored preview identities",
        first.node != chosen.node
            && first.snapshot.document.revision_id() != chosen.snapshot.document.revision_id()
            && first_id.draft == chosen_id.draft
            && first_id.change < chosen_id.change,
        json!("same draft, later change, fresh node and revision"),
        state(d),
    )?;
    inspect_joins(d)?;
    layout(d)?;
    native_enter_and_synthetic_ime(d)?;
    d.key(Key::B)?;
    wait_picture(d, 60, "Showing sequence frame 61")?;
    d.check(
        "Paused Before shows the saved destination without starting audio",
        draft(d)?.before_for_check() && d.app().transport.is_none() && editor(d) == entry,
        json!("Before source 60, unchanged saved cursors"),
        d.snapshot(),
    )?;
    d.key(Key::B)?;
    wait_picture(d, 12, "Showing proposed edit frame 61")?;
    audition(d, &chosen, &entry, &entry_revision)?;
    d.key(Key::Escape)?;
    d.wait_for("Escape closes and abandons the slice proposal", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Escape restores the entry view and leaves the saved edit and copied slice unchanged",
        document(d)?.nodes() == &entry_nodes
            && d.revision() == entry_revision
            && editor(d) == entry
            && d.app().pane == entry_pane
            && copied(d) == Some(10..24),
        json!({"editor":entry,"copied":[10,24],"revision":entry_revision}),
        d.snapshot(),
    )?;

    d.command("splice")?;
    d.key(Key::K)?;
    wait_ready(d)?;
    let commit_proposal = prepared(d)?;
    let commit_id = draft(d)?.proposal_for_check().id.clone();
    d.check(
        "Reopening starts a new draft from the untouched copied register",
        commit_id.draft != chosen_id.draft
            && commit_proposal.node != chosen.node
            && draft(d)?.proposal_for_check().ordinals == (10..24),
        json!({"fresh_draft":true,"copied":[10,24]}),
        state(d),
    )?;
    d.app_mut().receive_splice(
        Some(ProposalUpdate {
            id: chosen_id,
            result: Ok(chosen),
        }),
        None,
    );
    d.step(
        "Deliver an obsolete proposal after opening a new placement",
        true,
    )?;
    d.check(
        "A late proposal cannot replace the newly captured placement",
        draft(d)?.proposal_for_check().id == commit_id
            && prepared(d)?.snapshot.document.revision_id()
                == commit_proposal.snapshot.document.revision_id(),
        json!(commit_proposal.snapshot.document.revision_id()),
        state(d),
    )?;
    expect_focus(d, HEADING)?;
    d.key(Key::Enter)?;
    d.changed(&entry_revision)?;
    let committed_revision = d.revision();
    d.check("Enter commits the exact preview once and selects its inserted Source",
        d.app().splice.is_none() && *document(d)? == *commit_proposal.snapshot.document
            && d.app().selected_beat.as_ref() == Some(&commit_proposal.node)
            && d.app().sequence_cursor == 60,
        json!({"revision":commit_proposal.snapshot.document.revision_id(),"node":commit_proposal.node,"cursor":60}), d.snapshot())?;
    d.capture("Committed linked slice selected at its exact destination")?;
    d.key(Key::U)?;
    d.changed(&committed_revision)?;
    d.check(
        "One undo removes the whole placement and preserves the earlier structural split",
        document(d)?.nodes() == &entry_nodes && d.app().sequence_length() == entry_duration,
        json!("exact pre-placement nodes and duration"),
        d.snapshot(),
    )?;
    stale_revision(d, &full_original)?;
    Ok(())
}

fn saved_destination_button(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let destination = d.app().sequence_cursor;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    d.click("Play edit  ·  Space")?;
    let start_sample = d
        .app()
        .transport
        .as_ref()
        .ok_or("Edit playback did not start")?
        .sample;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed
        .restart(start_sample.0)
        .map_err(|error| error.to_string())?;
    let initial = delivery(d, Phase::Playing, start_sample, generation)?;
    inject(
        d,
        initial,
        "Start simulated edit delivery at the retained cursor",
    )?;
    d.settled()?;
    let old_display = d.app().presentation.diagnostic_snapshot()["displayed"].clone();

    let advanced = destination
        .checked_add(2)
        .ok_or("Test playback destination overflow")?;
    let target_label = format!("Showing sequence frame {}", advanced + 1);
    let sample = d
        .app()
        .transport
        .as_ref()
        .ok_or("Simulated edit playback stopped")?
        .domain()
        .sample_at_boundary(advanced)?;
    d.app_mut().feedback.hold_preview = true;
    let advancing = delivery(d, Phase::Playing, sample, generation)?;
    inject(
        d,
        advancing,
        "Advance edit delivery while withholding its picture",
    )?;
    d.wait_for("Advancing playback picture is withheld", |app| {
        app.feedback.held_reply.is_some()
    })?;
    let advancing_request = d.app().presentation.diagnostic_snapshot();
    let held_ticket = d
        .app()
        .feedback
        .held_reply
        .as_ref()
        .map(|reply| format!("{:?}", reply.ticket));
    d.check(
        "Playback advances to an interior Edit frame while retaining the last submitted picture",
        d.app().sequence_cursor == advanced
            && advancing_request["requested"]["label"] == target_label
            && advancing_request["requested"]["ticket"] == held_ticket.unwrap_or_default()
            && advancing_request["displayed"] == old_display,
        json!({"destination":advanced,"requested":target_label,"old_picture_retained":true}),
        d.snapshot(),
    )?;

    d.click("Place slice…  :splice")?;
    d.wait_for("Visible Place slice button opens the draft", |app| {
        app.splice.is_some()
    })?;
    paint_text(d, INTERIOR)?;
    let requested = d.app().presentation.diagnostic_snapshot();
    let stale = d
        .app_mut()
        .feedback
        .held_reply
        .take()
        .ok_or("The advancing playback picture was not held")?;
    d.check(
        "Opening Place captures the advancing interior cursor and requests its saved Edit picture without a seam proposal",
        d.app().splice.as_ref().is_some_and(|draft| {
            draft.cursor == advanced
                && draft.prepared_for_check().is_none()
        })
            && d.rect(APPLY).is_err()
            && !d.app().service.is_busy()
            && requested["requested"]["label"] == target_label
            && requested["requested"]["ticket"] != format!("{:?}", stale.ticket),
        json!({"destination":advanced,"requested":target_label,"seam_proposal":false}),
        json!({"draft":state(d),"picture":requested}),
    )?;

    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = Some(stale);
    d.capture(
        "Deliver the cancelled playback picture after Place requested its saved destination",
    )?;
    let after_stale = d.app().presentation.diagnostic_snapshot();
    d.check(
        "The stale playback reply cannot replace the saved destination request",
        after_stale["displayed"] == old_display
            && after_stale["requested"]["label"] == target_label,
        json!({"displayed":old_display,"requested":target_label}),
        after_stale,
    )?;
    d.settled()?;
    let presented = d.app().presentation.diagnostic_snapshot();
    d.check(
        "The captured no-seam destination reaches GPU presentation after the stale reply is rejected",
        d.app().presentation.displayed_label().as_deref() == Some(target_label.as_str())
            && !d.app().presentation.needs_render()
            && presented["displayed"]["label"] == target_label
            && presented["displayed"]["location"] == presented["requested"]["location"]
            && d.app().splice.as_ref().is_some_and(|draft| {
                draft.prepared_for_check().is_none()
            })
            && d.rect(APPLY).is_err()
            && !d.app().service.is_busy()
            && d.app().sequence_cursor == advanced
            && d.revision() == revision,
        json!({"displayed":target_label,"seam_proposal":false,"revision":revision}),
        json!({"picture":presented,"state":state(d)}),
    )?;
    d.key(Key::Escape)?;
    d.wait_for("Cancel the no-seam Place draft", |app| app.splice.is_none())?;
    d.settled()?;
    d.app_mut().feedback.simulate_playback = simulated;
    Ok(())
}

fn inspect_joins(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::F)?;
    wait_picture(d, 12, "Showing proposed edit frame 61")?;
    d.key(Key::H)?;
    wait_picture(d, 59, "Showing proposed edit frame 60")?;
    d.key(Key::L)?;
    wait_picture(d, 12, "Showing proposed edit frame 61")?;
    d.chord(&[Key::Num1, Key::Num4, Key::L])?;
    wait_picture(d, 26, "Showing proposed edit frame 75")?;
    d.key(Key::L)?;
    wait_picture(d, 60, "Showing proposed edit frame 76")?;
    d.chord(&[Key::Num1, Key::Num5, Key::H])?;
    wait_picture(d, 12, "Showing proposed edit frame 61")
}

fn audition(
    d: &mut Driver<'_>,
    proposed: &Prepared,
    entry: &Value,
    revision: &str,
) -> Result<(), String> {
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Slice loop did not start")?;
    let window = *run.window();
    let rate = proposed.snapshot.document.presentation_basis().frame_rate;
    let expected_start = rate
        .audio_boundary(proposed.range.start())
        .map_err(|error| error.to_string())?
        .0
        - d.app().audition_context.lead.0;
    let expected_end = rate
        .audio_boundary(proposed.range.end())
        .map_err(|error| error.to_string())?
        .0
        + d.app().audition_context.follow.0;
    d.check(
        "Shift Space loops genuine proposed content with context around both insertion joins",
        run.content == proposed.snapshot.content
            && run.revision == *proposed.snapshot.document.revision_id()
            && window.looping()
            && window.start() == AudioSample(expected_start)
            && window.end() == AudioSample(expected_end),
        json!({"window":[expected_start,expected_end],"looping":true}),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed.restart(0).map_err(|error| error.to_string())?;
    let playing = delivery(
        d,
        Phase::Playing,
        AudioSample(window.end().0 + 137),
        generation,
    )?;
    inject(
        d,
        playing.clone(),
        "Deliver proposed loop after one complete lap plus 137 samples",
    )?;
    let heard = AudioSample(window.start().0 + 137);
    d.check(
        "Heard loop time wraps without moving either saved cursor or the captured beat",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.content_sample() == Ok(heard))
            && editor(d) == *entry
            && d.revision() == revision,
        json!({"heard_sample":heard.0,"editor":entry}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Space pauses the proposal at its exact heard sample",
        d.app().transport.is_none() && draft(d)?.position == Some(heard) && editor(d) == *entry,
        json!({"paused":true,"sample":heard.0}),
        state(d),
    )?;
    inject(d, playing, "Deliver obsolete loop progress after pause")?;
    d.check(
        "Late delivery cannot restart a paused placement",
        d.app().transport.is_none() && draft(d)?.position == Some(heard) && editor(d) == *entry,
        json!("paused at retained sample"),
        state(d),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Space resumes the same both-join loop at the retained sample",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.window() == &window && run.sample == heard)
            && editor(d) == *entry,
        json!({"sample":heard.0,"looping":true,"window":[window.start().0,window.end().0]}),
        d.snapshot(),
    )?;
    let proposed_update = delivery(
        d,
        Phase::Playing,
        heard,
        feed.restart(0).map_err(|error| error.to_string())?,
    )?;
    inject(
        d,
        proposed_update.clone(),
        "Deliver resumed proposed content",
    )?;
    d.key(Key::B)?;
    let before = d
        .app()
        .transport
        .as_ref()
        .ok_or("Before comparison did not restart")?;
    d.check(
        "Running Before comparison keeps the same destination context and heard position",
        before.content == ContentIdentity::Committed
            && before.sample == heard
            && before.window() == &window
            && editor(d) == *entry,
        json!({"content":"Committed","sample":heard.0,"window":[window.start().0,window.end().0]}),
        d.snapshot(),
    )?;
    let mut stale = proposed_update;
    stale.phase = Phase::Failed;
    stale.error = Some("obsolete proposed delivery failure".into());
    inject(
        d,
        stale,
        "Deliver obsolete Proposed failure while Before is active",
    )?;
    d.check(
        "Old proposed failures cannot stop or relabel Before",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.content == ContentIdentity::Committed)
            && d.app().error.is_none(),
        json!("Before remains active"),
        d.snapshot(),
    )?;
    d.key(Key::B)?;
    d.check(
        "Returning to Proposed retains its exact immutable content and saved editor state",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.content == proposed.snapshot.content && run.sample == heard)
            && editor(d) == *entry
            && d.revision() == revision,
        json!({"sample":heard.0,"saved_revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Space)
}

fn stale_revision(
    d: &mut Driver<'_>,
    full_original: &std::collections::BTreeMap<NodeId, deadpan_core::BeatNode>,
) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    d.command("splice")?;
    wait_ready(d)?;
    let stale = prepared(d)?;
    let stale_id = draft(d)?.proposal_for_check().id.clone();
    let revision = document(d)?.revision_id().clone();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: revision,
    })?;
    d.wait_for(
        "Concurrent service Undo invalidates the captured destination",
        |app| {
            !app.service.is_busy()
                && app
                    .splice
                    .as_ref()
                    .is_some_and(|draft| draft.invalidated_for_check())
        },
    )?;
    let after = d.revision();
    d.app_mut().receive_splice(
        Some(ProposalUpdate {
            id: stale_id,
            result: Ok(stale),
        }),
        None,
    );
    d.key(Key::Enter)?;
    d.check(
        "A changed revision and late ready reply cannot retarget Enter to the new timeline",
        draft(d)?.invalidated_for_check()
            && d.rect(APPLY).is_err()
            && d.revision() == after
            && document(d)?.nodes() == full_original,
        json!({"apply_enabled":false,"revision":after,"full_original":true}),
        d.snapshot(),
    )?;
    d.capture("Changed revision leaves an explicit invalidated placement")?;
    d.key(Key::Escape)?;
    d.wait_for("Cancel releases the invalidated placement", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Cancelling a stale placement preserves the actual current edit and copied Original range",
        d.revision() == after && document(d)?.nodes() == full_original && copied(d) == Some(10..24),
        json!({"revision":after,"copied":[10,24]}),
        d.snapshot(),
    )
}

fn layout(d: &mut Driver<'_>) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(viewport);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing splice replay viewport")?
            .inner_rect = Some(viewport);
        d.step("Paint Place slice at the exact requested viewport", true)?;
        wait_endpoints(d, 12, 27)?;
        wait_picture(d, 12, "Showing proposed edit frame 61")?;
        for label in [
            "Place slice",
            "UNSAVED · Insert · Linked picture + sound",
            "In 12 · i",
            "Out 27 exclusive · o",
            "Destination Edit 60 · d",
            "Loop both joins · Shift Space",
            APPLY,
            CANCEL,
            "First included · Original frame 13",
            "Last included · Original frame 27",
            "PROPOSED · UNSAVED EDIT",
            "Edit boundary 60",
            "PROVISIONAL SLICE",
            "Before destination",
            "Following material",
        ] {
            paint_text(d, label)?;
        }
        endpoint_painted(d, "First included Original picture, frame 13")?;
        endpoint_painted(d, "Last included Original picture, frame 27")?;
        viewer_painted(d)?;
        tab_circuit(d, width, height)?;
        d.capture(&format!(
            "Visible Original endpoints, proposed destination and action at {width} by {height}"
        ))?;
    }
    Ok(())
}

fn tab_circuit(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let revision = d.revision();
    let preview = prepared(d)?.snapshot.document.revision_id().clone();
    let entry = editor(d);
    let controls = [
        "In 12 · i",
        "Out 27 exclusive · o",
        "Destination Edit 60 · d",
        "Inspect picture · f",
        "Previous seam · k",
        "Next seam · j",
        "−1 · h",
        "+1 · l",
        "Proposed · b",
        "Audition · Space",
        "Loop both joins · Shift Space",
        APPLY,
        CANCEL,
    ];
    for (direction, modifiers) in [("Tab", Modifiers::NONE), ("Shift Tab", Modifiers::SHIFT)] {
        d.click(HEADING)?;
        expect_focus(d, HEADING)?;
        let mut circuit = controls.to_vec();
        if modifiers.shift {
            circuit.reverse();
        }
        circuit.push(HEADING);
        for label in circuit {
            d.key_modified(Key::Tab, modifiers)?;
            // Observe native focus on its next paint, without assigning focus
            // or using a pointer to repair a missed traversal boundary.
            d.step("Paint the next native Place slice Tab target", false)?;
            let focused = focused_controls(d);
            let rect = d.rect(label)?;
            d.check(
                "Tab traverses every draft control and wraps through its heading",
                focused.len() == 1 && focused[0] == label
                    && d.harness.ctx.content_rect().contains_rect(rect)
                    && d.app().splice.is_some() && editor(d) == entry
                    && d.revision() == revision
                    && prepared(d)?.snapshot.document.revision_id() == &preview,
                json!({"direction":direction,"focused":label,"viewport":[width,height],"proposal_unchanged":true}),
                json!({"focused":focused,"rect":[rect.left(),rect.top(),rect.right(),rect.bottom()],"state":state(d)}),
            )?;
            if label != HEADING {
                paint_text(d, label)?;
            }
        }
    }
    Ok(())
}

fn native_enter_and_synthetic_ime(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let preview = prepared(d)?.snapshot.document.revision_id().clone();
    let entry = editor(d);
    focus_with_tab(d, "Proposed · b")?;
    d.key(Key::Enter)?;
    d.check(
        "Native Enter activates the focused comparison button without applying the slice",
        d.app().splice.is_some()
            && draft(d)?.before_for_check()
            && d.app().transport.is_none()
            && d.revision() == revision
            && editor(d) == entry
            && prepared(d)?.snapshot.document.revision_id() == &preview,
        json!({"before":true,"saved_revision":revision,"committed":false}),
        d.snapshot(),
    )?;
    expect_focus(d, "Before · b")?;
    d.key(Key::Enter)?;
    d.check(
        "A second native button Enter restores Proposed without entering the heading Apply route",
        !draft(d)?.before_for_check() && d.revision() == revision && editor(d) == entry,
        json!({"before":false,"saved_revision":revision}),
        state(d),
    )?;

    focus_with_tab(d, "Audition · Space")?;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    d.key(Key::Enter)?;
    d.check(
        "Native button Enter starts slice audition once",
        d.app().transport.is_some() && d.revision() == revision,
        json!({"playing":true}),
        state(d),
    )?;
    expect_focus(d, "Pause · Space")?;
    d.key(Key::Space)?;
    d.check(
        "Native button Space pauses slice audition once across layout retries",
        d.app().transport.is_none() && d.revision() == revision,
        json!({"playing":false}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Native button Enter resumes slice audition once",
        d.app().transport.is_some() && d.revision() == revision,
        json!({"playing":true}),
        state(d),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Native button Space pauses the resumed slice audition",
        d.app().transport.is_none() && editor(d) == entry,
        json!({"playing":false,"editor":entry}),
        state(d),
    )?;
    d.app_mut().feedback.simulate_playback = simulated;
    focus_with_tab(d, APPLY)?;
    d.events(
        "Inject synthetic IME Preedit and Enter while the Apply button has focus",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "slice".into(),
                active_range_chars: Some(0..5),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Synthetic composition Enter cannot activate the focused Apply button",
        d.app().ime_composing
            && draft(d)?.ready_for_check()
            && d.revision() == revision
            && editor(d) == entry,
        json!({"synthetic_composition":true,"draft_ready":true,"saved_revision":revision}),
        d.snapshot(),
    )?;
    d.events(
        "Inject synthetic Escape while the prior IME composition remains active",
        vec![
            key_event(Key::Escape, Modifiers::NONE, true),
            key_event(Key::Escape, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Synthetic composition Escape cannot abandon the slice draft",
        d.app().ime_composing
            && draft(d)?.ready_for_check()
            && d.revision() == revision
            && editor(d) == entry,
        json!({"synthetic_composition":true,"draft_ready":true,"saved_revision":revision}),
        d.snapshot(),
    )?;
    d.events(
        "Inject synthetic IME Commit and Enter in the same event batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("slice".into())),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "The synthetic composition completion batch cannot leak Enter into Apply",
        !d.app().ime_composing
            && draft(d)?.ready_for_check()
            && d.revision() == revision
            && editor(d) == entry
            && prepared(d)?.snapshot.document.revision_id() == &preview,
        json!({"synthetic_composition":false,"draft_ready":true,"saved_revision":revision}),
        d.snapshot(),
    )?;
    focus_with_tab(d, HEADING)?;
    d.chord(&[Key::D, Key::F])?;
    wait_picture(d, 12, "Showing proposed edit frame 61")
}

fn focused_controls(d: &Driver<'_>) -> Vec<String> {
    d.harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            (access.is_focused() && !access.is_hidden() && !access.is_disabled())
                .then(|| access.label().map(|label| label.to_string()))
                .flatten()
        })
        .collect()
}

fn expect_focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let focused = focused_controls(d);
    d.check(
        "Native keyboard focus belongs to the expected Place slice control",
        focused.len() == 1 && focused[0] == label,
        json!(label),
        json!(focused),
    )
}

fn focus_with_tab(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    // One complete 14-control circuit is sufficient. A missing target fails
    // rather than being repaired by assigning an egui ID directly.
    for _ in 0..14 {
        if focused_controls(d).as_slice() == [label] {
            return Ok(());
        }
        d.key(Key::Tab)?;
        d.step(
            "Paint keyboard focus while locating the slice control",
            false,
        )?;
    }
    expect_focus(d, label)
}

fn endpoint_painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let image = d.rect(label)?;
    let viewport = d.harness.ctx.content_rect();
    let shapes = &d.harness.output().shapes;
    let meshes = shapes.iter().enumerate().filter_map(|(index, clipped)| {
        let egui::Shape::Mesh(mesh) = &clipped.shape else { return None; };
        let bounds = mesh.calc_bounds();
        if bounds.min.distance(image.min) > 0.5 || bounds.max.distance(image.max) > 0.5 { return None; }
        let opaque_cover = shapes[index + 1..].iter().any(|later| {
            let egui::Shape::Rect(rect) = &later.shape else { return false; };
            rect.fill.is_opaque() && rect.brush.is_none() && rect.blur_width == 0.0
                && rect.rect.shrink(4.0).intersect(later.clip_rect).intersect(bounds).is_positive()
        });
        Some(json!({"texture":format!("{:?}",mesh.texture_id),"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "visible":matches!(mesh.texture_id, egui::TextureId::User(_)) && clipped.clip_rect.contains_rect(bounds) && viewport.contains_rect(bounds) && !opaque_cover,
            "opaque_cover":opaque_cover}))
    }).collect::<Vec<_>>();
    d.check("The labelled endpoint contains a submitted image mesh with an unclipped, unobscured paint area",
        !meshes.is_empty() && meshes.iter().all(|mesh| mesh["visible"] == true),
        json!({"label":label,"visible_image":true}), json!({"label":label,"meshes":meshes}))
}

fn viewer_painted(d: &mut Driver<'_>) -> Result<(), String> {
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Slice viewer has no displayed picture")?;
    // The native view also prints this caption as text. Match its image role,
    // so the caption cannot stand in for the actual picture allocation.
    let images = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            (access.role() == egui::accesskit::Role::Image
                && access.label().as_deref() == Some(label.as_str())
                && !access.is_hidden()
                && access.bounding_box().is_some())
            .then(|| node.rect())
        })
        .collect::<Vec<_>>();
    let [viewer] = images.as_slice() else {
        return Err(format!(
            "Expected one slice picture allocation, found {}",
            images.len()
        ));
    };
    let texture = d
        .app()
        .target
        .as_ref()
        .ok_or("Slice viewer has no GPU texture")?
        .texture;
    let viewport = d.harness.ctx.content_rect();
    let pixels_per_point = d.harness.ctx.pixels_per_point();
    let meshes = d
        .harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| {
            let egui::Shape::Mesh(mesh) = &clipped.shape else {
                return None;
            };
            if mesh.texture_id != texture {
                return None;
            }
            let bounds = mesh.calc_bounds();
            Some(
                json!({"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "visible":scenarios::picture_contains_rect(*viewer,bounds,pixels_per_point)
                && scenarios::picture_contains_rect(clipped.clip_rect,bounds,pixels_per_point)
                && scenarios::picture_contains_rect(viewport,bounds,pixels_per_point)}),
            )
        })
        .collect::<Vec<_>>();
    d.check(
        "The proposed destination picture is actually painted inside its allocated viewer",
        scenarios::picture_contains_rect(viewport, *viewer, pixels_per_point)
            && viewer.height() >= 140.0
            && !meshes.is_empty()
            && meshes.iter().all(|mesh| mesh["visible"] == true),
        json!({"minimum_height":140,"painted":true}),
        json!({"viewer_height":viewer.height(),"meshes":meshes}),
    )
}

fn paint_text(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Place slice text is actually painted inside its clip and viewport",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!({"label":label,"fully_visible":true}),
        json!(paint),
    )
}

fn wait_endpoints(d: &mut Driver<'_>, first: u64, out: u64) -> Result<(), String> {
    let first = format!("First included Original picture, frame {}", first + 1);
    let last = format!("Last included Original picture, frame {out}");
    let deadline = Instant::now() + Duration::from_secs(15);
    while d.rect(&first).is_err() || d.rect(&last).is_err() {
        if Instant::now() >= deadline {
            return Err(format!(
                "Endpoint pictures did not become visible: {}",
                state(d)
            ));
        }
        d.step(
            "Wait for both exact source endpoint pictures to be submitted",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
    Ok(())
}

fn wait_ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Latest exact slice proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .splice
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
    })
}

fn wait_picture(d: &mut Driver<'_>, source: u64, label: &str) -> Result<(), String> {
    d.wait_for(
        "Exact source picture reaches the proposed or endpoint viewer",
        |app| {
            !app.presentation.loading()
                && !app.presentation.needs_render()
                && app.presentation.displayed_source_frame() == Some(SourceFrameId(source))
                && app.presentation.displayed_label().as_deref() == Some(label)
        },
    )?;
    d.check(
        "The displayed picture identity matches the exact inspected join or source endpoint",
        d.app().presentation.displayed_source_frame() == Some(SourceFrameId(source))
            && d.app().presentation.displayed_label().as_deref() == Some(label),
        json!({"source_ordinal":source,"label":label}),
        d.app().presentation.diagnostic_snapshot(),
    )
}

fn delivery(
    d: &Driver<'_>,
    phase: Phase,
    sample: AudioSample,
    generation: deadpan_output::Generation,
) -> Result<Update, String> {
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No slice audition to deliver")?;
    Ok(Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase,
        sample: Some(sample),
        generation: Some(generation),
        error: None,
    })
}

fn inject(d: &mut Driver<'_>, update: Update, label: &str) -> Result<(), String> {
    d.app_mut().feedback.playback_updates.push_back(update);
    d.step(label, true)
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No splice replay project".into())
}

fn draft<'a>(d: &'a Driver<'_>) -> Result<&'a super::super::splice::Draft, String> {
    d.app()
        .splice
        .as_ref()
        .ok_or_else(|| "Place slice is not open".into())
}

fn prepared(d: &Driver<'_>) -> Result<Arc<Prepared>, String> {
    draft(d)?
        .prepared_for_check()
        .cloned()
        .ok_or_else(|| "Slice proposal is not prepared".into())
}

fn copied(d: &Driver<'_>) -> Option<std::ops::Range<u64>> {
    d.app()
        .moment
        .copied
        .as_ref()
        .map(|copy| copy.ordinals.clone())
}

fn editor(d: &Driver<'_>) -> Value {
    let app = d.app();
    json!({"source_cursor":app.source_cursor,"sequence_cursor":app.sequence_cursor,"selected_beat":app.selected_beat,
        "scope":app.sequence_scope.groups(),"duration":app.sequence_length(),"copied":copied(d)})
}

pub(super) fn state(d: &Driver<'_>) -> Value {
    json!(d.app().splice.as_ref().map(|draft| {
        let proposal = draft.proposal_for_check();
        json!({"session":proposal.id.session,"project":proposal.id.project,"base_revision":proposal.id.base_revision,
            "draft":proposal.id.draft,"change":proposal.id.change,"ordinals":proposal.ordinals,"parent":proposal.parent,
            "slot":proposal.index,"ready":draft.ready_for_check(),"before":draft.before_for_check(),"invalidated":draft.invalidated_for_check(),
            "cursor":draft.cursor,"position":draft.position.map(|sample| sample.0),
            "prepared":draft.prepared_for_check().map(|prepared| json!({"revision":prepared.snapshot.document.revision_id(),
                "node":prepared.node,"range":[prepared.range.start().0,prepared.range.end().0],"duration":prepared.plan.duration().frames()}))})
    }))
}
