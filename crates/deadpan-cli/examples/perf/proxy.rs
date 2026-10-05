//! Preview proxy seeks as the native viewer performs them: build and verify
//! the proxy into a private cache root, then measure proxy openings, cold
//! and warm proxy seeks to Metal completion, and the latency until the exact
//! Original picture replaces a proxy picture after the cursor rests.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::picture::{
    ProjectPictureSession, SourceAdmission, render_layers, source_to_render_frame,
};
use deadpan_cli::proxy::cache::ProxyCache;
use deadpan_cli::proxy::{BuildControl, ProxyStatus, build_proxy, open_proxy_file};
use deadpan_core::{ProjectFrame, SourceFrameId};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{IndexMeasurement, SourceSession, SourceSessionLimits};
use deadpan_plan::{Picture, RenderPlan};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::single_source::SingleSourceState;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::gpu::Gpu;
use crate::{Lcg, Result, ms, round, summary};

/// The native worker's rest period before refinement (`REFINE_DELAY`).
const REFINE_DELAY: Duration = Duration::from_millis(150);
/// The native worker's proxy decoder (`PROXY_SERVING_THREADS`).
const PROXY_SERVING_THREADS: u32 = 1;

pub struct Samples {
    pub cold: u64,
    pub warm: u64,
    pub refine: u64,
    pub seed: u64,
}

fn proxy_limits() -> SourceSessionLimits {
    let mut limits = SourceSessionLimits::interactive();
    limits.decode.threads = PROXY_SERVING_THREADS;
    limits
}

/// The project's Original record and qualified picture stream.
struct Subject {
    original: deadpan_store::original_media::OriginalMediaRecord,
    video: deadpan_media::source_qualification::QualifiedVideoSnapshot,
    asset: deadpan_core::AssetId,
    document: deadpan_core::ProjectDocument,
}

fn subject(package: &Path) -> Result<(ProjectStore, Subject)> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let revision = document.revision_id().clone();
    let asset = match store.single_source_state()? {
        Some(SingleSourceState::Ready { asset, .. }) => asset,
        _ => document
            .assets()
            .iter()
            .find(|(_, record)| record.video.is_some())
            .map(|(asset, _)| asset.clone())
            .ok_or("no picture asset")?,
    };
    let receipt = store.registered_source(&revision, &asset)?;
    let original = store
        .original_record(receipt.original().content())?
        .ok_or("original record missing")?;
    let video = receipt
        .snapshot()
        .video()
        .ok_or("the Original has no picture stream")?
        .clone();
    Ok((
        store,
        Subject {
            original,
            video,
            asset,
            document,
        },
    ))
}

/// Build (or find) the proxy of `package`'s Original in `cache`, with the
/// app's controls but no pause. Returns the build time.
pub fn build(package: &Path, cache: &ProxyCache, worker: &Path) -> Result<(Value, f64)> {
    let cancelled = AtomicBool::new(false);
    let (store, subject) = subject(package)?;
    let started = Instant::now();
    let original = subject.original.clone();
    let store_ref = &store;
    let status = build_proxy(
        cache,
        &subject.original,
        &subject.video,
        worker,
        Box::new(move |cancelled| {
            let snapshot = store_ref.snapshot_original(
                original.object().content(),
                OriginalMediaLimits::new(
                    original.object().byte_length().max(1),
                    Duration::from_secs(3600),
                )
                .map_err(deadpan_store::StoreError::from)?,
                cancelled,
            )?;
            Ok(Box::new(snapshot) as Box<dyn std::io::Read>)
        }),
        BuildControl::new(&cancelled),
    )?;
    let build_ms = ms(started);
    Ok((
        match status {
            ProxyStatus::Ready(_) => json!("ready"),
            ProxyStatus::NotNeeded => json!("not_needed"),
            ProxyStatus::Ineligible(reason) => json!({"ineligible": reason.to_string()}),
            ProxyStatus::Failed(message) => json!({"failed": message}),
            ProxyStatus::Missing(_) => json!("missing"),
        },
        build_ms,
    ))
}

