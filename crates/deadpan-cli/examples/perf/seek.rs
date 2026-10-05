//! Seek and frame-step latency through the committed project picture boundary
//! (store, plan, verified private source snapshot, persistent decoder) and the
//! shared Metal picture pipeline. Read-only.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::picture::{PreparedPicture, ProjectPictureSession, SourceAdmission};
use deadpan_core::{NodeKind, ProjectFrame};
use deadpan_media::source_session::{IndexMeasurement, interactive_decode_threads};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::gpu::Gpu;
use crate::{Lcg, Options, Result, ms, round, summary};

pub fn run(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let cold = options.number("cold", 10)?;
    let warm = options.number("warm", 200)?;
    let step = options.number("step", 240)?;
    let mut random = Lcg::new(options.number("seed", 3)?);
    let cancelled = AtomicBool::new(false);
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let revision = document.revision_id().clone();
    let basis = document.presentation_basis().clone();
    let gop = gop_summary(&store, &document)?;
    drop(store);
    let mut gpu = Gpu::new(basis.width, basis.height)?;

    // Cold: a fresh session (store, plan, private verified snapshot copy,
    // decoder open) to the first visible picture at a random frame. Complete
    // admission (export) measures the whole index first; progressive
    // admission (interactive preview) serves receipt-verified pictures while
    // that measurement runs, and `verified_ms` records when it completes.
    let mut frames = 0;
    let mut cold_reports = serde_json::Map::new();
    for (name, admission) in [
        ("complete", SourceAdmission::Complete),
        ("progressive", SourceAdmission::Progressive),
    ] {
        let mut cold_open = Vec::new();
        let mut cold_first = Vec::new();
        let mut cold_total = Vec::new();
        let mut verified = Vec::new();
        let mut cold_stats = Vec::new();
        for _ in 0..cold {
            let started = Instant::now();
            let mut session = ProjectPictureSession::open_revision_with(
                package, &revision, None, admission, &cancelled,
            )?;
            cold_open.push(ms(started));
            frames = session.plan().duration().frames();
            let frame = ProjectFrame(i64::try_from(random.below(u64::try_from(frames)?))?);
            let prepared_at = Instant::now();
            let prepared = session.prepare(frame, &cancelled)?;
            gpu.present(&prepared)?;
            cold_first.push(ms(prepared_at));
            cold_total.push(ms(started));
            match session.wait_source_measured(Duration::from_secs(300)) {
                Some(IndexMeasurement::Verified) | None => verified.push(ms(started)),
                Some(other) => return Err(format!("index measurement: {other:?}").into()),
            }
            cold_stats.push(stats(&session));
        }
        cold_reports.insert(
            name.into(),
            json!({
                "session_open_ms": summary(&cold_open),
                "first_picture_ms": summary(&cold_first),
                "total_ms": summary(&cold_total),
                "verified_ms": summary(&verified),
                "session_stats": cold_stats,
            }),
        );
    }

    // Warm: one open preview session, random non-adjacent frames (long-GOP
    // seeks), sampled after its background measurement has completed.
    let mut session = ProjectPictureSession::open_revision_with(
        package,
        &revision,
        None,
        SourceAdmission::Progressive,
        &cancelled,
    )?;
    session.prepare(ProjectFrame(0), &cancelled)?;
    // What a user sees first: random seeks while the background measurement
    // still runs (at most `measuring` samples, stopping when it finishes).
    let measuring_limit = options.number("measuring", 100)?;
    let mut measuring = Vec::new();
    let mut previous = 0_i64;
    while measuring.len() < usize::try_from(measuring_limit)?
        && session.source_measurement() == Some(IndexMeasurement::Measuring)
    {
        let mut frame = i64::try_from(random.below(u64::try_from(frames)?))?;
        if (frame - previous).abs() <= 1 {
            frame = (frame + frames / 2) % frames;
        }
        previous = frame;
        let started = Instant::now();
        let prepared = session.prepare(ProjectFrame(frame), &cancelled)?;
        gpu.present(&prepared)?;
        measuring.push(ms(started));
    }
    let measured_after = session.source_measurement();
    if !matches!(
        session.wait_source_measured(Duration::from_secs(300)),
        Some(IndexMeasurement::Verified) | None
    ) {
        return Err("warm session index measurement did not verify".into());
    }
    let mut decode = Vec::new();
    let mut submit = Vec::new();
    let mut complete = Vec::new();
    let mut total = Vec::new();
    let mut slow = Vec::new();
    previous = 0;
    for _ in 0..warm {
        let mut frame = i64::try_from(random.below(u64::try_from(frames)?))?;
        if (frame - previous).abs() <= 1 {
            frame = (frame + frames / 2) % frames;
        }
        previous = frame;
        let started = Instant::now();
        let prepared = session.prepare(ProjectFrame(frame), &cancelled)?;
        let decoded = ms(started);
        let presented = gpu.present(&prepared)?;
        let elapsed = ms(started);
        decode.push(decoded);
        submit.push(presented.submit_ms);
        complete.push(presented.complete_ms);
        total.push(elapsed);
        if elapsed >= 80.0 {
            slow.push(json!({"frame": frame, "total_ms": round(elapsed), "source": source(&prepared.picture)}));
        }
    }

    // Stepping: consecutive frames from a random start, one picture each.
    let start =
        i64::try_from(random.below(u64::try_from((frames - i64::try_from(step)?).max(1))?))?;
    session.prepare(ProjectFrame(start), &cancelled)?;
    let mut stepping = Vec::new();
    for offset in 1..=i64::try_from(step)? {
        let frame = start + offset;
        if frame >= frames {
            break;
        }
        let started = Instant::now();
        let prepared = session.prepare(ProjectFrame(frame), &cancelled)?;
        gpu.present(&prepared)?;
        stepping.push(ms(started));
    }
    let stepping_sum: f64 = stepping.iter().sum();
    let warm_stats = stats(&session);
    drop(session);
    // Preview proxy seeks, building the proxy into a private cache root.
    let proxy = match (options.text("proxy-cache"), options.text("worker")) {
        (Some(root), Some(worker)) => crate::proxy::run(
            package,
            &deadpan_cli::proxy::cache::ProxyCache::at(std::path::Path::new(root))?,
            std::path::Path::new(worker),
            &mut gpu,
            crate::proxy::Samples {
                cold,
                warm,
                refine: (warm / 4).max(10),
                seed: options.number("seed", 3)? + 1,
            },
        )?,
        _ => json!({"status": "not_measured"}),
    };
    Ok(json!({
        "proxy": proxy,
        "package": package,
        "adapter": gpu.adapter,
        "canvas": [basis.width, basis.height],
        "frame_rate": basis.frame_rate,
        "project_frames": frames,
        "source_gop": gop,
        "decode_threads": interactive_decode_threads(),
        "cold_state": "session-cold, page-cache-warm: each sample opens a new picture session (verified private snapshot copy, SHA-256, decoder; complete admission also measures the full index first) but the OS file cache is not purged (that needs root), so only the first sample can include disk reads",
        "cold": cold_reports,
        "measuring_seek": {
            "total_ms": summary(&measuring),
            "still_measuring_after_samples": measured_after == Some(IndexMeasurement::Measuring),
        },
        "warm_session_stats": warm_stats,
        "warm_seek": {
            "decode_ms": summary(&decode),
            "gpu_submit_ms": summary(&submit),
            "gpu_complete_ms": summary(&complete),
            "total_ms": summary(&total),
            "at_or_over_80ms": slow,
        },
        "frame_step": {
            "total_ms": summary(&stepping),
            "sustained_fps": if stepping_sum > 0.0 { round(stepping.len() as f64 * 1000.0 / stepping_sum) } else { 0.0 },
        },
    }))
}

