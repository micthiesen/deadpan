//! Resumable seek proxy builds through the real isolated media worker.
//!
//! A build encodes keyframe-aligned ranges and journals each one; these
//! tests interrupt builds by killing the worker, by killing the host
//! process, by cancelling, and by damaging the stored ranges or journal, and
//! show that the next build reuses exactly the intact recorded ranges,
//! encodes only the rest, publishes a proxy that verifies like any other and
//! removes its partial state. Every test uses a private cache root. Skips
//! with a printed reason when the built `deadpan-media-worker` is missing.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read};
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_cli::proxy::cache::{ProxyCache, ProxyEntry, ProxyKey};
use deadpan_cli::proxy::{
    BuildControl, ProxyBuildError, ProxyProgress, ProxyProgressSnapshot, ProxySubject,
    build_subject,
};
use deadpan_core::AssetId;
use deadpan_media::proxy::{ProxyOriginal, ProxyPlan, ProxyReason, hex, proxy_raster};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::SourceStreamInfo;
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// 120 pictures with a keyframe every 15: eight ranges at this target.
const FIXTURE: &str = "cfr-bframes.mp4";
const TARGET: u64 = 15;
const RANGES: u64 = 8;
const CHILD: &str = "DEADPAN_PROXY_RESUME_CHILD";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(FIXTURE)
}

