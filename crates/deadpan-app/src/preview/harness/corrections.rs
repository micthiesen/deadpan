//! Correcting transcript words and pauses through `:correct`.
//!
//! Saves a synthetic transcript and speech activity through the real project
//! service, then drives the modal sheet with real keys: edit text with a
//! space to split a word, move a word edge as an unsaved draft and apply it,
//! undo and redo the correction, and remove a pause. Corrections never create
//! a document revision.

use super::*;
use crate::preview::corrections::Item;
use deadpan_analysis::{ActivityAudio, SpeechActivity};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Transcript status checked", |app| {
        app.transcription.status_name() != "unchecked"
    })?;
    let (key, transcript, _) = super::transcript::synthetic(d)?;
    let session = d.app().workspace.as_ref().ok_or("No project")?.session;
    d.app_mut().submit(ProjectRequest::SaveTranscript {
        expected_session: session,
        attempt: 0,
        key,
        transcript: Arc::new(transcript),
    });
    d.wait_for("Transcript saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
    })?;
    save_activity(d)?;
    let revision = d.revision();
    let can_undo = d.app().workspace.as_ref().is_some_and(|w| w.can_undo);

    // Pauses show as quiet bands on Your edit's cards.
    let bands = painted_pause_bands(d);
    d.check(
        "Detected pauses are painted as bands on the beat cards",
        bands >= 1,
        json!({"bands":">=1"}),
        json!({"bands":bands}),
    )?;
    d.command("correct")?;
    d.settled()?;
    let heading = scenarios::text_paint_visibility(d, "Correct transcript and pauses");
    d.check(
        ":correct opens the correction sheet over the words and pauses",
        d.app().correction_open() && !heading.is_empty(),
        json!({"open":true}),
        json!({"open":d.app().correction_open(),"heading":heading}),
    )?;
    d.capture("Correction sheet")?;

    // Split "beta" into two words by typing a space.
    select(d, Item::Word(1))?;
    d.key(Key::C)?;
    d.settled()?;
    d.events(
        "Type into the word field",
        vec![egui::Event::Text(" uh".into())],
    )?;
    d.key(Key::Enter)?;
    wait_saved(d, "split")?;
    let words = texts(d);
    d.check(
        "Editing text with a space splits the word; the edit is unchanged",
        words
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect::<Vec<_>>()
            == ["alpha", "beta", "uh", "gamma"]
            && words[1].1 == 100
            && words[2].2 == 150
            && d.revision() == revision,
        json!({"words":["alpha","beta","uh","gamma"],"revision":revision}),
        json!({"words":words,"revision":d.revision()}),
    )?;
    let corrected = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.transcript.as_ref())
        .map(|transcript| {
            (0..4)
                .map(|word| transcript.corrected(word))
                .collect::<Vec<_>>()
        });
    d.check(
        "Split words are marked corrected; others stay recognized",
        corrected.as_deref() == Some(&[false, true, true, false][..]),
        json!([false, true, true, false]),
        json!(corrected),
    )?;
    d.capture("Split word")?;

    // Move gamma's start 30 ms earlier as one draft, then apply it.
    select(d, Item::Word(3))?;
    d.key(Key::B)?;
    for _ in 0..3 {
        d.key(Key::H)?;
    }
    d.settled()?;
    let unsaved = scenarios::text_paint_visibility(d, "UNSAVED");
    let before_apply = texts(d)[3].1;
    d.check(
        "Edge nudges are an unsaved draft until Enter",
        !unsaved.is_empty() && before_apply == 200,
        json!({"unsaved":true,"gamma_start":200}),
        json!({"unsaved":unsaved,"gamma_start":before_apply}),
    )?;
    d.capture("Unsaved edge draft")?;
    d.key(Key::Enter)?;
    wait_saved(d, "edge")?;
    d.check(
        "Enter applies the moved edge as one correction",
        texts(d)[3].1 == 197 && d.revision() == revision,
        json!({"gamma_start":197}),
        json!({"gamma_start":texts(d)[3].1}),
    )?;

    d.key(Key::U)?;
    wait_saved(d, "undo")?;
    let undone = texts(d)[3].1;
    d.key_modified(Key::U, egui::Modifiers::SHIFT)?;
    wait_saved(d, "redo")?;
    let redone = texts(d)[3].1;
    d.check(
        "u and Shift+U undo and redo the correction, not the edit",
        undone == 200
            && redone == 197
            && d.revision() == revision
            && d.app().workspace.as_ref().is_some_and(|w| w.can_undo) == can_undo,
        json!({"undone":200,"redone":197,"revision":revision,"can_undo":can_undo}),
        json!({"undone":undone,"redone":redone,"revision":d.revision(),"can_undo":d.app().workspace.as_ref().map(|w| w.can_undo)}),
    )?;

    // Remove the first pause.
    let pauses = pause_count(d);
    select(d, Item::Pause(0))?;
    d.key(Key::X)?;
    wait_saved(d, "pause removal")?;
    let after = pause_count(d);
    let projected = d
        .app_mut()
        .source_analysis()
        .ok()
        .and_then(|speech| speech.pauses().map(<[_]>::len));
    d.check(
        "x removes a pause; pause motions see the corrected pauses",
        pauses == 2 && after == 1 && projected.is_none_or(|count| count <= 1),
        json!({"before":2,"after":1}),
        json!({"before":pauses,"after":after,"projected":projected}),
    )?;
    d.capture("Pause removed")?;

    // Backspace and Delete are text keys and never remove anything.
    let words = texts(d).len();
    let pauses = pause_count(d);
    d.key(Key::Backspace)?;
    d.key(Key::Delete)?;
    d.settled()?;
    d.check(
        "Backspace and Delete never remove a word or pause",
        texts(d).len() == words && pause_count(d) == pauses && d.app().correction_settled(),
        json!({"words":words,"pauses":pauses}),
        json!({"words":texts(d).len(),"pauses":pause_count(d)}),
    )?;

    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape closes the sheet",
        !d.app().correction_open(),
        json!({"open":false}),
        json!({"open":d.app().correction_open()}),
    )?;
    d.report.skipped.push("Replay corrects a synthetic transcript and synthetic speech activity; measured-edge snapping (Shift+H/L) is covered by analysis tests.".into());
    Ok(())
}