/// The session's own decoder-cache counters (`ProjectPictureSession::stats`).
fn stats(session: &ProjectPictureSession) -> Value {
    let stats = session.stats();
    json!({
        "source_opens": stats.source_opens,
        "source_open_ms": round(stats.source_open_us as f64 / 1000.0),
        "source_reuses": stats.source_reuses,
        "decoded_frames": stats.decoded_frames,
        "background_frames": stats.background_frames,
        "decoder_reuse_rate": if stats.source_opens + stats.source_reuses > 0 {
            round(stats.source_reuses as f64 / (stats.source_opens + stats.source_reuses) as f64)
        } else { 0.0 },
    })
}

fn source(picture: &PreparedPicture) -> Value {
    match picture {
        PreparedPicture::Frame { id, .. } => json!({"original_frame": id.0}),
        PreparedPicture::Generated { id, .. } => json!({"generated_frame": id.0}),
        PreparedPicture::Background => json!("background"),
    }
}

/// Keyframe spacing of each picture asset; seek cost follows preroll length.
fn gop_summary(store: &ProjectStore, document: &deadpan_core::ProjectDocument) -> Result<Value> {
    let mut assets = Vec::new();
    for (asset, record) in document.assets() {
        if record.video.is_none() {
            continue;
        }
        let used = document
            .nodes()
            .values()
            .any(|node| matches!(&node.kind, NodeKind::Source { .. }));
        if !used {
            continue;
        }
        let index = store.source_video_index(document.revision_id(), asset)?;
        let keys: Vec<usize> = index
            .frames()
            .iter()
            .enumerate()
            .filter_map(|(ordinal, frame)| frame.keyframe.then_some(ordinal))
            .collect();
        let spacing: Vec<f64> = keys
            .windows(2)
            .map(|pair| (pair[1] - pair[0]) as f64)
            .collect();
        assets.push(json!({
            "asset": asset, "frames": index.frames().len(), "keyframes": keys.len(),
            "keyframe_spacing_frames": summary(&spacing),
        }));
    }
    Ok(json!(assets))
}
