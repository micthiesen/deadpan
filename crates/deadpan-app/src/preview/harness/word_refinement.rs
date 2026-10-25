//! Real model/worker proposals on an explicitly supplied, seeded Original.
//! Audition routing/feedback are simulated in the visual lane; no proposal or Ready state is injected.
use super::*;
use crate::preview::corrections::Item;
use deadpan_analysis::refinement::RefinementOfferAvailability;
use egui::Key;

const REFINE: &str = "Refine word timing locally…";
const ACCEPT: &str = "Accept approximate timing  Enter";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("This replay runs the actual installed phoneme pack and isolated Rust/ONNX worker on the explicit fixture Original. Audition routing and feedback are simulated; canonical playback PCM and device output are not exercised in this visual lane; listening, VoiceOver speech, OS IME and physical display behavior remain owner checks. Fixture/transcript are workflow inputs, not a timing-quality oracle.".into());
    d.wait_for("Seeded Original transcript is published", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
    })?;
    let word = d.options.refinement_word;
    let baseline = d.app().workspace.clone().ok_or("No fixture workspace")?;
    let transcript = baseline
        .transcript
        .as_ref()
        .ok_or("Fixture needs its actual transcript")?;
    if transcript.transcript.words().get(word).is_none() || transcript.corrected(word) {
        return Err("The explicit refinement target must be an existing unpinned word".into());
    }
    let baseline_words = transcript.transcript.words().to_vec();
    let pinned: Vec<_> = (0..baseline_words.len())
        .filter(|index| transcript.corrected(*index))
        .collect();
    if pinned.is_empty() {
        return Err(
            "The refinement fixture must include a manually pinned neighboring word".into(),
        );
    }
    let baseline_regions = baseline
        .corrections
        .as_ref()
        .and_then(|value| value.stored.as_ref())
        .ok_or("Pinned correction fixture missing")?
        .corrections
        .word_regions()
        .to_vec();
    let baseline_version = baseline.corrections.as_ref().unwrap().version();
    let revision = d.revision();
    let location = (
        d.app().source_cursor,
        d.app().sequence_cursor,
        d.app().selected_beat.clone(),
        d.app().moment.range(),
    );

    // A real queued job can be cancelled without touching annotations.
    open(d, word, true)?;
    d.key(Key::Escape)?;
    drain(d, "Cancelled native refinement drains")?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "Cancelling a real requested job preserves every word and authored revision",
    )?;

    // Republish the unchanged raw transcript through the sole writer while a
    // new real job is active. A changed immutable display identity still revokes it.
    open(d, word, false)?;
    let current = d.app().workspace.clone().ok_or("No workspace")?;
    let source = current.transcript.as_ref().ok_or("No transcript")?;
    let request = ProjectRequest::SaveTranscript {
        expected_session: current.session,
        attempt: 90_001,
        lifecycle_epoch: None,
        key: source.key.clone(),
        transcript: source.proposal.clone(),
    };
    if !d.app_mut().submit(request) {
        return Err("Could not enqueue the stale-display witness".into());
    }
    d.wait_for_limit(
        "Changed transcript revokes the native proposal",
        Duration::from_secs(120),
        |app| app.refinement.status() == "failed",
    )?;
    d.check(
        "A replaced transcript cannot acquire the old word timing offer",
        d.app()
            .refinement
            .error()
            .is_some_and(|error| error.contains("changed")),
        json!("changed transcript refuses"),
        json!(d.app().refinement.error()),
    )?;
    d.key(Key::Escape)?;
    drain(d, "Stale native refinement drains")?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "Stale completion cannot change annotations",
    )?;

    // Sleep is observed by the job's own watcher, not just a UI progress callback.
    open(d, word, false)?;
    d.app().service.jobs().lifecycle().will_sleep();
    d.app().service.jobs().lifecycle().did_wake();
    d.wait_for_limit(
        "Sleep invalidates the native refinement run",
        Duration::from_secs(120),
        |app| app.refinement.status() == "failed",
    )?;
    d.check(
        "Wake does not silently resume or accept a local timing candidate",
        d.app()
            .refinement
            .error()
            .is_some_and(|error| error.contains("sleep")),
        json!("sleep interruption"),
        json!(d.app().refinement.error()),
    )?;
    d.key(Key::Escape)?;
    drain(d, "Interrupted native refinement drains")?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "Interrupted refinement preserves word timing and edit history",
    )?;

    // The successful run starts through the keyboard-discoverable native button.
    open(d, word, true)?;
    d.wait_for_limit(
        "Actual installed model and isolated worker finish",
        Duration::from_secs(120),
        |app| app.refinement.status() != "running",
    )?;
    if d.app().refinement.status() != "ready" {
        return Err(format!(
            "Actual refinement did not produce a review: {} {:?}",
            d.app().refinement.status(),
            d.app().refinement.error()
        ));
    }
    let engine = d.app().refinement.engine().map(str::to_owned);
    let review = d.app().refinement.review().ok_or("No real review")?;
    let offer = review.offers().first().ok_or("No target offer")?.clone();
    d.check(
        "The actual pinned native backend supplies an explicit approximate offer",
        engine.as_deref() == Some(deadpan_jobs::refinement::ENGINE_ID)
            && offer.availability == RefinementOfferAvailability::Approximate,
        json!({"engine":deadpan_jobs::refinement::ENGINE_ID,"availability":"Approximate"}),
        json!({"engine":engine,"offer":offer,"diagnostics":review.diagnostics()}),
    )?;
    let proposed = offer
        .centisecond_bounds
        .ok_or("No offered annotation bounds")?;
    d.settled()?;
    let waveform = scenarios::text_paint_visibility(d, "Measured Original waveform");
    d.check(
        "The real measured waveform and old/new edge overlay are visible",
        waveform.iter().any(|row| row["fully_visible"] == true),
        json!("visible measured waveform label"),
        json!(waveform),
    )?;
    let hear = scenarios::text_paint_visibility(d, "Hear comparison context");
    d.check(
        "Comparison audition is visible without scrolling at the standard workspace size",
        hear.iter().any(|row| row["fully_visible"] == true),
        json!("visible comparison audition"),
        json!(hear),
    )?;
    d.capture("Real native word timing, old and raw candidate edges")?;

    // Modified Enter belongs to the reserved/global/native input rules.
    d.key_modified(Key::Enter, egui::Modifiers::COMMAND)?;
    d.check(
        "Command-Return cannot accept the proposal",
        d.app().refinement.status() == "ready" && !d.app().service.is_busy(),
        json!("ready, no save"),
        json!(d.app().refinement.status()),
    )?;

    // Context playback owns a separate audio-only range and changes no editor cursor.
    d.key(Key::Space)?;
    d.check(
        "Audition uses the captured audio-only comparison context",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| matches!(run.domain(), crate::transport::Domain::AudioRange(_)))
            && location
                == (
                    d.app().source_cursor,
                    d.app().sequence_cursor,
                    d.app().selected_beat.clone(),
                    d.app().moment.range(),
                ),
        json!("audio-range, editor location retained"),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;

    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.settled()?;
    let accept = scenarios::text_paint_visibility(d, "Accept approximate timing");
    let hear = scenarios::text_paint_visibility(d, "Hear comparison context");
    d.check(
        "Minimum workspace keeps comparison audition and explicit acceptance visible",
        accept.iter().any(|row| row["fully_visible"] == true)
            && hear.iter().any(|row| row["fully_visible"] == true),
        json!("visible comparison audition and acceptance"),
        json!({"accept":accept,"hear":hear}),
    )?;
    d.capture("Real word timing review at minimum size")?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.settled()?;

    // Reach Accept using real Tab focus; Space must activate the focused
    // button rather than being stolen by the audition shortcut.
    focus_button(d, ACCEPT)?;
    d.key(Key::Space)?;
    d.wait_for("Explicitly accepted timing is published", |app| {
        !app.refinement.open()
    })?;
    d.settled()?;
    let accepted = d.app().workspace.clone().ok_or("No accepted workspace")?;
    let words = accepted
        .transcript
        .as_ref()
        .ok_or("No accepted transcript")?;
    let version = accepted.corrections.as_ref().unwrap().version();
    d.check("One explicit acceptance saves one correction while preserving cuts and neighbor bounds",
        version == baseline_version + 1 && d.revision() == revision
            && words.transcript.words()[word].start_cs == proposed.start_cs
            && words.transcript.words()[word].end_cs == proposed.end_cs
            && words.transcript.words().iter().enumerate().all(|(index, value)| index == word || *value == baseline_words[index])
            && baseline_regions.iter().all(|region| accepted.corrections.as_ref().unwrap().stored.as_ref().unwrap().corrections.word_regions().contains(region)),
        json!({"version":baseline_version+1,"bounds":proposed,"revision":revision,"prior_pins":pinned}),
        json!({"version":version,"bounds":[words.transcript.words()[word].start_cs,words.transcript.words()[word].end_cs],"revision":d.revision()}))?;
    d.capture("Accepted native timing remains a correction outside edit history")?;

    super::corrections::select(d, Item::Word(word))?;
    d.settled()?;
    let disabled = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some(REFINE) && access.is_disabled()
    });
    let pinned_reason = scenarios::text_paint_visibility(d, "timing is manually pinned");
    d.check(
        "Accepted words visibly stay pinned and retain manual edge controls",
        disabled && !pinned_reason.is_empty(),
        json!("disabled refinement with pinned reason"),
        json!({"disabled":disabled,"reason":pinned_reason}),
    )?;
    d.key(Key::U)?;
    d.wait_for(
        "One correction Undo restores the earlier target timing",
        |app| {
            app.workspace
                .as_ref()
                .and_then(|workspace| workspace.transcript.as_ref())
                .is_some_and(|words| words.transcript.words()[word] == baseline_words[word])
        },
    )?;
    d.settled()?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "One correction Undo restores all prior words and preserves authored history",
    )?;
    d.key(Key::Escape)?;
    Ok(())
}