/// The isolated media worker of this checkout's build.
fn media_worker() -> Option<PathBuf> {
    if let Some(worker) = std::env::var_os("DEADPAN_MEDIA_WORKER") {
        return Some(PathBuf::from(worker)).filter(|worker| worker.is_file());
    }
    let targets = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain([Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target")]);
    targets
        .map(|target| target.join("debug/deadpan-media-worker"))
        .find(|worker| worker.is_file())
}

/// One VideoToolbox proxy session at a time across test processes, shared
/// with the media worker's real-media proxy suite (`VT_HANG` there).
fn vt_slot() -> File {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(std::env::temp_dir().join("deadpan-proxy-tests-videotoolbox.lock"))
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
    file
}

struct Original {
    bytes: Vec<u8>,
    index: SourceIndexSnapshot,
    info: SourceStreamInfo,
    identity: ProxyOriginal,
}

impl Original {
    fn load() -> Result<Self> {
        Self::load_from(&fixture())
    }

    fn load_from(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let content = SourceContentIdentity::new(digest, bytes.len() as u64)?;
        let cancelled = AtomicBool::new(false);
        let input = VerifiedSourceInput::copy_verified(
            &mut bytes.as_slice(),
            content,
            1 << 30,
            Duration::from_secs(60),
            &cancelled,
        )?;
        let session = SourceSession::open_input(
            input,
            AssetId::new("original")?,
            SourceSessionLimits::default(),
            &cancelled,
        )?;
        let index = session.index().clone();
        let identity = ProxyOriginal {
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            sha256: hex(&digest),
            byte_length: bytes.len() as u64,
            stream_index: index.stream_index(),
        };
        Ok(Self {
            info: session.info().clone(),
            bytes,
            index,
            identity,
        })
    }

    fn subject(&self) -> ProxySubject<'_> {
        ProxySubject {
            identity: self.identity.clone(),
            index: &self.index,
            info: &self.info,
        }
    }

    fn key(&self) -> ProxyKey {
        self.subject().key().unwrap()
    }

    fn plan(&self) -> ProxyPlan {
        let (width, height) = proxy_raster(self.info.width, self.info.height);
        ProxyPlan {
            width,
            height,
            reason: ProxyReason::Requested,
        }
    }

    fn pictures(&self) -> u64 {
        self.index.index().frames().len() as u64
    }

    /// Build through the production path into `cache`.
    fn build(
        &self,
        cache: &ProxyCache,
        worker: &Path,
        cancelled: &AtomicBool,
        progress: &ProxyProgress,
        target: u64,
    ) -> std::result::Result<ProxyEntry, ProxyBuildError> {
        let bytes = self.bytes.clone();
        build_subject(
            cache,
            &self.subject(),
            worker,
            self.plan(),
            Box::new(move |_| Ok(Box::new(Cursor::new(bytes)) as Box<dyn Read>)),
            BuildControl {
                progress: Some(progress),
                segment_pictures: target,
                stall: Duration::from_secs(20),
                ..BuildControl::new(cancelled)
            },
        )
    }

    /// The published proxy verifies: its bytes hash as recorded, every
    /// picture is a keyframe at exactly its Original picture's time and
    /// duration, and the entry's sidecar names these Original bytes.
    fn check_published(&self, cache: &ProxyCache, entry: &ProxyEntry) {
        let sidecar = entry.sidecar();
        sidecar
            .validate_for(&self.identity, self.index.index(), &self.info)
            .unwrap();
        cache.open(entry, &AtomicBool::new(false)).unwrap();
        let proxy = sidecar.index.index();
        assert_eq!(proxy.frames().len(), self.index.index().frames().len());
        for (proxy, original) in proxy.frames().iter().zip(self.index.index().frames()) {
            assert!(proxy.keyframe);
            // The proxy's clock is the Original's time-base denominator.
            assert_eq!(proxy.pts, original.pts);
            assert_eq!(proxy.reported_duration, original.reported_duration);
        }
        assert_eq!(proxy.terminal_end(), self.index.index().terminal_end());
    }
}

/// `key`'s partial state directory under `root`.
fn partial_directory(root: &Path, key: &ProxyKey) -> PathBuf {
    root.join(".partial").join(key.directory())
}

fn journal(root: &Path, key: &ProxyKey) -> Option<serde_json::Value> {
    let bytes = std::fs::read(partial_directory(root, key).join("ranges.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn journaled(root: &Path, key: &ProxyKey) -> usize {
    journal(root, key).map_or(0, |journal| journal["ranges"].as_array().unwrap().len())
}

/// A stand-in worker recording each run's process ID in `pids` (the shell
/// `exec`s, so it is the worker's own). It runs the real worker, except that
/// run `fail_on` reports a non-retryable media failure.
struct Stub {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl Stub {
    fn new(worker: &Path, fail_on: Option<u32>) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().display().to_string();
        let fail = fail_on.map_or(String::new(), |run| {
            format!(
                "if [ \"$run\" -eq {run} ]; then printf '{{\"status\":\"failure\",\"code\":\"invalid_media\",\"message\":\"stub refused the range\"}}\\n' >&2; exit 1; fi\n"
            )
        });
        let script = format!(
            "#!/bin/sh\necho $$ >> {root}/pids\nrun=$(/usr/bin/wc -l < {root}/pids)\n{fail}exec {} \"$@\"\n",
            worker.display()
        );
        let path = directory.path().join("worker.sh");
        std::fs::write(&path, script)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        Ok(Self { directory, path })
    }

    fn pids(&self) -> Vec<String> {
        std::fs::read_to_string(self.directory.path().join("pids"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn kill(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-KILL", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Wait until a worker run started after at least one range was journaled
/// is alive, and SIGKILL it. Returns its process ID.
fn kill_a_running_worker(stub: &Stub, root: &Path, key: &ProxyKey) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        assert!(Instant::now() < deadline, "no worker to kill");
        let done = journaled(root, key);
        let pids = stub.pids();
        if done >= 1
            && pids.len() > done
            && let Some(pid) = pids.last()
            && alive(pid)
            && kill(pid)
        {
            return pid.clone();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Run a build that is cancelled as soon as it has decided what to reuse,
/// and return that decision.
fn reuse_decision(
    original: &Original,
    cache: &ProxyCache,
    worker: &Path,
    target: u64,
) -> ProxyProgressSnapshot {
    let cancelled = AtomicBool::new(false);
    let progress = ProxyProgress::default();
    std::thread::scope(|scope| {
        let build = scope.spawn(|| original.build(cache, worker, &cancelled, &progress, target));
        while progress.snapshot().ranges == 0 && !build.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        cancelled.store(true, Ordering::Release);
        let result = build.join().unwrap();
        assert!(
            result.as_ref().is_err_and(ProxyBuildError::is_cancellation),
            "{:?}",
            result.map(|_| ())
        );
    });
    progress.snapshot()
}

/// (1) A worker SIGKILLed mid-build ends the build as a machine condition:
/// nothing is remembered against the Original, the journaled ranges stay,
/// and the next build reuses them and encodes only the rest.
#[test]
fn a_killed_worker_leaves_completed_ranges_for_the_next_build() -> Result {
    let Some(worker) = media_worker() else {
        eprintln!(
            "skipping: needs a built deadpan-media-worker (cargo build -p deadpan-media-worker)"
        );
        return Ok(());
    };
    let _slot = vt_slot();
    let original = Original::load()?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("Proxies");
    let cache = ProxyCache::at(&root)?;
    let key = original.key();
    let stub = Stub::new(&worker, None)?;
    let cancelled = AtomicBool::new(false);
    let progress = ProxyProgress::default();
    let error = std::thread::scope(|scope| {
        let build =
            scope.spawn(|| original.build(&cache, &stub.path, &cancelled, &progress, TARGET));
        kill_a_running_worker(&stub, &root, &key);
        build.join().unwrap().map(|_| ()).unwrap_err()
    });
    assert!(
        matches!(
            &error,
            ProxyBuildError::Conversion(deadpan_media::ConversionError::Worker { code, .. })
                if code == deadpan_media::WORKER_TERMINATED
        ),
        "{error}"
    );
    assert!(error.is_environmental() && !error.is_cancellation());
    assert_eq!(
        cache.failure(&key),
        None,
        "a killed worker is not a verdict"
    );
    assert!(
        cache.lookup(&key)?.is_none(),
        "nothing partial is published"
    );
    let kept = journaled(&root, &key) as u64;
    assert!((1..RANGES).contains(&kept), "{kept}");
    let first = progress.snapshot();
    assert_eq!((first.ranges, first.reused_ranges), (RANGES, 0));

    // The next build: the real worker, the same cache.
    let progress = ProxyProgress::default();
    let entry = original.build(&cache, &worker, &AtomicBool::new(false), &progress, TARGET)?;
    let resumed = progress.snapshot();
    assert_eq!(resumed.reused_ranges, kept);
    assert_eq!(resumed.encoded_ranges, RANGES - kept);
    assert_eq!(
        resumed.reused_pictures + resumed.encoded_pictures,
        original.pictures()
    );
    assert_eq!(resumed.completed_pictures, original.pictures());
    original.check_published(&cache, &entry);
    assert!(
        !partial_directory(&root, &key).exists(),
        "published: partial removed"
    );
    assert_eq!(cache.partial_bytes(&key), None);
    Ok(())
}

/// The child half of [`a_killed_host_resumes_from_its_journal`]: when run
/// with [`CHILD`] set, build the fixture's proxy into the given cache root
/// with the given worker until the parent kills this process.
#[test]
fn host_process_for_kill_test() -> Result {
    let Some(setting) = std::env::var_os(CHILD) else {
        return Ok(());
    };
    let setting = setting.into_string().unwrap();
    let (root, worker) = setting.split_once('\n').unwrap();
    let original = Original::load()?;
    let cache = ProxyCache::at(Path::new(root))?;
    let result = original.build(
        &cache,
        Path::new(worker),
        &AtomicBool::new(false),
        &ProxyProgress::default(),
        TARGET,
    );
    panic!(
        "the parent should have killed this build: {:?}",
        result.map(|_| ())
    );
}

/// (2) The host process is SIGKILLed mid-build. Its worker notices the
/// closed control pipe and exits; the journal it left is resumed in a new
/// process, which reuses the recorded ranges, encodes the rest, publishes a
/// verified proxy and removes the partial state.
#[test]
fn a_killed_host_resumes_from_its_journal() -> Result {
    let Some(worker) = media_worker() else {
        eprintln!(
            "skipping: needs a built deadpan-media-worker (cargo build -p deadpan-media-worker)"
        );
        return Ok(());
    };
    let _slot = vt_slot();
    let original = Original::load()?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("Proxies");
    let key = original.key();
    let stub = Stub::new(&worker, None)?;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "host_process_for_kill_test", "--test-threads=1"])
        .env(
            CHILD,
            format!("{}\n{}", root.display(), stub.path.display()),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Kill the host while one of its workers runs, after a recorded range.
    let deadline = Instant::now() + Duration::from_secs(60);
    let orphan = loop {
        assert!(
            Instant::now() < deadline,
            "the child never journaled a range"
        );
        assert!(
            child.try_wait()?.is_none(),
            "the child build ended by itself"
        );
        let done = journaled(&root, &key);
        let pids = stub.pids();
        if done >= 1
            && pids.len() > done
            && let Some(pid) = pids.last()
            && alive(pid)
        {
            child.kill()?;
            break pid.clone();
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let status = child.wait()?;
    assert!(!status.success());
    let kept = journaled(&root, &key) as u64;
    assert!((1..RANGES).contains(&kept), "{kept}");
    // The orphaned worker stops by itself once it finds its host gone.
    let deadline = Instant::now() + Duration::from_secs(20);
    while alive(&orphan) {
        assert!(
            Instant::now() < deadline,
            "the orphaned worker kept running"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let cache = ProxyCache::at(&root)?;
    let progress = ProxyProgress::default();
    let entry = original.build(&cache, &worker, &AtomicBool::new(false), &progress, TARGET)?;
    let resumed = progress.snapshot();
    assert!(resumed.reused_ranges >= kept.min(1) && resumed.reused_ranges > 0);
    // The journal may have gained the orphan's range only if it finished
    // and was recorded before the host died, which the kill above precedes.
    assert_eq!(resumed.reused_ranges, kept);
    assert_eq!(resumed.encoded_ranges, RANGES - kept);
    original.check_published(&cache, &entry);
    assert!(!partial_directory(&root, &key).exists());
    Ok(())
}

/// (3)–(5) Cancellation keeps completed ranges; damaged ranges, a torn tail
/// and a torn journal are never trusted; a journal for other Original
/// bytes, another recipe or another range rule is discarded; a remembered
/// failure removes the partial state.
#[test]
fn damaged_or_foreign_partial_state_is_encoded_again_never_trusted() -> Result {
    let Some(worker) = media_worker() else {
        eprintln!(
            "skipping: needs a built deadpan-media-worker (cargo build -p deadpan-media-worker)"
        );
        return Ok(());
    };
    let _slot = vt_slot();
    let original = Original::load()?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("Proxies");
    let cache = ProxyCache::at(&root)?;
    let key = original.key();
    let directory = partial_directory(&root, &key);

    // (5) Cancel after four encoded ranges: they stay, nothing is remembered
    // or published, and cleanup keeps the current Original's ranges.
    let cancelled = AtomicBool::new(false);
    let progress = ProxyProgress::default();
    std::thread::scope(|scope| {
        let build = scope.spawn(|| original.build(&cache, &worker, &cancelled, &progress, TARGET));
        while progress.snapshot().encoded_ranges < 4 && !build.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        cancelled.store(true, Ordering::Release);
        assert!(
            build
                .join()
                .unwrap()
                .is_err_and(|error| error.is_cancellation())
        );
    });
    let kept = journaled(&root, &key);
    assert!((4..RANGES as usize).contains(&kept), "{kept}");
    assert_eq!(cache.failure(&key), None);
    assert!(cache.lookup(&key)?.is_none());
    cache.cleanup(
        std::slice::from_ref(&key),
        deadpan_cli::proxy::cache::ProxyCleanupPolicy {
            partial_grace: Duration::ZERO,
            ..Default::default()
        },
    )?;
    assert_eq!(journaled(&root, &key), kept, "retained by cleanup");
    let resumed = reuse_decision(&original, &cache, &worker, TARGET);
    assert_eq!(resumed.reused_ranges, kept as u64);

    // (3a) One flipped byte inside a recorded range, and a torn tail.
    let ranges = journal(&root, &key).unwrap()["ranges"].clone();
    let victim = &ranges[1];
    let data = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("segments.bin"))?;
    let at = victim["offset"].as_u64().unwrap() + victim["length"].as_u64().unwrap() / 2;
    let mut byte = [0_u8];
    data.read_exact_at(&mut byte, at)?;
    data.write_all_at(&[byte[0] ^ 0x5a], at)?;
    let length = data.metadata()?.len();
    data.write_all_at(b"torn range bytes", length)?;
    drop(data);
    let resumed = reuse_decision(&original, &cache, &worker, TARGET);
    assert_eq!(
        resumed.reused_ranges,
        kept as u64 - 1,
        "the damaged range is dropped"
    );
    let survivors = journal(&root, &key).unwrap()["ranges"].clone();
    assert!(
        survivors
            .as_array()
            .unwrap()
            .iter()
            .all(|range| range["start"] != victim["start"])
    );

    // (3b) Truncation into the last recorded range drops that range.
    let recorded = survivors.as_array().unwrap().len();
    let last_end = survivors
        .as_array()
        .unwrap()
        .iter()
        .map(|range| range["offset"].as_u64().unwrap() + range["length"].as_u64().unwrap())
        .max()
        .unwrap();
    OpenOptions::new()
        .write(true)
        .open(directory.join("segments.bin"))?
        .set_len(last_end - 1)?;
    let resumed = reuse_decision(&original, &cache, &worker, TARGET);
    assert_eq!(resumed.reused_ranges, recorded as u64 - 1);
    let recorded = journaled(&root, &key);
    assert!(recorded >= 1);

    // (4) A journal naming other Original bytes, another recipe, encoder or
    // plan is discarded whole, although its ranges are intact.
    for (field, value) in [
        ("/original/sha256", serde_json::json!("0".repeat(64))),
        (
            "/original/byte_length",
            serde_json::json!(original.bytes.len() + 1),
        ),
        ("/recipe", serde_json::json!(99)),
        ("/encoder", serde_json::json!("another encoder")),
        ("/width", serde_json::json!(2)),
    ] {
        let mut changed = journal(&root, &key).unwrap();
        assert!(!changed["ranges"].as_array().unwrap().is_empty());
        *changed.pointer_mut(field).unwrap() = value;
        std::fs::write(directory.join("ranges.json"), serde_json::to_vec(&changed)?)?;
        let resumed = reuse_decision(&original, &cache, &worker, TARGET);
        assert_eq!(resumed.reused_ranges, 0, "{field}");
        rebuild_ranges(&original, &cache, &worker, recorded)?;
    }
    // Ranges planned with another target are not reused either.
    let resumed = reuse_decision(&original, &cache, &worker, TARGET * 2);
    assert_eq!(resumed.reused_ranges, 0);
    rebuild_ranges(&original, &cache, &worker, recorded)?;
    // Unchanged, the same journal is reused.
    assert_eq!(
        reuse_decision(&original, &cache, &worker, TARGET).reused_ranges,
        journaled(&root, &key) as u64
    );

    // (3c) A torn journal is never trusted.
    let bytes = std::fs::read(directory.join("ranges.json"))?;
    std::fs::write(directory.join("ranges.json"), &bytes[..bytes.len() / 2])?;
    let resumed = reuse_decision(&original, &cache, &worker, TARGET);
    assert_eq!(resumed.reused_ranges, 0);
    assert_eq!(
        std::fs::metadata(directory.join("segments.bin"))?.len(),
        0,
        "the untrusted ranges are dropped"
    );

    // A remembered failure removes the partial state.
    rebuild_ranges(&original, &cache, &worker, 2)?;
    let failing = Stub::new(&worker, Some(1))?;
    let error = original
        .build(
            &cache,
            &failing.path,
            &AtomicBool::new(false),
            &ProxyProgress::default(),
            TARGET,
        )
        .map(|_| ())
        .unwrap_err();
    assert!(
        !error.is_environmental() && !error.is_cancellation(),
        "{error}"
    );
    assert!(cache.failure(&key).is_some());
    assert!(!directory.exists(), "a failed build keeps no ranges");
    cache.forget_failure(&key)?;

    // Finally a complete build from nothing verifies.
    let progress = ProxyProgress::default();
    let entry = original.build(&cache, &worker, &AtomicBool::new(false), &progress, TARGET)?;
    assert_eq!(progress.snapshot().encoded_ranges, RANGES);
    original.check_published(&cache, &entry);
    assert!(!directory.exists());
    Ok(())
}

/// Encode until the journal holds at least `ranges` ranges, then cancel.
fn rebuild_ranges(original: &Original, cache: &ProxyCache, worker: &Path, ranges: usize) -> Result {
    let root = cache.path().to_owned();
    let key = original.key();
    let cancelled = AtomicBool::new(false);
    let progress = ProxyProgress::default();
    std::thread::scope(|scope| {
        let build = scope.spawn(|| original.build(cache, worker, &cancelled, &progress, TARGET));
        while journaled(&root, &key) < ranges && !build.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        cancelled.store(true, Ordering::Release);
        assert!(
            build
                .join()
                .unwrap()
                .is_err_and(|error| error.is_cancellation())
        );
    });
    assert!(journaled(&root, &key) >= ranges);
    Ok(())
}

/// Measurement, not a check: build the proxy of the Original named by
/// `DEADPAN_PROXY_MEASURE` as one range and in ranges of the production
/// target, then cancel a build halfway and resume it, printing each time.
/// Run in release:
/// `DEADPAN_PROXY_MEASURE=/path/uhd.mp4 cargo test --release -p deadpan-cli
/// --test proxy_resume measure -- --ignored --nocapture`.
#[test]
#[ignore = "measurement on a caller-supplied Original"]
fn measure_range_overhead_and_resume() -> Result {
    let (Some(path), Some(worker)) = (std::env::var_os("DEADPAN_PROXY_MEASURE"), media_worker())
    else {
        eprintln!("skipping: set DEADPAN_PROXY_MEASURE and build deadpan-media-worker");
        return Ok(());
    };
    let _slot = vt_slot();
    let original = Original::load_from(Path::new(&path))?;
    let target = deadpan_cli::proxy::PROXY_SEGMENT_PICTURES;
    for (label, segment) in [
        ("one range", u64::MAX / 2),
        ("production ranges", target),
        ("ranges of 120", 120),
    ] {
        let scratch = tempfile::tempdir()?;
        let cache = ProxyCache::at(&scratch.path().join("Proxies"))?;
        let progress = ProxyProgress::default();
        let started = Instant::now();
        let entry = original.build(&cache, &worker, &AtomicBool::new(false), &progress, segment)?;
        original.check_published(&cache, &entry);
        eprintln!(
            "{label}: {:.1} s, {:?}, proxy {} bytes",
            started.elapsed().as_secs_f64(),
            progress.snapshot(),
            entry.sidecar().file.byte_length
        );
    }
    let scratch = tempfile::tempdir()?;
    let cache = ProxyCache::at(&scratch.path().join("Proxies"))?;
    let cancelled = AtomicBool::new(false);
    let progress = ProxyProgress::default();
    let started = Instant::now();
    std::thread::scope(|scope| {
        let build = scope.spawn(|| original.build(&cache, &worker, &cancelled, &progress, target));
        while progress.percent() < 50 && !build.is_finished() {
            std::thread::sleep(Duration::from_millis(5));
        }
        cancelled.store(true, Ordering::Release);
        let _ = build.join().unwrap();
    });
    eprintln!(
        "cancelled at {:.1} s with {:?}",
        started.elapsed().as_secs_f64(),
        progress.snapshot()
    );
    let progress = ProxyProgress::default();
    let started = Instant::now();
    let entry = original.build(&cache, &worker, &AtomicBool::new(false), &progress, target)?;
    original.check_published(&cache, &entry);
    eprintln!(
        "resumed: {:.1} s, {:?}",
        started.elapsed().as_secs_f64(),
        progress.snapshot()
    );
    Ok(())
}