pub(super) fn save_activity(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let asset = original_asset(workspace).ok_or("No Original")?;
    let source = workspace
        .sources
        .get(asset)
        .ok_or("Missing Original source")?;
    let audio = source
        .receipt
        .snapshot()
        .audio()
        .ok_or("Fixture has no audio")?;
    let origin = audio
        .frames()
        .first()
        .ok_or("Empty audio index")?
        .valid_start;
    let samples: u64 = 60_800;
    let quiet =
        |sample: u64| (9_600..15_200).contains(&sample) || (25_600..31_200).contains(&sample);
    let speech = (0..samples.div_ceil(deadpan_analysis::VAD_HOP))
        .map(|hop| {
            if quiet(hop * deadpan_analysis::VAD_HOP + 256) {
                0
            } else {
                240
            }
        })
        .collect();
    let energy = (0..samples.div_ceil(deadpan_analysis::ENERGY_HOP))
        .map(|frame| {
            if quiet(frame * deadpan_analysis::ENERGY_HOP + 80) {
                20
            } else {
                200
            }
        })
        .collect();
    let activity = SpeechActivity::new(
        ActivityAudio {
            origin,
            sample_rate: audio.stream().sample_rate,
            samples,
        },
        speech,
        energy,
    )
    .map_err(|e| e.to_string())?;
    let key = deadpan_store::SpeechActivityKey {
        content: source.receipt.original().content().to_string(),
        audio_stream: audio.stream().stream_index,
        model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
        engine: "replay".into(),
    };
    let session = workspace.session;
    d.app_mut().submit(ProjectRequest::SaveSpeechActivity {
        expected_session: session,
        attempt: 0,
        key,
        activity: Arc::new(activity),
    });
    d.wait_for("Speech activity saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.speech_activity.is_some())
    })
}

/// Move the selection with real `l`/`h` presses until it reaches `item`.
fn select(d: &mut Driver<'_>, item: Item) -> Result<(), String> {
    for _ in 0..12 {
        let current = d.app().correction_item();
        if current == Some(item) {
            return Ok(());
        }
        let forward = match (current, item) {
            (Some(Item::Word(at)), Item::Word(to)) => at < to,
            _ => true,
        };
        let key = if forward { Key::L } else { Key::H };
        d.key(key)?;
        d.settled()?;
        if d.app().correction_item() == current && forward {
            // At the end: go back from the start.
            for _ in 0..12 {
                d.key(Key::H)?;
            }
        }
    }
    Err(format!(
        "Could not select {item:?}; selected {:?}",
        d.app().correction_item()
    ))
}

fn wait_saved(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    d.wait_for(&format!("Correction saved: {label}"), |app| {
        app.correction_settled()
    })?;
    d.settled()?;
    if let Some(error) = d.app().correction_error() {
        return Err(format!("Correction {label} failed: {error}"));
    }
    Ok(())
}

fn texts(d: &Driver<'_>) -> Vec<(String, u32, u32)> {
    d.app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.transcript.as_ref())
        .map(|transcript| {
            transcript
                .transcript
                .words()
                .iter()
                .map(|word| (word.text.clone(), word.start_cs, word.end_cs))
                .collect()
        })
        .unwrap_or_default()
}

fn pause_count(d: &Driver<'_>) -> usize {
    d.app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.speech_activity.as_ref())
        .map_or(0, |activity| activity.pauses.pauses.len())
}

fn painted_pause_bands(d: &Driver<'_>) -> usize {
    d.harness
        .output()
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape, egui::Shape::Rect(rect) if rect.fill == crate::preview::style::PAUSE_BAND)
        })
        .count()
}
