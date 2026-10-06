//! Stress runs of the real main-viewer preview worker on a large package.
//! Ignored by default; run in release on a package copy, for example
//!
//! ```sh
//! DEADPAN_STRESS_PACKAGE=/tmp/copy.deadpan \
//! DEADPAN_STRESS_PROXY_CACHE=/tmp/proxies-gen-4k30 \
//! DEADPAN_STRESS_REPORT=/tmp/seek-storm.json \
//! cargo test --release --locked -p deadpan-app --bin deadpan-app seek_storm -- --ignored
//! ```
//!
//! `DEADPAN_STRESS_PROXY_CACHE` names a cache root with the package's
//! published proxy (`perf seek` or `perf proxy-build` builds one); without it
//! the worker serves only the Original. The storm submits random stopped
//! seeks faster than any picture decodes, then plays with periodic jumps
//! (transport-tagged requests at the picture rate), as held keys, a dragged
//! playhead and a jumpy edit would. It records replies by tier, the latency
//! of every reply, the preview queue's high-water mark and the process's
//! resident memory, and requires bounded queues and memory and an exact
//! picture after the storm.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command as Process;

use deadpan_cli::proxy::cache::ProxyCache;

use super::*;

fn resident_bytes() -> Option<u64> {
    let output = Process::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kib| kib * 1024)
}

fn summary(samples: &mut [f64]) -> serde_json::Value {
    if samples.is_empty() {
        return serde_json::json!({"n": 0});
    }
    samples.sort_by(f64::total_cmp);
    let rank =
        |p: f64| samples[((p * samples.len() as f64).ceil() as usize).clamp(1, samples.len()) - 1];
    serde_json::json!({
        "n": samples.len(),
        "p50": rank(0.5),
        "p95": rank(0.95),
        "max": samples[samples.len() - 1],
    })
}

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound.max(1)
    }
}

#[derive(Default)]
struct Tally {
    proxy: u64,
    exact: u64,
    errors: u64,
    latency_ms: Vec<f64>,
}

impl Tally {
    fn take(&mut self, worker: &PreviewWorker, submitted: &BTreeMap<u64, Instant>) {
        while let Some(reply) = worker.take_reply() {
            if let Some(at) = submitted.get(&reply.ticket.request) {
                self.latency_ms.push(at.elapsed().as_secs_f64() * 1000.0);
            }
            match reply.picture {
                Ok(picture) if picture.tier == PictureTier::Proxy => self.proxy += 1,
                Ok(_) => self.exact += 1,
                Err(_) => self.errors += 1,
            }
        }
    }

    fn report(mut self, submitted: u64) -> serde_json::Value {
        serde_json::json!({
            "submitted": submitted,
            "replies": {"proxy": self.proxy, "exact": self.exact, "errors": self.errors},
            "superseded_without_reply": submitted.saturating_sub(self.proxy + self.exact + self.errors),
            "submit_to_reply_ms": summary(&mut self.latency_ms),
        })
    }
}

fn wait_reply(worker: &PreviewWorker, timeout: Duration) -> Option<Reply> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(reply) = worker.take_reply() {
            return Some(reply);
        }
        std::thread::sleep(Duration::from_micros(500));
    }
    None
}

