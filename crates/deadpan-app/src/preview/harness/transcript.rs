//! The Original transcript rail: model prompt, stored words, search and jumps.
//!
//! Replay never downloads a model. It checks the install prompt, then saves a
//! synthetic transcript of the fixture's audio through the real project
//! service and drives search, Enter and word clicks against exact frames.

use super::*;
use deadpan_analysis::{AnalysedAudio, Transcript, Word, picture_at};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Transcript status checked", |app| {
        app.transcription.status_name() != "unchecked"
    })?;
    let prompt = scenarios::text_paint_visibility(d, "Install model…");
    d.check(
        "Without an installed pack the rail offers the model with its size and license",
        d.app().transcription.status_name() == "needs_model"
            && !prompt.is_empty()
            && !scenarios::text_paint_visibility(d, "MIT license").is_empty(),
        json!({"status":"needs_model","prompt":"Install model…"}),
        json!({"status":d.app().transcription.status_name(),"prompt":prompt}),
    )?;
    let revision = d.revision();
    let (key, transcript, index) = synthetic(d)?;
    let session = d.app().workspace.as_ref().ok_or("No project")?.session;
    let submitted = d.app_mut().submit(ProjectRequest::SaveTranscript {
        expected_session: session,
        attempt: 0,
        key,
        transcript: Arc::new(transcript.clone()),
    });
    d.wait_for("Transcript saved and shown", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
            && app.transcription.status_name() == "ready"
    })?;
    d.command("source")?;
    d.settled()?;
    let sentence = scenarios::text_paint_visibility(d, "alpha beta");
    d.check(
        "A saved transcript appears in the rail without changing the edit",
        submitted
            && d.revision() == revision
            && !sentence.is_empty()
            && sentence.iter().all(|paint| paint["fully_visible"] == true),
        json!({"revision":revision,"sentence":"alpha beta"}),
        json!({"revision":d.revision(),"sentence":sentence}),
    )?;
    d.capture("Transcript words in the Original rail")?;

    let expected = |word: usize| -> Result<u64, String> {
        let seconds = transcript
            .seconds(transcript.words()[word].start_cs)
            .map_err(|e| e.to_string())?;
        picture_at(&index, seconds)
            .map(|frame| frame as u64)
            .ok_or_else(|| "synthetic word lies outside the picture".to_owned())
    };
    let search = d.rect("transcript-search").ok();
    let field = d
        .harness
        .root()
        .children_recursive()
        .find(|node| {
            node.accesskit_node().role() == egui::accesskit::Role::TextInput
                && node.accesskit_node().placeholder() == Some("Find words")
        })
        .map(|node| node.rect())
        .or(search)
        .ok_or("Missing transcript search field")?;
    d.click_at("Transcript search field", field.center())?;
    // Real key presses with their text: editor bindings such as `g` must
    // not see keys typed into the field.
    let cursor = d.app().source_cursor;
    let typed = ["g", "a", "m"]
        .into_iter()
        .zip([Key::G, Key::A, Key::M])
        .flat_map(|(text, key)| {
            [
                egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Text(text.into()),
                egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        })
        .collect();
    d.events("Type a transcript search", typed)?;
    d.check(
        "Keys typed into Find words reach the field, not editor bindings",
        d.app().transcription.search_text() == "gam"
            && d.app().bindings.pending().is_empty()
            && d.app().source_cursor == cursor,
        json!({"search":"gam","pending":"","source_cursor":cursor}),
        json!({"search":d.app().transcription.search_text(),"pending":format!("{:?}",d.app().bindings.pending()),"source_cursor":d.app().source_cursor}),
    )?;
    d.key(Key::Enter)?;
    d.settled()?;
    let gamma = expected(2)?;
    d.check(
        "Enter jumps the Original cursor to the first match's exact picture",
        d.app().view == View::Source
            && d.app().source_cursor == gamma
            && d.app().current_word() == Some(2)
            && d.revision() == revision,
        json!({"source_cursor":gamma,"current_word":2}),
        json!({"source_cursor":d.app().source_cursor,"current_word":d.app().current_word(),"view":format!("{:?}",d.app().view)}),
    )?;
    d.capture("Search match and current word")?;

    let text = d
        .harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text().starts_with("alpha beta") => {
                Some(text.visual_bounding_rect())
            }
            _ => None,
        })
        .ok_or("Missing painted sentence")?;
    d.click_at(
        "First transcript word",
        egui::pos2(text.left() + 4.0, text.center().y),
    )?;
    d.settled()?;
    let alpha = expected(0)?;
    d.check(
        "Clicking a word moves the Original cursor to the picture where it begins",
        d.app().source_cursor == alpha && d.revision() == revision,
        json!({"source_cursor":alpha}),
        json!({"source_cursor":d.app().source_cursor}),
    )?;
    words_in_your_edit(d, &expected)?;
    d.report.skipped.push("Replay saves a synthetic transcript; real recognition is covered by the worker tests and transcription qualification. No model is downloaded.".into());
    Ok(())
}

