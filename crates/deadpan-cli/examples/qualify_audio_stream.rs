//! Read-only canonical PCM preparation timings, without an audio device.
//! Usage: qualify_audio_stream PACKAGE [BUILD_LABEL] > REPORT.json
//! Run separately in debug and release, with other builds stopped. A completed
//! measurement does not qualify device deadlines or acoustic playback.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::ExitCode,
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use deadpan_cli::audio::{MAX_OFFLINE_AUDIO_FRAMES, OfflineAudioSession};
use deadpan_core::{AudioSample, FrameRange, ProjectFrame};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SAMPLE_RATE: u32 = 48_000;
const BATCH_FRAMES: u32 = 8_192;
const MAXIMUM_BATCHES: i64 = 8;
const MAXIMUM_SECONDS: u64 = 120;

fn main() -> ExitCode {
    let mut report = json!({
        "schema_version": 1,
        "status": "running",
        "scope": "read-only committed HEAD, canonical limited PCM preparation; no device or UI",
        "build": {
            "crate": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "debug_assertions": cfg!(debug_assertions),
            "target_os": std::env::consts::OS,
            "target_arch": std::env::consts::ARCH,
            "profile_note": "Build label is caller-supplied; debug_assertions does not establish optimization flags or compiler version."
        },
        "sample_rate": SAMPLE_RATE,
        "channels": 2,
        "batch_frames": BATCH_FRAMES,
        "maximum_batches_per_pass": MAXIMUM_BATCHES,
        "full_batch_audio_budget_ms": f64::from(BATCH_FRAMES) * 1000.0 / f64::from(SAMPLE_RATE),
        "maximum_seconds": MAXIMUM_SECONDS,
        "passes": [],
        "limits": [
            "Timing includes canonical read and media admission; hashing and reporting are outside read timing.",
            "First read includes cold source preparation. Later reads measure refill work in one persistent session.",
            "Fresh sessions reset application caches, not operating-system file caches.",
            "Retained-session repeat reuses the session; its four-tile cache cannot retain an eight-block pass.",
            "A read exceeding its audio duration records preparation risk, not a measured device underrun.",
            "Read count, returned PCM and cooperative deadlines are bounded; media admission may inspect the complete source."
        ]
    });
    let result = run(&mut report);
    report["status"] = json!(if result.is_ok() { "measured" } else { "failed" });
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    if let Err(error) = serde_json::to_writer_pretty(&mut output, &report)
        .map_err(std::io::Error::other)
        .and_then(|()| output.write_all(b"\n"))
    {
        eprintln!("cannot write qualification report: {error}");
        return ExitCode::FAILURE;
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(report: &mut Value) -> Result<()> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(MAXIMUM_SECONDS);
    report["started_unix_ms"] = json!(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis());
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if !(1..=2).contains(&arguments.len()) {
        return Err("usage: qualify_audio_stream PACKAGE [BUILD_LABEL]".into());
    }
    report["build"]["label"] = match arguments.get(1) {
        Some(label) => json!(label.to_str().ok_or("BUILD_LABEL must be UTF-8")?),
        None => Value::Null,
    };
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    report["build"]["executable_on_disk"] = json!(executable);
    report["build"]["executable_sha256"] = json!(hash_file(&executable, deadline)?);
    let package = fs::canonicalize(&arguments[0])?;
    report["package"] = json!(package);
    let captured = Instant::now();
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    drop(store);
    let plan = RenderPlan::compile(&document)?;
    let range = FrameRange::new(ProjectFrame(0), ProjectFrame(plan.duration().frames()))?;
    let rate = document.presentation_basis().frame_rate;
    let available = rate.audio_boundary(range.end())?.0;
    let end = available.min(MAXIMUM_BATCHES * i64::from(BATCH_FRAMES));
    report["capture_ms"] = json!(captured.elapsed().as_secs_f64() * 1000.0);
    report["project_id"] = json!(document.project_id());
    report["revision_id"] = json!(document.revision_id());
    report["document_sha256"] = json!(hex(Sha256::digest(document.to_json()?.as_bytes())));
    report["frame_rate"] =
        json!({"numerator": rate.numerator(), "denominator": rate.denominator()});
    report["captured_frame_range"] = json!([range.start().0, range.end().0]);
    report["available_sample_range"] = json!([0, available]);
    report["measured_sample_range"] = json!([0, end]);
    report["shortened_to_available_interval"] =
        json!(available < MAXIMUM_BATCHES * i64::from(BATCH_FRAMES));
    if available <= i64::from(BATCH_FRAMES) {
        return Err(
            "fixture must contain more than 8192 samples to measure a refill after prefill".into(),
        );
    }
    if BATCH_FRAMES > MAX_OFFLINE_AUDIO_FRAMES {
        return Err("diagnostic batch exceeds the admitted offline audio bound".into());
    }
    check_deadline(deadline)?;
    let cancelled = AtomicBool::new(false);
    let mut reference = None;
    let mut passes = Vec::new();
    // Drop the first session before opening the second, retaining one source
    // cache at a time. Both pin the initial HEAD even if a writer later edits.
    for fresh in 1..=2 {
        let opening = Instant::now();
        let session = OfflineAudioSession::open_revision(
            &package,
            document.revision_id(),
            range,
            &cancelled,
            deadline,
        );
        let open_ms = opening.elapsed().as_secs_f64() * 1000.0;
        let mut pass =
            json!({"kind": "fresh_session", "session": fresh, "open_ms": open_ms, "reads": []});
        let result = (|| -> Result<()> {
            let mut session = session?;
            if session.document() != &document {
                return Err("reopened immutable revision differs from captured HEAD".into());
            }
            measure(&mut session, end, &cancelled, &mut pass, &mut reference)?;
            passes.push(pass.take());
            if fresh == 2 {
                pass = json!({"kind": "retained_session_repeat", "session": fresh, "open_ms": null, "reads": []});
                measure(&mut session, end, &cancelled, &mut pass, &mut reference)?;
            }
            Ok(())
        })();
        if let Err(error) = &result {
            pass["error"] = json!(error.to_string());
        }
        if !pass.is_null() {
            passes.push(pass);
        }
        report["passes"] = json!(passes);
        result?;
    }
    report["all_pcm_and_gain_hashes_match"] = json!(true);
    report["elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    Ok(())
}

fn measure(
    session: &mut OfflineAudioSession,
    end: i64,
    cancelled: &AtomicBool,
    pass: &mut Value,
    reference: &mut Option<Vec<(String, String)>>,
) -> Result<()> {
    let mut start = 0;
    let mut hashes = Vec::new();
    while start < end {
        let frames = u32::try_from((end - start).min(i64::from(BATCH_FRAMES)))?;
        let began = Instant::now();
        let result = session.read(AudioSample(start), frames, cancelled);
        let elapsed_ms = began.elapsed().as_secs_f64() * 1000.0;
        let budget_ms = f64::from(frames) * 1000.0 / f64::from(SAMPLE_RATE);
        let mut row = json!({
            "start_sample": start, "requested_frames": frames, "read_ms": elapsed_ms,
            "audio_budget_ms": budget_ms, "exceeds_audio_budget": elapsed_ms > budget_ms,
            "first_read": start == 0
        });
        let block = match result {
            Ok(block) => block,
            Err(error) => {
                row["error"] = json!(error.to_string());
                pass["reads"]
                    .as_array_mut()
                    .ok_or("missing read report")?
                    .push(row);
                return Err(error.into());
            }
        };
        let mut pcm = Sha256::new();
        let mut gain = Sha256::new();
        let mut nonzero_frames = 0_usize;
        let mut peak = 0.0_f32;
        for frame in &block.samples {
            nonzero_frames += usize::from(frame.iter().any(|sample| *sample != 0.0));
            for sample in frame {
                pcm.update(sample.to_le_bytes());
                peak = peak.max(sample.abs());
            }
        }
        for value in &block.gain {
            gain.update(value.to_le_bytes());
        }
        let fingerprint = (hex(pcm.finalize()), hex(gain.finalize()));
        let matches = reference
            .as_ref()
            .is_none_or(|expected| expected.get(hashes.len()) == Some(&fingerprint));
        row["returned_frames"] = json!(block.samples.len());
        row["nonzero_frames"] = json!(nonzero_frames);
        row["sample_peak"] = json!(peak);
        row["pcm_sha256_f32le_interleaved"] = json!(fingerprint.0);
        row["gain_sha256_f64le"] = json!(fingerprint.1);
        row["matches_first_pass"] = if reference.is_some() {
            json!(matches)
        } else {
            Value::Null
        };
        pass["reads"]
            .as_array_mut()
            .ok_or("missing read report")?
            .push(row);
        if !matches {
            return Err(
                "canonical PCM or limiter gain differs from the first fresh session".into(),
            );
        }
        hashes.push(fingerprint);
        start += i64::from(frames);
    }
    if reference.is_none() {
        *reference = Some(hashes);
    }
    Ok(())
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err("audio stream qualification exceeded its 120-second deadline".into());
    }
    Ok(())
}

fn hash_file(path: &Path, deadline: Instant) -> Result<String> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() > 512 * 1024 * 1024 {
        return Err("qualification executable exceeds the 512 MiB hash bound".into());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 64 * 1024];
    loop {
        check_deadline(deadline)?;
        let count = file.read(&mut bytes)?;
        if count == 0 {
            return Ok(hex(hash.finalize()));
        }
        hash.update(&bytes[..count]);
    }
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
