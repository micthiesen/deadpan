//! Real-device Sequence audition with pictures following the heard clock, as
//! the native editor schedules them: one picture in flight, the newest desired
//! frame wins, and a frame the decoder could not reach before the heard clock
//! moved past it is dropped (skipped).
//!
//! Pictures come from the Original through a progressively admitted
//! interactive decoder, as in the native viewer. With `--pictures adaptive`
//! (and `--proxy-cache NEW_DIR --worker MEDIA_WORKER`) each picture's tier is
//! chosen as the native preview worker chooses it during playback
//! (`deadpan_media::playback_pictures`): the exact Original picture when its
//! measured decode cost fits the picture budget, otherwise the verified
//! preview proxy picture, while the Original decoder repositions ahead in
//! bounded steps between pictures. `--pictures original` never uses a proxy.
//! Opens PACKAGE writable (the engine needs the original import handle), so
//! run it on a copy. Monitor gain is low; timing does not depend on it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use deadpan_cli::picture::{render_layers, source_to_render_frame};
use deadpan_cli::proxy::cache::ProxyCache;
use deadpan_core::{AssetId, AudioSample, MIX_SAMPLE_RATE, ProjectFrame, SourceFrameId};
use deadpan_media::playback_pictures::{PictureClock, PictureSource, PlaybackPictures};
use deadpan_media::source_session::{RepositionProgress, SourceSession, SourceSessionLimits};
use deadpan_plan::{Picture, RenderPlan};
use deadpan_playback::{Engine, Phase, Snapshot, SourceEntry};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::gpu::Gpu;
use crate::{Options, Result, ms, round, summary};

const MONITOR_GAIN: f32 = 0.05;
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);
/// Reposition work between heard-clock polls, standing in for the native
/// worker's yield when the next picture request arrives.
const REPOSITION_SLICE: Duration = Duration::from_millis(2);