/// A three-word transcript over the fixture's real audio clock.
fn synthetic(
    d: &Driver<'_>,
) -> Result<
    (
        deadpan_store::TranscriptKey,
        Transcript,
        deadpan_core::SourceFrameIndex,
    ),
    String,
> {
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let asset = original_asset(workspace).ok_or("No Original")?;
    let source = workspace
        .sources
        .get(asset)
        .ok_or("Missing Original source")?;
    let snapshot = source.receipt.snapshot();
    let audio = snapshot.audio().ok_or("Fixture has no audio")?;
    let video = snapshot.video().ok_or("Fixture has no picture")?;
    let origin = audio
        .frames()
        .first()
        .ok_or("Empty audio index")?
        .valid_start;
    let rate = audio.stream().sample_rate;
    let word = |text: &str, start_cs, end_cs, probability, segment| Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability,
        segment,
    };
    let transcript = Transcript::new(
        AnalysedAudio {
            origin,
            sample_rate: rate,
            duration_cs: 380,
        },
        vec![
            word("alpha", 10, 50, 0.95, 0),
            word("beta", 100, 150, 0.40, 0),
            word("gamma", 200, 260, 0.90, 1),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok((
        deadpan_store::TranscriptKey {
            content: source.receipt.original().content().to_string(),
            audio_stream: audio.stream().stream_index,
            model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
            language: "en".into(),
            engine: "replay".into(),
        },
        transcript,
        video.index().index().clone(),
    ))
}

/// `w` and `diw` in Your edit: motions land on word starts, the cut removes
/// exactly the word's frames, and words follow the edit.
fn words_in_your_edit(
    d: &mut Driver<'_>,
    expected: &dyn Fn(usize) -> Result<u64, String>,
) -> Result<(), String> {
    d.command("sequence")?;
    focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    // The baseline presents Original picture k at project frame k.
    let (alpha, beta) = (expected(0)?, expected(1)?);
    d.key(Key::W)?;
    d.settled()?;
    let first = d.app().sequence_cursor;
    d.key(Key::W)?;
    d.settled()?;
    d.check(
        "w moves the Edit cursor to each word's first picture",
        first == alpha && d.app().sequence_cursor == beta,
        json!({"first":alpha,"second":beta}),
        json!({"first":first,"second":d.app().sequence_cursor,"message":d.app().message}),
    )?;
    let before = d.app_mut().edit_speech()?;
    let word = before
        .runs()
        .iter()
        .find(|run| run.word == 1)
        .copied()
        .ok_or("beta has no run in Your edit")?;
    let duration = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .plan
        .duration()
        .frames();
    let revision = d.revision();
    d.chord(&[Key::D, Key::I, Key::W])?;
    d.changed(&revision)?;
    d.settled()?;
    let after = d.app_mut().edit_speech()?;
    let remaining: Vec<u32> = after.runs().iter().map(|run| run.word).collect();
    let shortened = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .plan
        .duration()
        .frames();
    d.check(
        "diw cuts exactly the word's frames and the transcript follows the edit",
        duration - shortened == word.range.duration().frames()
            && remaining == [0, 2]
            && after.runs()[1].range.start().0
                == before.runs()[2].range.start().0 - word.range.duration().frames(),
        json!({"removed":word.range.duration().frames(),"words":[0,2]}),
        json!({"removed":duration - shortened,"words":remaining}),
    )?;
    let alpha_paint = scenarios::text_paint_visibility(d, "alpha");
    let beta_paint = scenarios::text_paint_visibility(d, "beta");
    let caption = scenarios::text_paint_visibility(d, "Your edit · 2 of 3 words kept");
    d.check(
        "In Your edit the rail lists the edit's words, without the cut one",
        !alpha_paint.is_empty() && beta_paint.is_empty() && !caption.is_empty(),
        json!({"alpha":"painted","beta":"absent","caption":"2 of 3 words kept"}),
        json!({"alpha":alpha_paint,"beta":beta_paint,"caption":caption}),
    )?;
    d.capture("Word cut from Your edit")?;
    let cut = d.revision();
    d.key(Key::U)?;
    d.changed(&cut)?;
    d.settled()?;

    // A split inside a word changes beats, not words.
    d.chord(&[Key::G, Key::G])?;
    d.chord(&[Key::W, Key::W, Key::L, Key::L])?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    d.settled()?;
    let split = d.app_mut().edit_speech()?;
    let runs: Vec<(u32, i64, i64)> = split
        .runs()
        .iter()
        .map(|run| (run.word, run.range.start().0, run.range.end().0))
        .collect();
    let unchanged: Vec<(u32, i64, i64)> = before
        .runs()
        .iter()
        .map(|run| (run.word, run.range.start().0, run.range.end().0))
        .collect();
    d.check(
        "Splitting inside a word keeps one word occurrence",
        runs == unchanged,
        json!(unchanged),
        json!(runs),
    )?;
    let split_revision = d.revision();
    d.key(Key::U)?;
    d.changed(&split_revision)?;
    d.settled()?;

    // The rail search ("gam") steps through its occurrences in Your edit.
    focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    let gamma = d
        .app_mut()
        .edit_speech()?
        .runs()
        .iter()
        .find(|run| run.word == 2)
        .map(|run| run.range.start().0 as u64)
        .ok_or("gamma has no run in Your edit")?;
    d.key(Key::N)?;
    d.settled()?;
    let next = d.app().sequence_cursor;
    d.key_modified(Key::N, egui::Modifiers::SHIFT)?;
    d.settled()?;
    d.check(
        "n and N move the Edit cursor to matching words in Your edit, wrapping",
        next == gamma
            && d.app().sequence_cursor == gamma
            && d.app()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("wrapped")),
        json!({"next":gamma,"previous":gamma,"wrapped":true}),
        json!({"next":next,"previous":d.app().sequence_cursor,"message":d.app().message}),
    )?;
    d.events(
        "Slash focuses Find words",
        vec![
            egui::Event::Key {
                key: Key::Slash,
                physical_key: Some(Key::Slash),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("/".into()),
        ],
    )?;
    d.settled()?;
    let focused = d
        .harness
        .ctx
        .memory(|memory| memory.has_focus(egui::Id::new(TRANSCRIPT_SEARCH_ID)));
    d.check(
        "/ focuses the transcript search without typing a slash",
        focused && d.app().transcription.search_text() == "gam",
        json!({"focused":true,"search":"gam"}),
        json!({"focused":focused,"search":d.app().transcription.search_text()}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    Ok(())
}

fn focus_your_edit(d: &mut Driver<'_>) -> Result<(), String> {
    for _ in 0..6 {
        if d.app().pane == Pane::Sequence
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(pane_id(Pane::Sequence)))
        {
            return Ok(());
        }
        d.key(Key::Tab)?;
    }
    Err("Keyboard focus could not reach Your edit".into())
}
