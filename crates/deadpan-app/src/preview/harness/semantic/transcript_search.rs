//! Native complete-phrase selection over an explicitly synthetic annotation.
use super::*;
use deadpan_analysis::{Transcript, Word};
use deadpan_core::{
    RegisterName, RegisterValue, SemanticInstruction, SemanticSelector, SemanticVisualSelection,
};
use deadpan_store::{TranscriptKey, registers::RegisterBank};

const SELECT: &str = r#"select search="needle""#;
// Independent fixture oracle: 30000/1001 fps, video/audio container origin 0.
// [0.30,0.50), [1.00,1.20), [1.70,1.90) seconds, enclosed once in frames.
const MATCHES: [(i64, i64); 3] = [(8, 15), (29, 36), (50, 57)];

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Transcript-search receipts finish", |app| {
        !app.service.is_busy()
            && !app.service.annotation_busy()
            && !app.macros.is_pending()
            && !app.copied.is_pending()
    })?;
    d.settled()
}
fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("No search workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}
fn copied_ranges(d: &Driver<'_>) -> Result<Vec<(i64, i64)>, String> {
    let saved = bank(d)?;
    let Some(RegisterValue::EditedSet { set }) = saved
        .entries
        .get(&RegisterName::unnamed())
        .map(AsRef::as_ref)
    else {
        return Err("Search did not copy one EditedSet".into());
    };
    set.components()
        .iter()
        .map(|slice| match slice.selection() {
            deadpan_core::SliceCaptureSelection::Range { range } => {
                Ok((range.start().0, range.end().0))
            }
            _ => Err("Search copied a non-range component".into()),
        })
        .collect()
}
fn selected(d: &Driver<'_>) -> Result<bool, String> {
    Ok(
        matches!(d.app().capture_visual_selection()?, Some(SemanticVisualSelection::TranscriptSearch { query, ranges })
        if query.as_str()=="needle" && ranges.iter().map(|range|(range.start().0,range.end().0)).collect::<Vec<_>>()==MATCHES),
    )
}
fn save(d: &mut Driver<'_>, key: &TranscriptKey, words: &Transcript) -> Result<(), String> {
    let session = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No search workspace")?
        .session;
    d.wait_for("Search annotation lane is ready", |app| {
        !app.service.annotation_busy()
    })?;
    if !d.app_mut().submit(ProjectRequest::SaveTranscript {
        expected_session: session,
        lifecycle_epoch: None,
        attempt: 0,
        key: key.clone(),
        transcript: Arc::new(words.clone()),
    }) {
        return Err("Search synthetic annotation was not admitted".into());
    }
    d.wait_for("Search annotation is displayed and qualified", |app| {
        !app.service.annotation_busy()
            && app.transcription.status_name() == "ready"
            && app
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.transcript.as_ref())
                .is_some_and(|stored| &stored.transcript == words)
    })?;
    d.settled()
}
fn picture_order(
    d: &Driver<'_>,
    measured: &deadpan_core::SourceFrameIndex,
) -> Result<Vec<u64>, String> {
    let plan =
        deadpan_plan::RenderPlan::compile(document(d)?).map_err(|error| error.to_string())?;
    let first = plan
        .provider_picture(ProjectFrame(0))
        .map_err(|error| error.to_string())?;
    let deadpan_plan::Picture::Source { asset, .. } = first.picture else {
        return Err("Expected Source baseline".into());
    };
    let index = deadpan_core::SourceFrameIndex::new(
        asset,
        measured.time_base(),
        measured.frames().to_vec(),
        measured.terminal_end(),
        measured.terminal_provenance(),
    )
    .map_err(|error| error.to_string())?;
    (0..plan.duration().frames())
        .map(|frame| {
            plan.provider_picture(ProjectFrame(frame))
                .map_err(|error| error.to_string())?
                .picture
                .select_source_frame(&index)
                .map(|picture| picture.identity.0)
                .map_err(|error| error.to_string())
        })
        .collect()
}
fn remaining() -> Vec<u64> {
    (0..120)
        .filter(|frame| {
            !MATCHES
                .iter()
                .any(|(start, end)| *frame >= *start as u64 && *frame < *end as u64)
        })
        .collect()
}
fn check_cut(
    d: &mut Driver<'_>,
    measured: &deadpan_core::SourceFrameIndex,
    label: &str,
) -> Result<(), String> {
    let order = picture_order(d, measured)?;
    d.check(
        label,
        d.app().sequence_length() == 99 && copied_ranges(d)? == MATCHES && order == remaining(),
        json!({"frames":99,"copied":MATCHES,"retained_Original_order":remaining()}),
        json!({"state":state(d),"pictures":order}),
    )
}

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    d.harness.input_mut().screen_rect = Some(rect);
    d.harness
        .input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing search viewport")?
        .inner_rect = Some(rect);
    d.step("Paint transcript selectors at compact size", false)?;
    let baseline = document(d)?.clone();
    let (key, original_words, measured) = super::super::transcript::synthetic(d)?;
    let fixture_clock = measured.time_base()
        == deadpan_core::SourceTimeBase::new(1, 30000).map_err(|error| error.to_string())?
        && measured.frames().len() == 120
        && measured.frames()[0].pts == 0
        && measured
            .frames()
            .iter()
            .enumerate()
            .all(|(ordinal, frame)| frame.pts == ordinal as i64 * 1001)
        && measured.terminal_end() == 120120
        && baseline.presentation_basis().frame_rate
            == deadpan_core::FrameRate::new(30000, 1001).map_err(|error| error.to_string())?
        && original_words.audio().origin == 0
        && original_words.audio().sample_rate == 48000;
    d.check("Replay starts with the qualified zero-origin 120-picture 30000/1001 fixture",
        fixture_clock && d.app().sequence_length()==120 && !d.app().workspace.as_ref().unwrap().can_undo
            && picture_order(d,&measured)?==(0..120).collect::<Vec<_>>(),
        json!({"pictures":120,"video_time_base":[1,30000],"terminal":120120,"audio_origin":0,"audio_rate":48000}),state(d))?;
    let words = Transcript::new(
        original_words.audio(),
        [30, 100, 170]
            .into_iter()
            .map(|start_cs| Word {
                text: "needle".into(),
                start_cs,
                end_cs: start_cs + 20,
                probability: 1.0,
                segment: 0,
            })
            .collect(),
    )
    .map_err(|error| error.to_string())?;
    save(d, &key, &words)?;
    at(d, 0)?;
    let before_bank = bank(d)?;
    d.command(SELECT)?;
    idle(d)?;
    d.check(
        "Native JSON search retains its exact query and three separate literal Edit intervals",
        selected(d)?
            && document(d)? == &baseline
            && bank(d)? == before_bank
            && d.app().selected_edit_range().is_none(),
        json!({"query":"needle","ranges":MATCHES,"selected_frames":21,"hull":false}),
        state(d),
    )?;
    let inspector = d
        .app()
        .inspector_description()
        .ok_or("Missing search inspector")?;
    d.check(
        "Compact search inspector model exposes the query and every separate range",
        inspector.kind == "Transcript search"
            && inspector.parameter.is_none()
            && inspector
                .fields
                .iter()
                .any(|(label, value)| *label == "Transcript query" && value == "\"needle\"")
            && inspector
                .fields
                .iter()
                .filter(|(label, _)| *label == "Selected range")
                .count()
                == 3,
        json!({"query":"needle","ranges":MATCHES,"parameter":null}),
        json!(inspector.fields),
    )?;
    d.capture("Transcript query needle with three separate highlights and exact ranges")?;
    let long_query = format!("{}needle", "!".repeat(512));
    d.command(&format!(
        "select search={}",
        serde_json::to_string(&long_query).map_err(|error| error.to_string())?
    ))?;
    idle(d)?;
    let long_label = d
        .app()
        .edit_range_label()
        .ok_or("Missing long-query status")?;
    let long_inspector = d
        .app()
        .inspector_description()
        .ok_or("Missing long-query inspector")?;
    d.check("Long accepted raw query retains exact inspector intent while the compact footer stays bounded",
        matches!(d.app().capture_visual_selection()?, Some(SemanticVisualSelection::TranscriptSearch { query, ranges })
            if query.as_str()==long_query && ranges.iter().map(|range|(range.start().0,range.end().0)).collect::<Vec<_>>()==MATCHES)
            && long_label.len()<250 && long_label.contains('…')
            && long_inspector.fields.iter().any(|(label,value)|*label=="Transcript query" && value==&serde_json::to_string(&long_query).unwrap()),
        json!({"query_bytes":long_query.len(),"exact_inspector":true,"footer_abbreviated":true}),json!({"footer":long_label}))?;
    d.capture("Long raw transcript query keeps the compact picture and bounded status")?;
    d.command(SELECT)?;
    idle(d)?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Yank stores the three historical components as one 21-frame union",
        copied_ranges(d)? == MATCHES && document(d)? == &baseline,
        json!({"copied_ranges":MATCHES,"document_write":false}),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    idle(d)?;
    check_cut(
        d,
        &measured,
        "One native cut removes only the matching pictures and intervening gaps stay intact",
    )?;
    let cut = document(d)?.clone();
    undo(d, &baseline)?;
    idle(d)?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "One Redo restores the complete atomic search cut",
        same_document(document(d)?, &cut)?,
        json!({"frames":99}),
        state(d),
    )?;
    undo(d, &baseline)?;
    idle(d)?;

    at(d, 0)?;
    d.command("record w")?;
    d.command(SELECT)?;
    idle(d)?;
    d.key(Key::Y)?;
    idle(d)?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let saved = bank(d)?;
    let macro_saved = matches!(saved.entries.get(&RegisterName::new('w').unwrap()).map(AsRef::as_ref),
        Some(RegisterValue::Macro {program}) if matches!(program.instructions(),[
            SemanticInstruction::SelectTranscriptSearch {query},
            SemanticInstruction::Yank {selector:SemanticSelector::TranscriptSearch {query:yank},..},
            SemanticInstruction::Cut {selector:SemanticSelector::TranscriptSearch {query:cut},..}
        ] if query.as_str()=="needle" && yank==query && cut==query));
    d.check(
        "Saved macro retains search intent in Select, Yank and Cut",
        macro_saved,
        json!({"macro":"w","query":"needle","instructions":3}),
        state(d),
    )?;
    check_cut(
        d,
        &measured,
        "Recorded search cuts exactly the literal union",
    )?;
    undo(d, &baseline)?;
    idle(d)?;
    at(d, 90)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    check_cut(
        d,
        &measured,
        "Dot reruns the saved query independently of the current cursor",
    )?;
    undo(d, &baseline)?;
    idle(d)?;
    at(d, 80)?;
    d.key(Key::V)?;
    d.chord(&[Key::Num3, Key::L])?;
    d.key(Key::V)?;
    idle(d)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    let manual_order: Vec<_> = (0..120).filter(|frame| !(80..83).contains(frame)).collect();
    d.check(
        "A new explicit Visual range overrides the saved search for dot",
        d.app().sequence_length() == 117 && picture_order(d, &measured)? == manual_order,
        json!({"manual_range":[80,83],"frames":117}),
        state(d),
    )?;
    undo(d, &baseline)?;
    idle(d)?;
    at(d, 0)?;
    d.command("macro w")?;
    idle(d)?;
    check_cut(
        d,
        &measured,
        "Named macro re-resolves its stored query against the restored edit",
    )?;
    let called = document(d)?.clone();
    // Ordinary p inserts after the selected beat. Select the final retained
    // Source, whose end is the explicit seam at frame 99.
    at(d, 98)?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    let mut pasted = remaining();
    let copied: Vec<u64> = MATCHES
        .iter()
        .flat_map(|(start, end)| (*start as u64)..(*end as u64))
        .collect();
    pasted.splice(99..99, copied);
    d.check(
        "Seam paste inserts the copied union in canonical component order with one Undo",
        d.app().sequence_length() == 120 && picture_order(d, &measured)? == pasted,
        json!({"seam":99,"frames":120,"inserted_ranges":MATCHES}),
        state(d),
    )?;
    d.capture("Copied transcript union pasted in order at an explicit Edit seam")?;
    undo(d, &called)?;
    idle(d)?;
    undo(d, &baseline)?;
    idle(d)?;

    at(d, 10)?;
    let prior_bank = bank(d)?;
    let prior_selection = d.app().capture_visual_selection()?;
    d.command(r#"select search="missing-needle-query""#)?;
    idle(d)?;
    d.check(
        "A missing search refuses without document, bank or selection writes",
        same_document(document(d)?, &baseline)?
            && bank(d)? == prior_bank
            && d.app().capture_visual_selection()? == prior_selection
            && d.app().error.is_some(),
        json!({"missing_query_refused":true,"bank_unchanged":true}),
        state(d),
    )?;
    for context in ["select role=video", "source"] {
        d.command(context)?;
        idle(d)?;
        d.command(SELECT)?;
        idle(d)?;
        d.check(
            "Wrong media role and Original context refuse search selection early",
            same_document(document(d)?, &baseline)?
                && bank(d)? == prior_bank
                && d.app().error.is_some(),
            json!({"context":context,"no_write":true}),
            state(d),
        )?;
        d.command("sequence")?;
        d.command("select role=linked")?;
        idle(d)?;
    }
    at(d, 10)?;
    d.key(Key::Colon)?;
    d.events(
        "Retain the search command text before concurrent navigation",
        vec![Event::Text(SELECT.into())],
    )?;
    d.app_mut().sequence_cursor = 11;
    d.app_mut().request_picture(false);
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "A delayed native Enter cannot retarget search after the captured cursor changes",
        same_document(document(d)?, &baseline)?
            && bank(d)? == prior_bank
            && d.app().sequence_cursor == 11
            && d.app().capture_visual_selection()?.is_none()
            && d.app().error.is_some(),
        json!({"entry_cursor":10,"current_cursor":11,"no_write":true}),
        state(d),
    )?;
    at(d, 10)?;
    d.key(Key::Colon)?;
    d.events(
        "Retain search entry while a new annotation is published",
        vec![Event::Text(SELECT.into())],
    )?;
    let mut changed = words.words().to_vec();
    changed[0].text = "changed".into();
    let changed = Transcript::new(words.audio(), changed).map_err(|error| error.to_string())?;
    save(d, &key, &changed)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check("A changed displayed analysis invalidates a retained command even without an edit revision change",
        same_document(document(d)?,&baseline)? && bank(d)?==prior_bank
            && d.app().capture_visual_selection()?.is_none() && d.app().error.is_some(),
        json!({"analysis_changed":true,"revision_changed":false,"no_write":true}),state(d))?;
    save(d, &key, &words)?;
    at(d, 0)?;
    d.command(SELECT)?;
    idle(d)?;
    d.check(
        "Replay ends with the protected Original and the qualified search visible",
        same_document(document(d)?, &baseline)? && selected(d)? && d.app().sequence_length() == 120,
        json!({"frames":120,"query":"needle","ranges":MATCHES}),
        state(d),
    )?;
    let unchanged_bank = bank(d)?;
    save(d, &key, &changed)?;
    d.check("Replacing words after a finished selection clears its stale highlights without an authored revision",
        same_document(document(d)?, &baseline)? && bank(d)? == unchanged_bank
            && d.app().capture_visual_selection()?.is_none() && !d.app().edit_range.has_set(),
        json!({"analysis_changed":true,"old_selection_cleared":true,"authored_write":false}),state(d))?;
    save(d, &key, &words)?;
    d.command(SELECT)?;
    idle(d)?;
    d.check(
        "Fresh search restores the exact query and literal ranges after analysis replacement",
        selected(d)? && same_document(document(d)?, &baseline)?,
        json!({"query":"needle","ranges":MATCHES}),
        state(d),
    )?;
    d.report.skipped.push("Transcript-search replay stores explicit synthetic words over the real qualified Original through SaveTranscript. It proves native input, exact selection, copy/cut/history and retained-analysis guards; it does not run or qualify speech recognition.".into());
    Ok(())
}
