#![cfg(target_os = "macos")]

//! DP-02 / spec 26.3: 10,000 actual one-frame time insertions/extensions.
//! Six live nodes keep the command/history work bounded. Expected clocks and
//! resumed PCM come from an exact integer-ratio oracle and independently decoded
//! Original, never from a previous edited render. Missing tools fail explicitly.

#[path = "fractional_edits/audio.rs"]
mod audio;
#[path = "fractional_edits/edits.rs"]
mod edits;
#[path = "fractional_edits/export.rs"]
mod export;
#[path = "fractional_edits/fixture.rs"]
mod fixture;
#[path = "fractional_edits/support.rs"]
mod support;

use serde_json::json;
use support::{Result, Run};

const EDITS: i64 = 10_000;
const SOURCE_FRAMES: i64 = 100;
const CUT_FRAME: i64 = 50;
const CUT_SAMPLE: i64 = 80_080;
const FINAL_FRAMES: i64 = 10_100;
const FINAL_SAMPLES: i64 = 16_176_160;

/// Independently derived five-frame allocation cycle at 30000/1001 and 48 kHz.
/// Do not replace this with any production frame/sample conversion helper.
fn boundary(frame: i64) -> i64 {
    assert!(frame >= 0);
    8_008 * (frame / 5) + [0, 1_602, 3_203, 4_805, 6_406][(frame % 5) as usize]
}

/// Flattened structural source coordinate, in fifths of a sample. This omits
/// the authored resume binding and is deliberately not the canonical PCM oracle.
fn flattened_suffix_source_fifths(inserted: i64, output_sample: i64) -> i128 {
    i128::from(output_sample) * 5 - i128::from(inserted) * 8_008
}

#[test]
fn ten_thousand_one_frame_duration_edits_preserve_fractional_av_boundaries() -> Result {
    let run = Run::new()?;
    eprintln!("fractional edits evidence: {}", run.root.display());
    let result = qualify(&run);
    if let Err(error) = &result {
        run.write_json("failure.json", &json!({"error": error.to_string()}))?;
        eprintln!(
            "fractional edits failed; retained {}: {error}",
            run.root.display()
        );
    }
    result
}

fn qualify(run: &Run) -> Result {
    let started = std::time::Instant::now();
    let source = fixture::create(run)?;
    let fixture_seconds = started.elapsed().as_secs_f64();
    let started = std::time::Instant::now();
    let edited = edits::apply(run, &source)?;
    let edit_seconds = started.elapsed().as_secs_f64();
    let started = std::time::Instant::now();
    let pcm = audio::check(run, &source, &edited)?;
    let pcm_seconds = started.elapsed().as_secs_f64();
    let started = std::time::Instant::now();
    let emitted = export::check(run, &source, &edited)?;
    let export_seconds = started.elapsed().as_secs_f64();
    assert_eq!(
        run.cli_sha256,
        support::sha256(std::path::Path::new(env!("CARGO_BIN_EXE_deadpan-cli")))?,
        "CLI binary changed during qualification"
    );
    assert_eq!(
        run.test_binary_sha256,
        support::sha256(&std::env::current_exe()?)?,
        "test binary changed during qualification"
    );
    let bytes = run.check_storage()?;
    run.check("completed")?;
    run.write_json(
        "result.json",
        &json!({
            "passed": true,
            "scope": "10000 one-frame time insertions/extensions; shared PCM and real Render",
            "commands": {"insert_time": 1, "set_hold_duration": 9999,
                "edge_setup": edited.setup_commands},
            "intermediate_pcm_edits": edited.checkpoints.iter().map(|(i, _)| i).collect::<Vec<_>>(),
            "final_revision": edited.revision.as_str(),
            "final_frames": FINAL_FRAMES, "final_samples": FINAL_SAMPLES,
            "wrong_sum_of_rounded_deltas": EDITS * boundary(1),
            "correct_inserted_samples": boundary(EDITS),
            "maximum_document_bytes": edited.maximum_document_bytes,
            "maximum_request_edit_bytes": edited.maximum_edit_bytes,
            "history_before": edited.history_before, "history_after": edited.history_after,
            "pcm": pcm, "emitted": emitted, "scratch_bytes": bytes,
            "seconds": {"fixture": fixture_seconds, "edits": edit_seconds,
                "shared_pcm": pcm_seconds, "render_and_verify": export_seconds},
            "source_sha256": support::sha256(&source.movie)?,
            "cli_sha256": run.cli_sha256,
            "test_binary_sha256": run.test_binary_sha256,
        }),
    )?;
    eprintln!("fractional edits passed; evidence {}", run.root.display());
    Ok(())
}
