//! Real-device Sequence audition with pictures following the heard clock, as
//! the native editor schedules them: one picture in flight, the newest desired
//! frame wins, and a frame the decoder could not reach before the heard clock
//! moved past it is dropped (skipped).
//! Opens PACKAGE writable (the engine needs the original import handle), so
//! run it on a copy. Monitor gain is low; timing does not depend on it.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use deadpan_cli::picture::ProjectPictureSession;
use deadpan_core::{AudioSample, MIX_SAMPLE_RATE, ProjectFrame};
use deadpan_playback::{Engine, Phase, Snapshot, SourceEntry};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::gpu::Gpu;
use crate::{Options, Result, ms, round, summary};

const MONITOR_GAIN: f32 = 0.05;

pub fn run(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let seconds = options.number("seconds", 30)?;
    let start_frame = i64::try_from(options.number("start-frame", 0)?)?;
    let store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let document = Arc::new(store.snapshot()?);
    let rate = document.presentation_basis().frame_rate;
    let basis = document.presentation_basis().clone();
    let mut sources = BTreeMap::new();
    for asset in document.assets().keys() {
        let receipt = Arc::new(store.registered_source(document.revision_id(), asset)?);
        let original = store
            .original_record(receipt.original().content())?
            .ok_or("registered source has no original record")?;
        sources.insert(asset.clone(), SourceEntry { receipt, original });
    }
    let snapshot = Arc::new(Snapshot::committed(
        1,
        document.clone(),
        sources,
        store.original_import_handle()?,
    ));

    let cancelled = AtomicBool::new(false);
    let mut pictures =
        ProjectPictureSession::open_revision(package, document.revision_id(), None, &cancelled)?;
    let mut gpu = Gpu::new(basis.width, basis.height)?;
    let frames = pictures.plan().duration().frames();
    // Warm the picture decoder as the editor's stopped picture already has.
    gpu.present(&pictures.prepare(ProjectFrame(start_frame), &cancelled)?)?;

    let wake = Arc::new((Mutex::new(false), Condvar::new()));
    let notify = wake.clone();
    let engine = Engine::new(Arc::new(move || {
        let (flag, signal) = &*notify;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        signal.notify_all();
    }))?;
    let start_sample = rate.audio_boundary(ProjectFrame(start_frame))?;
    let requested = Instant::now();
    engine.play(1, snapshot, start_sample, MONITOR_GAIN)?;

    let mut first_sound_ms = None;
    let mut phases: Vec<Value> = Vec::new();
    let mut last_phase = None;
    let mut failure = None;
    let mut heard: Option<AudioSample> = None;
    let mut presented = 0_u64;
    let mut dropped = 0_u64;
    let mut last_frame: Option<i64> = None;
    let mut first_frame: Option<i64> = None;
    let mut terminal = None;
    let mut picture_ms = Vec::new();
    let limit = Duration::from_secs(seconds);
    loop {
        {
            let (flag, signal) = &*wake;
            let mut woken = flag.lock().unwrap_or_else(|e| e.into_inner());
            if !*woken {
                woken = signal
                    .wait_timeout(woken, Duration::from_millis(2))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            *woken = false;
        }
        if let Some(update) = engine.poll() {
            if last_phase != Some(update.phase) {
                phases.push(
                    json!({"phase": format!("{:?}", update.phase), "at_ms": round(ms(requested))}),
                );
                last_phase = Some(update.phase);
            }
            match update.phase {
                Phase::Playing => {
                    if first_sound_ms.is_none() && update.sample.is_some() {
                        first_sound_ms = Some(ms(requested));
                    }
                    heard = update.sample.or(heard);
                }
                Phase::Failed => {
                    failure = update.error.clone();
                    terminal = Some("Failed");
                    break;
                }
                Phase::Ended => {
                    heard = update.sample.or(heard);
                    terminal = Some("Ended");
                    break;
                }
                Phase::Stopped => {
                    terminal = Some("Stopped");
                    break;
                }
                Phase::Preparing => {}
            }
        }
        if let Some(first) = first_sound_ms
            && ms(requested) - first >= limit.as_secs_f64() * 1000.0
        {
            break;
        }
        if first_sound_ms.is_none() && requested.elapsed() > Duration::from_secs(20) {
            failure = Some("no delivered audio within 20 s".into());
            break;
        }
        let Some(sample) = heard else { continue };
        // Desired picture: the frame containing the latest heard sample.
        let frame = i64::try_from(
            i128::from(sample.0) * i128::from(rate.numerator())
                / (i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator())),
        )?
        .min(frames - 1);
        if last_frame == Some(frame) {
            continue;
        }
        if let Some(previous) = last_frame
            && frame > previous + 1
        {
            dropped += u64::try_from(frame - previous - 1)?;
        }
        let started = Instant::now();
        gpu.present(&pictures.prepare(ProjectFrame(frame), &cancelled)?)?;
        picture_ms.push(ms(started));
        presented += 1;
        first_frame.get_or_insert(frame);
        last_frame = Some(frame);
    }
    let heard_seconds =
        heard.map(|sample| (sample.0 - start_sample.0) as f64 / f64::from(MIX_SAMPLE_RATE));
    engine.stop();
    let stop_requested = Instant::now();
    while stop_requested.elapsed() < Duration::from_secs(5) {
        if engine.poll().is_some_and(|update| {
            matches!(update.phase, Phase::Stopped | Phase::Failed | Phase::Ended)
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let diagnostics = engine.diagnostics();
    engine.shutdown();
    let expected_frames =
        heard_seconds.map(|s| s * f64::from(rate.numerator()) / f64::from(rate.denominator()));
    // Coverage: the audition must span the requested interval (or the rest of
    // a shorter project) and pictures must start at the start frame and reach
    // the heard end; a short or early-stopped run is not a sustained pass.
    let fps = f64::from(rate.numerator()) / f64::from(rate.denominator());
    let available_seconds = (frames - start_frame) as f64 / fps;
    let coverable_seconds = available_seconds.min(seconds as f64);
    let heard_end_frame = heard.map(|sample| {
        (i128::from(sample.0) * i128::from(rate.numerator())
            / (i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()))) as i64
    });
    let leading_missed = first_frame.map(|first| first - start_frame);
    let trailing_missed = heard_end_frame
        .zip(last_frame)
        .map(|(end, last)| (end.min(frames - 1) - last).max(0));
    let covered = heard_seconds.is_some_and(|heard| heard + 1.0 / fps >= coverable_seconds)
        && failure.is_none();
    Ok(json!({
        "package": package,
        "adapter": gpu.adapter,
        "canvas": [basis.width, basis.height],
        "frame_rate": rate,
        "monitor_gain": MONITOR_GAIN,
        "requested_seconds": seconds,
        "start_frame": start_frame,
        "playback_start_to_first_heard_ms": first_sound_ms.map(round),
        "heard_seconds": heard_seconds.map(round),
        "available_seconds": round(available_seconds),
        "coverable_seconds": round(coverable_seconds),
        "terminal_phase": terminal,
        "covered_requested_interval": covered,
        "phases": phases,
        "failure": failure,
        "audio": {
            "generations": diagnostics.generations,
            "device_reports": diagnostics.reports,
            "underruns_starved": diagnostics.starved,
            "device_faults": diagnostics.faults,
            "silent_padding_frames": diagnostics.silent_frames,
            "max_callback_render_cost_us": round(diagnostics.max_render_cost_ns as f64 / 1000.0),
        },
        "pictures": {
            "presented": presented,
            "dropped_skipped": dropped,
            "expected_frames": expected_frames.map(round),
            "first_presented_frame": first_frame,
            "last_presented_frame": last_frame,
            "heard_end_frame": heard_end_frame,
            "leading_missed": leading_missed,
            "trailing_missed": trailing_missed,
            "decode_and_gpu_ms": summary(&picture_ms),
        },
    }))
}