/// `perf proxy-build PACKAGE --proxy-cache DIR --worker BIN`: one build,
/// for measuring its effect on concurrent playback and editing.
pub fn build_stage(options: &crate::Options) -> Result<Value> {
    let package = options.package()?;
    let cache = ProxyCache::at(Path::new(
        options
            .text("proxy-cache")
            .ok_or("--proxy-cache is required")?,
    ))?;
    let worker = Path::new(options.text("worker").ok_or("--worker is required")?);
    let (status, build_ms) = build(package, &cache, worker)?;
    Ok(json!({"status": status, "build_ms": round(build_ms)}))
}

/// Open the published proxy in place, as the native worker does.
fn open(cache: &ProxyCache, subject: &Subject, cancelled: &AtomicBool) -> Result<SourceSession> {
    let (file, sidecar) = open_proxy_file(cache, &subject.original, &subject.video, cancelled)?
        .ok_or("published proxy was not found")?;
    let input = VerifiedSourceInput::from_verified_file(file, sidecar.content())?;
    Ok(SourceSession::open_input_indexed(
        input,
        Arc::new(sidecar.index.clone()),
        &sidecar.info,
        proxy_limits(),
        cancelled,
    )?)
}

pub fn run(
    package: &Path,
    cache: &ProxyCache,
    worker: &Path,
    gpu: &mut Gpu,
    samples: Samples,
) -> Result<Value> {
    let cancelled = AtomicBool::new(false);
    let mut random = Lcg::new(samples.seed);
    let (status, build_ms) = build(package, cache, worker)?;
    if status != "ready" {
        return Ok(json!({"status": status}));
    }
    let (_store, subject) = subject(package)?;
    let entry = deadpan_cli::proxy::cached_proxy(cache, &subject.original, &subject.video)?
        .ok_or("published proxy was not found")?;
    let sidecar = entry.sidecar().clone();
    let document = &subject.document;
    let revision = document.revision_id().clone();
    let plan = RenderPlan::compile(document)?;
    let frames = plan.duration().frames();
    let basis = document.presentation_basis();
    // The receipt's index under the asset alias this revision uses.
    let aliased = subject.video.index().for_asset(subject.asset.clone())?;
    let view = View {
        plan: &plan,
        index: aliased.index(),
        canvas: [basis.width, basis.height],
        cancelled: &cancelled,
    };
    let random_frame = |random: &mut Lcg, previous: &mut i64| -> Result<i64> {
        let mut frame = i64::try_from(random.below(u64::try_from(frames)?))?;
        if (frame - *previous).abs() <= 1 {
            frame = (frame + frames / 2) % frames;
        }
        *previous = frame;
        Ok(frame)
    };

    // Opening: the first opening of a new entry state hashes the file once;
    // every later one checks only the recorded file state.
    let directory = cache.path().join(entry.key().directory());
    let _ = std::fs::remove_file(directory.join("verified.json"));
    let started = Instant::now();
    drop(open(cache, &subject, &cancelled)?);
    let first_open_ms = ms(started);
    let mut opens = Vec::new();
    for _ in 0..samples.cold.max(1) {
        let started = Instant::now();
        drop(open(cache, &subject, &cancelled)?);
        opens.push(ms(started));
    }

    // Cold: a new reader (lookup, sidecar validation against the receipt,
    // recorded-state check, decoder) to the first composed picture.
    let mut cold = Vec::new();
    let mut previous = 0;
    for _ in 0..samples.cold {
        let started = Instant::now();
        let mut session = open(cache, &subject, &cancelled)?;
        let frame = random_frame(&mut random, &mut previous)?;
        view.present(gpu, &mut session, frame)?;
        cold.push(ms(started));
    }

    // Warm: one reader, random non-adjacent seeks.
    let mut session = open(cache, &subject, &cancelled)?;
    let mut warm = Vec::new();
    let mut slow = Vec::new();
    for _ in 0..samples.warm {
        let frame = random_frame(&mut random, &mut previous)?;
        let elapsed = view.present(gpu, &mut session, frame)?;
        if elapsed >= 80.0 {
            slow.push(json!({"frame": frame, "total_ms": round(elapsed)}));
        }
        warm.push(elapsed);
    }

    // Refinement: a proxy seek, the worker's rest period, then the exact
    // Original picture from a warm, verified preview session.
    let mut exact = ProjectPictureSession::open_revision_with(
        package,
        &revision,
        None,
        SourceAdmission::Progressive,
        &cancelled,
    )?;
    exact.prepare(ProjectFrame(0), &cancelled)?;
    if !matches!(
        exact.wait_source_measured(Duration::from_secs(600)),
        Some(IndexMeasurement::Verified) | None
    ) {
        return Err("Original background measurement did not verify".into());
    }
    let mut proxy_visible = Vec::new();
    let mut refined = Vec::new();
    let mut exact_seek = Vec::new();
    for _ in 0..samples.refine {
        let frame = random_frame(&mut random, &mut previous)?;
        let started = Instant::now();
        proxy_visible.push(view.present(gpu, &mut session, frame)?);
        std::thread::sleep(REFINE_DELAY);
        let exact_started = Instant::now();
        let prepared = exact.prepare(ProjectFrame(frame), &cancelled)?;
        gpu.present(&prepared)?;
        exact_seek.push(ms(exact_started));
        refined.push(ms(started));
    }
    let original_bytes = subject.original.object().byte_length();
    Ok(json!({
        "status": "ready",
        "reason": sidecar.reason,
        "raster": [sidecar.info.width, sidecar.info.height],
        "original_raster": [subject.video.interpretation().width, subject.video.interpretation().height],
        "proxy_bytes": sidecar.file.byte_length,
        "original_bytes": original_bytes,
        "bytes_ratio": round(sidecar.file.byte_length as f64 / original_bytes as f64),
        "pictures": sidecar.index.index().frames().len(),
        "fidelity": {
            "samples": sidecar.fidelity.samples,
            "mean_abs_difference": sidecar.fidelity.mean(),
            "max_block_difference": sidecar.fidelity.max_block(),
            "bias": sidecar.fidelity.bias(),
        },
        "build_ms": round(build_ms),
        "serving_threads": PROXY_SERVING_THREADS,
        "first_open_ms": round(first_open_ms),
        "open_ms": summary(&opens),
        "cold_seek": {"total_ms": summary(&cold)},
        "warm_seek": {"total_ms": summary(&warm), "at_or_over_80ms": slow},
        "refinement": {
            "rest_ms": REFINE_DELAY.as_millis() as u64,
            "proxy_visible_ms": summary(&proxy_visible),
            "exact_seek_ms": summary(&exact_seek),
            "refined_ms": summary(&refined),
        },
    }))
}

struct View<'a> {
    plan: &'a RenderPlan,
    index: &'a deadpan_core::SourceFrameIndex,
    canvas: [u32; 2],
    cancelled: &'a AtomicBool,
}

impl View<'_> {
    /// The Original picture a project frame shows, selected as the preview
    /// worker selects it, decoded from the proxy and composed on Metal.
    fn present(&self, gpu: &mut Gpu, session: &mut SourceSession, frame: i64) -> Result<f64> {
        let started = Instant::now();
        let sample = self.plan.picture(ProjectFrame(frame))?;
        if !matches!(
            sample.picture,
            Picture::Source { .. } | Picture::Freeze { .. }
        ) {
            return Err("perf proxy fixtures show the Original throughout".into());
        }
        let id: SourceFrameId = sample.picture.select_source_frame(self.index)?.identity;
        let layers = render_layers(&sample.framing, sample.gap_after.is_some())?;
        let picture = session.frame(id, Duration::from_secs(15), self.cancelled)?;
        let info = session.info().clone();
        let rgba = source_to_render_frame(picture, &info)?;
        gpu.present_frame(&rgba, self.canvas, &layers)?;
        Ok(ms(started))
    }
}