pub fn run(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let seconds = options.number("seconds", 30)?;
    let start_frame = i64::try_from(options.number("start-frame", 0)?)?;
    let policy = options.text("pictures").unwrap_or("original");
    if !matches!(policy, "original" | "adaptive") {
        return Err("--pictures must be original or adaptive".into());
    }
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

    let opened = Instant::now();
    let mut pictures = Pictures::open(package, &store, policy == "adaptive", options)?;
    let open_ms = ms(opened);
    let mut gpu = Gpu::new(basis.width, basis.height)?;
    let frames = pictures.plan.duration().frames();
    // Warm the picture decoder as the editor's stopped picture already has.
    pictures.present(&mut gpu, ProjectFrame(start_frame), false)?;

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
    let mut tiers = Tiers::default();
    let mut presented_at: Vec<f64> = Vec::new();
    let mut lag_frames = Vec::new();
    // An update polled after a picture, handled at the top of the loop so
    // no phase change or terminal state is lost.
    let mut pending = None;
    let limit = Duration::from_secs(seconds);
    let frame_of = |sample: AudioSample| -> Result<i64> {
        Ok(i64::try_from(
            i128::from(sample.0) * i128::from(rate.numerator())
                / (i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator())),
        )?)
    };
    loop {
        if pending.is_some() {
            // Handle the update polled after the last picture first.
        } else if pictures.repositioning() {
            // Reposition between pictures instead of sleeping.
            pictures.reposition_slice(REPOSITION_SLICE);
        } else {
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
        if let Some(update) = pending.take().or_else(|| engine.poll()) {
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
        let frame = frame_of(sample)?.min(frames - 1);
        if last_frame == Some(frame) {
            continue;
        }
        if let Some(previous) = last_frame
            && frame > previous + 1
        {
            dropped += u64::try_from(frame - previous - 1)?;
        }
        let started = Instant::now();
        let shown = pictures.present(&mut gpu, ProjectFrame(frame), true)?;
        picture_ms.push(ms(started));
        presented_at.push(ms(requested));
        tiers.record(shown, ms(requested));
        // How far the heard clock had moved on when this picture completed.
        // The update itself is handled at the top of the loop.
        pending = engine.poll();
        if let Some(now) = pending.as_ref().and_then(|update| update.sample).or(heard) {
            lag_frames.push((frame_of(now)?.min(frames - 1) - frame) as f64);
        }
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
    // A stop shows the exact picture: what the viewer refines to after a
    // proxy picture. Measured from the stop, decode plus Metal completion.
    let exact_on_stop = match last_frame {
        Some(frame) => {
            let started = Instant::now();
            pictures.present(&mut gpu, ProjectFrame(frame), false)?;
            Some(round(ms(started)))
        }
        None => None,
    };
    let diagnostics = engine.diagnostics();
    engine.shutdown();
    let queues: serde_json::Map<String, Value> = deadpan_diagnostics::snapshot()
        .queues
        .named()
        .into_iter()
        .filter(|(name, _)| name.starts_with("playback"))
        .map(|(name, level)| {
            (
                name.to_owned(),
                json!({"current": level.current, "high": level.high}),
            )
        })
        .collect();
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
    let intervals: Vec<f64> = presented_at.windows(2).map(|w| w[1] - w[0]).collect();
    Ok(json!({
        "package": package,
        "adapter": gpu.adapter,
        "canvas": [basis.width, basis.height],
        "frame_rate": rate,
        "monitor_gain": MONITOR_GAIN,
        "requested_seconds": seconds,
        "start_frame": start_frame,
        "picture_policy": policy,
        "picture_session_open_ms": round(open_ms),
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
            "queue_depths": queues,
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
            "presentation_interval_ms": summary(&intervals),
            "lag_frames_at_completion": summary(&lag_frames),
            "tiers": tiers.report(),
            "exact_on_stop_ms": exact_on_stop,
            "proxy": pictures.proxy_report.clone(),
            "repositions": {
                "started": pictures.repositions,
                "reached": pictures.repositions_reached,
            },
        },
    }))
}

/// Which pixels one presented picture showed and what it cost.
#[derive(Clone, Copy)]
struct Shown {
    tier: PictureSource,
    background: bool,
    /// Decode and conversion before GPU submission.
    decode_ms: f64,
}

#[derive(Default)]
struct Tiers {
    exact: Vec<f64>,
    reduced: Vec<f64>,
    background: u64,
    /// Consecutive proxy pictures and how long they lasted, from the first
    /// proxy picture's presentation to the next exact picture's.
    runs: Vec<f64>,
    run_lengths: Vec<f64>,
    run: Option<(f64, u64)>,
}

impl Tiers {
    fn record(&mut self, shown: Shown, at_ms: f64) {
        if shown.background {
            self.background += 1;
            return;
        }
        match shown.tier {
            PictureSource::Exact => {
                self.exact.push(shown.decode_ms);
                if let Some((started, pictures)) = self.run.take() {
                    self.runs.push(at_ms - started);
                    self.run_lengths.push(pictures as f64);
                }
            }
            PictureSource::Reduced => {
                self.reduced.push(shown.decode_ms);
                match &mut self.run {
                    Some((_, pictures)) => *pictures += 1,
                    None => self.run = Some((at_ms, 1)),
                }
            }
        }
    }

    fn report(&self) -> Value {
        json!({
            "exact": self.exact.len(),
            "proxy": self.reduced.len(),
            "background": self.background,
            "exact_decode_ms": summary(&self.exact),
            "proxy_decode_ms": summary(&self.reduced),
            "proxy_runs_to_exact_ms": summary(&self.runs),
            "proxy_run_pictures": summary(&self.run_lengths),
            "unfinished_proxy_run_pictures": self.run.map(|(_, pictures)| pictures),
        })
    }
}

/// The single Original's interactive decoder, its optional proxy and the
/// shared tier policy. A project without an Original (Background Holds)
/// shows only Background pictures.
struct Pictures {
    plan: RenderPlan,
    canvas: [u32; 2],
    picture_period: Duration,
    source: Option<Source>,
    proxy_report: Value,
    costs: PlaybackPictures,
    reposition: Option<SourceFrameId>,
    repositions: u64,
    repositions_reached: u64,
    cancelled: AtomicBool,
}

struct Source {
    asset: AssetId,
    original: SourceSession,
    proxy: Option<SourceSession>,
}

impl Pictures {
    fn open(
        package: &Path,
        store: &ProjectStore,
        adaptive: bool,
        options: &Options,
    ) -> Result<Self> {
        let cancelled = AtomicBool::new(false);
        let document = store.snapshot()?;
        let basis = document.presentation_basis();
        let plan = RenderPlan::compile(&document)?;
        let has_picture = document
            .assets()
            .values()
            .any(|record| record.video.is_some());
        let mut proxy_report = json!("not_used");
        let source = if has_picture {
            let (_reader, subject) = crate::proxy::subject(package)?;
            let video = &subject.video;
            let limits = SourceSessionLimits::interactive_for(
                video.interpretation().width,
                video.interpretation().height,
            );
            let snapshot = store.snapshot_original(
                subject.original.object().content(),
                OriginalMediaLimits::new(limits.decode.max_input_bytes, limits.opening_timeout)
                    .map_err(deadpan_store::StoreError::from)?,
                &cancelled,
            )?;
            let input = snapshot.into_source_input(video.index().content())?;
            let original = SourceSession::open_admitted_input(
                input,
                Arc::new(video.index().for_asset(subject.asset.clone())?),
                video.interpretation(),
                limits,
                &cancelled,
            )?;
            let proxy = if adaptive {
                let cache = ProxyCache::at(Path::new(
                    options
                        .text("proxy-cache")
                        .ok_or("--pictures adaptive needs --proxy-cache")?,
                ))?;
                let worker = Path::new(
                    options
                        .text("worker")
                        .ok_or("--pictures adaptive needs --worker")?,
                );
                let (status, build_ms) = crate::proxy::build(package, &cache, worker)?;
                proxy_report = json!({"status": status, "build_ms": round(build_ms)});
                if status == "ready" {
                    Some(crate::proxy::open(&cache, &subject, &cancelled)?)
                } else {
                    None
                }
            } else {
                None
            };
            Some(Source {
                asset: subject.asset,
                original,
                proxy,
            })
        } else {
            None
        };
        let period = Duration::from_nanos(u64::try_from(
            u128::from(basis.frame_rate.denominator()) * 1_000_000_000
                / u128::from(basis.frame_rate.numerator()),
        )?);
        Ok(Self {
            plan,
            canvas: [basis.width, basis.height],
            picture_period: period,
            source,
            proxy_report,
            costs: PlaybackPictures::default(),
            reposition: None,
            repositions: 0,
            repositions_reached: 0,
            cancelled,
        })
    }

    fn repositioning(&self) -> bool {
        self.reposition.is_some()
            || self
                .source
                .as_ref()
                .is_some_and(|source| source.original.repositioning().is_some())
    }

    /// Present project `frame`. `playing` lets the tier policy choose the
    /// proxy; a stopped picture is always exact.
    fn present(&mut self, gpu: &mut Gpu, frame: ProjectFrame, playing: bool) -> Result<Shown> {
        let sample = self.plan.picture(frame)?;
        let source = match (&sample.picture, self.source.as_mut()) {
            (Picture::Source { asset, .. } | Picture::Freeze { asset, .. }, Some(source))
                if *asset == source.asset =>
            {
                source
            }
            (Picture::Blank | Picture::Background, _) => {
                gpu.present_background()?;
                return Ok(Shown {
                    tier: PictureSource::Exact,
                    background: true,
                    decode_ms: 0.0,
                });
            }
            _ => return Err("perf playback shows only the Original and Background".into()),
        };
        let layers = render_layers(&sample.framing, sample.gap_after.is_some())?;
        let id = sample
            .picture
            .select_source_frame(source.original.index().index())?
            .identity;
        let ordinal_period = picture_duration(source.original.index().index(), id)
            .ok_or("Original picture has no duration")?;
        let clock = PictureClock {
            picture_period: self.picture_period,
            ordinal_period,
        };
        let plan = source.original.decode_plan(id);
        let tier = if playing {
            self.costs.choose(plan, clock, source.proxy.is_some())
        } else {
            PictureSource::Exact
        };
        let started = Instant::now();
        let (rgba, shown) = match (tier, source.proxy.as_mut()) {
            (PictureSource::Reduced, Some(proxy)) => {
                let decoded = proxy.frame(id, FRAME_TIMEOUT, &self.cancelled)?;
                let rgba = source_to_render_frame(decoded, proxy.info())?;
                self.costs.record_reduced(started.elapsed());
                let ahead = PlaybackPictures::ahead(
                    id,
                    source.original.current(),
                    source.original.repositioning(),
                );
                self.reposition =
                    self.costs
                        .reposition_target(source.original.index().index(), id, clock, ahead);
                (rgba, PictureSource::Reduced)
            }
            _ => {
                let reopening = source.original.reopening();
                let decoded = source.original.frame(id, FRAME_TIMEOUT, &self.cancelled)?;
                let rgba = source_to_render_frame(decoded, source.original.info())?;
                if !reopening {
                    self.costs.record_exact(plan, started.elapsed());
                }
                self.reposition = None;
                (rgba, PictureSource::Exact)
            }
        };
        let decode_ms = ms(started);
        gpu.present_frame(&rgba, self.canvas, &layers)?;
        Ok(Shown {
            tier: shown,
            background: false,
            decode_ms,
        })
    }

    /// At most `slice` of reposition work, as the native worker does between
    /// picture requests.
    fn reposition_slice(&mut self, slice: Duration) {
        let Some(source) = self.source.as_mut() else {
            return;
        };
        let never = AtomicBool::new(false);
        if let Some(target) = self.reposition.take() {
            if source
                .original
                .begin_reposition(target, FRAME_TIMEOUT, &never)
                .is_err()
            {
                return;
            }
            self.repositions += 1;
        }
        if source.original.repositioning().is_none() {
            return;
        }
        let started = Instant::now();
        let deadline = started + slice;
        match source
            .original
            .advance_reposition(FRAME_TIMEOUT, &never, &mut || Instant::now() >= deadline)
        {
            Ok(RepositionProgress::Pending { ordinals }) => {
                self.costs.record_reposition(ordinals, started.elapsed());
            }
            Ok(RepositionProgress::Reached { ordinals }) => {
                self.costs.record_reposition(ordinals, started.elapsed());
                self.repositions_reached += 1;
            }
            Err(_) => {}
        }
    }
}

fn picture_duration(index: &deadpan_core::SourceFrameIndex, id: SourceFrameId) -> Option<Duration> {
    let frames = index.frames();
    let position = usize::try_from(id.0).ok()?;
    let start = frames.get(position)?.pts;
    let end = frames
        .get(position + 1)
        .map_or(index.terminal_end(), |next| next.pts);
    let ticks = u64::try_from(end.checked_sub(start)?).ok()?;
    let base = index.time_base();
    let nanos = u128::from(ticks) * u128::from(base.numerator()) * 1_000_000_000
        / u128::from(base.denominator());
    Some(Duration::from_nanos(u64::try_from(nanos).ok()?)).filter(|d| !d.is_zero())
}