#[test]
#[ignore = "stress run on a large package; see the module documentation"]
fn seek_storm_and_jumpy_playback_stay_bounded() {
    let Ok(package) = std::env::var("DEADPAN_STRESS_PACKAGE") else {
        panic!("DEADPAN_STRESS_PACKAGE must name a package copy");
    };
    let package = PathBuf::from(package);
    let seconds: u64 = std::env::var("DEADPAN_STRESS_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10);
    let interval = Duration::from_millis(
        std::env::var("DEADPAN_STRESS_INTERVAL_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(4),
    );
    let store = ProjectStore::open(&package, deadpan_store::AccessMode::ReadWrite).unwrap();
    let workspace = workspace_of(&store, 1, package.clone());
    let frames = u64::try_from(workspace.plan.duration().frames()).unwrap();
    let rate = workspace.document.presentation_basis().frame_rate;
    let period = Duration::from_nanos(
        u64::from(rate.denominator()) * 1_000_000_000 / u64::from(rate.numerator()),
    );
    let worker = PreviewWorker::new(egui::Context::default()).unwrap();
    let cache = std::env::var("DEADPAN_STRESS_PROXY_CACHE")
        .ok()
        .map(|path| ProxyCache::at(Path::new(&path)).unwrap());
    worker.set_proxy_cache(cache.clone());
    let mut random = Lcg(17);
    let mut serial = 0_u64;
    let mut submit = |worker: &PreviewWorker, frame: u64, transport| {
        serial += 1;
        let mut request = request(&workspace, sequence(i64::try_from(frame).unwrap()), serial);
        request.ticket.transport = transport;
        worker.submit(request.ticket, request.work);
        serial
    };

    // Warm: the first picture opens the Original; later stopped seeks let
    // the idle worker open the proxy.
    let warm_started = Instant::now();
    submit(&worker, 0, None);
    wait_reply(&worker, Duration::from_secs(60)).expect("first picture");
    let mut proxy_open = false;
    if cache.is_some() {
        for _ in 0..40 {
            submit(&worker, random.below(frames), None);
            let reply = wait_reply(&worker, Duration::from_secs(10)).expect("warm seek");
            if reply
                .picture
                .as_ref()
                .is_ok_and(|p| p.tier == PictureTier::Proxy)
            {
                proxy_open = true;
                wait_reply(&worker, Duration::from_secs(10));
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let warm_ms = warm_started.elapsed().as_secs_f64() * 1000.0;
    std::thread::sleep(REFINE_DELAY * 2);
    while worker.take_reply().is_some() {}
    let resident_before = resident_bytes();
    let mut resident_high = resident_before.unwrap_or(0);

    // Stopped seek storm: a new random seek every `interval`.
    let mut submitted = BTreeMap::new();
    let mut tally = Tally::default();
    let storm_started = Instant::now();
    let mut next = Instant::now();
    let mut count = 0_u64;
    let mut sampled = Instant::now();
    while storm_started.elapsed() < Duration::from_secs(seconds) {
        let request = submit(&worker, random.below(frames), None);
        submitted.insert(request, Instant::now());
        count += 1;
        next += interval;
        while Instant::now() < next {
            tally.take(&worker, &submitted);
            std::thread::sleep(Duration::from_micros(250));
        }
        if sampled.elapsed() >= Duration::from_millis(500) {
            sampled = Instant::now();
            resident_high = resident_high.max(resident_bytes().unwrap_or(0));
        }
    }
    // The last seek's pictures after the storm: proxy (if open), then exact.
    let last = submit(&worker, random.below(frames), None);
    let settled = Instant::now();
    let mut first_ms = None;
    let mut exact_ms = None;
    while exact_ms.is_none() && settled.elapsed() < Duration::from_secs(10) {
        if let Some(reply) = worker.take_reply()
            && reply.ticket.request == last
        {
            let at = settled.elapsed().as_secs_f64() * 1000.0;
            first_ms.get_or_insert(at);
            if reply
                .picture
                .as_ref()
                .is_ok_and(|p| p.tier == PictureTier::Original)
            {
                exact_ms = Some(at);
            }
        }
        std::thread::sleep(Duration::from_micros(250));
    }
    let storm = tally.report(count);

    // Jumpy playback: transport-tagged requests at the picture rate, one in
    // flight (the newest frame wins), jumping to a random frame every half
    // second, as a YTP edit's cuts and repeats do.
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let generation = feed.restart(0).unwrap();
    let mut playback = Tally::default();
    let mut submitted = BTreeMap::new();
    let mut frame = random.below(frames);
    let play_started = Instant::now();
    let mut jumped = Instant::now();
    let mut requests = 0_u64;
    let mut in_flight = None;
    let mut skipped = 0_u64;
    let mut last_requested: Option<u64> = None;
    let mut tiers_by_jump: Vec<(u64, u64)> = Vec::new();
    while play_started.elapsed() < Duration::from_secs(seconds) {
        if jumped.elapsed() >= Duration::from_millis(500) {
            jumped = Instant::now();
            frame = random.below(frames.saturating_sub(64).max(1));
            tiers_by_jump.push((playback.proxy, playback.exact));
            last_requested = None;
        }
        let before = (playback.proxy, playback.exact, playback.errors);
        playback.take(&worker, &submitted);
        if (playback.proxy, playback.exact, playback.errors) != before {
            in_flight = None;
        }
        if in_flight.is_none() {
            let heard =
                frame + u64::try_from(jumped.elapsed().as_nanos() / period.as_nanos()).unwrap();
            let heard = heard.min(frames - 1);
            if last_requested != Some(heard) {
                if let Some(previous) = last_requested
                    && heard > previous + 1
                {
                    skipped += heard - previous - 1;
                }
                let request = submit(&worker, heard, Some(generation));
                submitted.insert(request, Instant::now());
                in_flight = Some(request);
                last_requested = Some(heard);
                requests += 1;
            }
        }
        if sampled.elapsed() >= Duration::from_millis(500) {
            sampled = Instant::now();
            resident_high = resident_high.max(resident_bytes().unwrap_or(0));
        }
        std::thread::sleep(Duration::from_micros(500));
    }
    worker.cancel();
    let resident_after = resident_bytes();
    let queue = deadpan_diagnostics::QUEUES.picture_preview.level();
    let pictures = deadpan_diagnostics::PLAYBACK_PICTURES.snapshot();
    let mut playback_report = playback.report(requests);
    playback_report["skipped_frames"] = serde_json::json!(skipped);
    playback_report["jumps"] = serde_json::json!(tiers_by_jump.len());
    let report = serde_json::json!({
        "package": package,
        "frames": frames,
        "canvas": [workspace.document.presentation_basis().width, workspace.document.presentation_basis().height],
        "proxy_open": proxy_open,
        "warm_ms": warm_ms,
        "storm": {
            "seconds": seconds,
            "interval_ms": interval.as_secs_f64() * 1000.0,
            "replies": storm,
            "after_storm_first_picture_ms": first_ms,
            "after_storm_exact_picture_ms": exact_ms,
        },
        "jumpy_playback": playback_report,
        "worker_playback_counters": {
            "exact": pictures.exact,
            "proxy": pictures.reduced,
            "repositions": pictures.repositions,
            "repositions_reached": pictures.repositions_reached,
        },
        "picture_preview_queue": {"current": queue.current, "high": queue.high},
        "resident_bytes": {
            "before_storm": resident_before,
            "high": resident_high,
            "after": resident_after,
        },
    });
    if let Ok(path) = std::env::var("DEADPAN_STRESS_REPORT") {
        std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
    worker.shutdown();
    // One pending plus one in progress: the mailbox never queues more.
    assert!(queue.high <= 2, "preview queue high-water {}", queue.high);
    assert!(
        exact_ms.is_some(),
        "the exact picture must follow the storm's last seek"
    );
    if let (Some(before), Some(after)) = (resident_before, resident_after) {
        assert!(
            after < before + 1024 * 1024 * 1024,
            "resident memory grew from {before} to {after} bytes"
        );
    }
}