fn open(d: &mut Driver<'_>, word: usize, keyboard: bool) -> Result<(), String> {
    if !d.app().correction_open() {
        d.command("correct")?;
        d.settled()?;
    }
    super::corrections::select(d, Item::Word(word))?;
    if keyboard {
        focus_button(d, REFINE)?;
        d.key(Key::Enter)?;
    } else {
        d.click(REFINE)?;
    }
    d.check(
        "Local refinement opens as an unsaved owned request",
        d.app().refinement.open(),
        json!(true),
        json!(d.app().refinement.status()),
    )
}

fn focus_button(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..48 {
        if d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.label().as_deref() == Some(label) && access.is_focused() && !access.is_disabled()
        }) {
            return Ok(());
        }
        d.key(Key::Tab)?;
    }
    Err(format!(
        "Could not focus {label} using Tab: {}",
        d.widgets()
    ))
}

fn drain(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    d.wait_for_limit(label, Duration::from_secs(120), |app| {
        !app.refinement.open() && app.refinement.drained()
    })
}

fn unchanged(
    d: &mut Driver<'_>,
    words: &[deadpan_analysis::Word],
    revision: &str,
    label: &str,
) -> Result<(), String> {
    let current = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.transcript.as_ref())
        .ok_or("No transcript")?;
    d.check(
        label,
        current.transcript.words() == words && d.revision() == revision,
        json!({"words":words,"revision":revision}),
        json!({"words":current.transcript.words(),"revision":d.revision()}),
    )
}

