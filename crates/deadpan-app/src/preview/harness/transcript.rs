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
    d.events(
        "Type a transcript search",
        vec![egui::Event::Text("gam".into())],
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