/// A separate performance lane owns the actual canonical PCM preparation,
/// output device, reported delivery clock and pause/resume path.
pub(super) fn run_playback(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Native inference, canonical AudioRange preparation and macOS device delivery are real in this lane. Acoustic listening, physical display timing, VoiceOver speech and OS-delivered IME remain owner checks; this is workflow verification, not a phonetic accuracy evaluation.".into());
    if d.app().feedback.simulate_playback {
        return Err("Real refinement playback lane must not inject audition feedback".into());
    }
    d.wait_for("Actual fixture transcript is published", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
    })?;
    d.command("monitor 5%")?;
    d.settled()?;
    let word = d.options.refinement_word;
    let baseline = d.app().workspace.clone().ok_or("No fixture")?;
    let baseline_words = baseline
        .transcript
        .as_ref()
        .ok_or("No transcript")?
        .transcript
        .words()
        .to_vec();
    let revision = d.revision();
    let location = (
        d.app().source_cursor,
        d.app().sequence_cursor,
        d.app().selected_beat.clone(),
        d.app().moment.range(),
    );
    open(d, word, true)?;
    d.wait_for_limit(
        "Real playback lane finishes actual native inference",
        Duration::from_secs(120),
        |app| app.refinement.status() != "running",
    )?;
    if d.app().refinement.status() != "ready" {
        return Err(format!(
            "No real native review for device audition: {:?}",
            d.app().refinement.error()
        ));
    }
    d.check(
        "Real device lane starts from a verified native model proposal",
        d.app().refinement.engine() == Some(deadpan_jobs::refinement::ENGINE_ID),
        json!(deadpan_jobs::refinement::ENGINE_ID),
        json!(d.app().refinement.engine()),
    )?;
    let picture = d.app().presentation.diagnostic_snapshot();
    let before = d.app().playback.diagnostics();
    d.key(Key::Space)?;
    d.wait_for_limit(
        "Actual AudioRange device delivery advances",
        Duration::from_secs(30),
        |app| {
            app.transport.as_ref().is_some_and(|run| {
                run.phase == deadpan_playback::Phase::Playing
                    && matches!(run.domain(), crate::transport::Domain::AudioRange(_))
                    && run.sample.0 > run.window().start().0 + 2400
                    && run.sample.0 < run.window().end().0 - 2400
            })
        },
    )?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No actual audio range transport")?;
    let first_generation = run.generation;
    let first_window = *run.window();
    let delivered = run.sample;
    let active = d.app().playback.diagnostics();
    d.check("Actual prepared PCM reaches a healthy device generation",
        first_generation.is_some() && active.generations > before.generations && active.reports > before.reports
            && active.starved == before.starved && active.faults == before.faults && active.silent_frames == before.silent_frames,
        json!({"real_device":true,"starvation":0,"faults":0,"padding":0}),
        json!({"generation":format!("{first_generation:?}"),"sample":delivered.0,
            "generations":active.generations-before.generations,"reports":active.reports-before.reports,
            "starved":active.starved-before.starved,"faults":active.faults-before.faults,"silent_frames":active.silent_frames-before.silent_frames}))?;
    d.key(Key::Space)?;
    let paused = d
        .app()
        .resume
        .as_ref()
        .ok_or("Pause lost the exact AudioRange resume")?;
    let current = d.app().workspace.as_ref().ok_or("No current workspace")?;
    let retained = paused
        .sample_for_domain(
            crate::transport::ContentRef {
                session: current.session,
                project: current.document.project_id(),
                revision: current.document.revision_id(),
                content: &deadpan_playback::ContentIdentity::Committed,
            },
            paused.domain(),
            paused.window(),
            d.app().refinement.audition_cursor(),
        )
        .ok_or("Paused source-range sample/cursor identity was lost")?;
    d.check("Pause retains an independent exact context sample and the same window",
        d.app().transport.is_none() && *paused.window() == first_window && retained >= delivered,
        json!({"paused":true,"minimum_sample":delivered.0}), json!({"retained_sample":retained.0,"independent_cursor":d.app().refinement.audition_cursor()}))?;
    d.key(Key::Space)?;
    let resumed = d
        .app()
        .transport
        .as_ref()
        .ok_or("No resumed AudioRange request")?;
    d.check("Resume starts at the retained device estimate rather than restarting the context",
        resumed.sample == retained && *resumed.window() == first_window,
        json!({"sample":retained.0,"window":[first_window.start().0,first_window.end().0]}),
        json!({"sample":resumed.sample.0,"window":[resumed.window().start().0,resumed.window().end().0]}))?;
    d.wait_for_limit(
        "Resumed actual device generation advances",
        Duration::from_secs(30),
        |app| {
            app.transport.as_ref().is_some_and(|run| {
                run.phase == deadpan_playback::Phase::Playing
                    && run.generation != first_generation
                    && run.sample > retained
            })
        },
    )?;
    d.check("Real context audition preserves both editor clocks, selection and submitted picture identity",
        location == (d.app().source_cursor, d.app().sequence_cursor, d.app().selected_beat.clone(), d.app().moment.range())
            && picture["displayed"] == d.app().presentation.diagnostic_snapshot()["displayed"]
            && picture["geometry_revision"] == d.app().presentation.diagnostic_snapshot()["geometry_revision"]
            && picture["source_frame"] == d.app().presentation.diagnostic_snapshot()["source_frame"],
        json!({"editor_location":"retained","picture":picture}), d.snapshot())?;
    let resumed_generation = d
        .app()
        .transport
        .as_ref()
        .ok_or("No advancing resumed AudioRange")?
        .generation;
    d.wait_for_limit(
        "Resumed comparison reaches its actual device terminal callback",
        Duration::from_secs(30),
        |app| {
            app.transport.is_none()
                && app.resume.is_none()
                && app
                    .feedback
                    .last_playback_update
                    .as_ref()
                    .is_some_and(|update| {
                        update.phase == deadpan_playback::Phase::Ended
                            && update.generation == resumed_generation
                            && update.sample == Some(first_window.end())
                    })
        },
    )?;
    let ended = d.app().playback.diagnostics();
    let terminal = d
        .app()
        .feedback
        .last_playback_update
        .as_ref()
        .ok_or("No actual AudioRange terminal update")?;
    d.check("Natural context completion clears transport and resume without moving editor clocks or picture",
        d.app().refinement.status() == "ready" && terminal.error.is_none()
            && ended.faults == before.faults && ended.starved == before.starved
            && location == (d.app().source_cursor, d.app().sequence_cursor, d.app().selected_beat.clone(), d.app().moment.range())
            && picture["displayed"] == d.app().presentation.diagnostic_snapshot()["displayed"]
            && picture["geometry_revision"] == d.app().presentation.diagnostic_snapshot()["geometry_revision"]
            && picture["source_frame"] == d.app().presentation.diagnostic_snapshot()["source_frame"],
        json!({"phase":"Ended","sample":first_window.end().0,"transport":false,"resume":false,"editor_location":"retained","faults":0,"starved":0}),
        json!({"phase":format!("{:?}",terminal.phase),"sample":terminal.sample.map(|sample|sample.0),
            "generation":format!("{:?}",terminal.generation),"error":terminal.error,
            "faults":ended.faults-before.faults,"starved":ended.starved-before.starved,"state":d.snapshot()}))?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "Natural AudioRange completion preserves annotations and authored history",
    )?;
    d.key(Key::Space)?;
    let restarted = d
        .app()
        .transport
        .as_ref()
        .ok_or("Fresh Space did not restart the comparison context")?;
    d.check(
        "Fresh Space after completion restarts the same context at its beginning",
        restarted.sample == first_window.start() && *restarted.window() == first_window,
        json!({"sample":first_window.start().0,"window":[first_window.start().0,first_window.end().0]}),
        json!({"sample":restarted.sample.0,"window":[restarted.window().start().0,restarted.window().end().0]}),
    )?;
    d.wait_for_limit(
        "Restarted comparison activates a new real device generation",
        Duration::from_secs(30),
        |app| {
            app.transport.as_ref().is_some_and(|run| {
                run.phase == deadpan_playback::Phase::Playing
                    && run.generation.is_some()
                    && run.generation != resumed_generation
                    && run.sample > first_window.start()
            })
        },
    )?;
    d.key(Key::Escape)?;
    d.wait_for(
        "Closing the review revokes context transport and resume",
        |app| !app.refinement.open() && app.transport.is_none() && app.resume.is_none(),
    )?;
    unchanged(
        d,
        &baseline_words,
        &revision,
        "Real device audition never accepts timing or edits the Original",
    )?;
    d.key(Key::Escape)?;
    Ok(())
}
