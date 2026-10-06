//! Preview proxy building and lookup for one registered Original.
//!
//! Proxies serve interactive seeking only. This module and its per-user
//! [`cache`] are the sole hosts of [`deadpan_media::proxy`] outside the media
//! crate; the native preview worker and the performance harness use them.
//! Export, render, verification, AI conditioning, tracking, shot analysis and
//! thumbnails never do; `export_paths_never_reach_proxies` checks every
//! crate's sources.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_media::proxy::{
    ProxyIneligible, ProxyOriginal, ProxyPlan, ProxySidecar, VerifyControl, estimated_proxy_bytes,
    hex, proxy_plan, proxy_request, verify_proxy,
};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::QualifiedVideoSnapshot;
use deadpan_media::{ProxyEncodeOptions, encode_proxy_retrying};
use deadpan_store::original_media::OriginalMediaRecord;

pub mod cache;
use cache::{
    DEFAULT_PROXY_BUDGET_BYTES, ProxyCache, ProxyCacheError, ProxyCleanupPolicy, ProxyEntry,
    ProxyKey,
};

/// Whole build: copy, worker encoding, verification and publication.
pub const PROXY_BUILD_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
/// Free space kept beyond a build's estimated needs on each volume.
const FREE_SPACE_MARGIN: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProxyBuildError {
    #[error("{0}")]
    Ineligible(#[from] ProxyIneligible),
    #[error(transparent)]
    Cache(#[from] ProxyCacheError),
    #[error(transparent)]
    Store(#[from] deadpan_store::StoreError),
    #[error("proxy encoding failed: {0}")]
    Conversion(#[from] deadpan_media::ConversionError),
    #[error("proxy verification failed: {0}")]
    Verification(#[from] deadpan_media::proxy::ProxyError),
    #[error(transparent)]
    Input(#[from] deadpan_media::source_input::SourceInputError),
    #[error(transparent)]
    Index(#[from] deadpan_media::source_index::SourceIndexError),
    #[error(transparent)]
    Document(#[from] deadpan_core::DocumentError),
    #[error(
        "not enough free space for a proxy on the {volume} volume: {available} bytes free, {needed} needed"
    )]
    Space {
        volume: &'static str,
        needed: u64,
        available: u64,
    },
    #[error("proxy I/O: {0}")]
    Io(#[from] std::io::Error),
}

impl ProxyBuildError {
    /// Cancellation: not a verdict about the media. A deadline is a failure,
    /// so a build that cannot finish is not restarted.
    pub fn is_cancellation(&self) -> bool {
        matches!(
            self,
            Self::Conversion(deadpan_media::ConversionError::Cancelled)
                | Self::Verification(deadpan_media::proxy::ProxyError::Cancelled)
                | Self::Cache(ProxyCacheError::Proxy(
                    deadpan_media::proxy::ProxyError::Cancelled
                ))
        )
    }

    /// A condition of this machine rather than of this Original (space,
    /// budget, an overloaded or unresponsive VideoToolbox service that also
    /// failed its retry): retried on a later opening, never remembered as a
    /// failure.
    pub fn is_environmental(&self) -> bool {
        match self {
            Self::Space { .. } | Self::Cache(ProxyCacheError::Budget { .. }) => true,
            Self::Conversion(error) => deadpan_media::retryable(error),
            _ => false,
        }
    }
}

/// What the cache holds for one Original.
#[derive(Debug, Clone)]
pub enum ProxyStatus {
    /// The Original already seeks fast enough.
    NotNeeded,
    /// A proxy cannot represent this Original exactly.
    Ineligible(ProxyIneligible),
    /// An earlier build for this Original and recipe failed; it is not
    /// repeated until the failure is forgotten.
    Failed(String),
    /// A proxy should exist and does not, or the cached one is unusable.
    Missing(ProxyPlan),
    Ready(ProxyEntry),
}

/// Cooperative control of one build.
#[derive(Clone, Copy)]
pub struct BuildControl<'a> {
    pub cancelled: &'a AtomicBool,
    /// While set the worker's process group is suspended and verification
    /// waits; paused time never counts as a stall.
    pub pause: Option<&'a AtomicBool>,
    pub stall: Duration,
}

impl<'a> BuildControl<'a> {
    pub fn new(cancelled: &'a AtomicBool) -> Self {
        Self {
            cancelled,
            pause: None,
            stall: deadpan_media::PROXY_STALL_TIMEOUT,
        }
    }

    fn wait_while_paused(&self) -> Result<(), ProxyBuildError> {
        while self
            .pause
            .is_some_and(|pause| pause.load(Ordering::Acquire))
        {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(deadpan_media::ConversionError::Cancelled.into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(deadpan_media::ConversionError::Cancelled.into());
        }
        Ok(())
    }
}

pub fn proxy_key(
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
) -> Result<ProxyKey, ProxyCacheError> {
    ProxyKey::new(
        original.object().content().digest(),
        video.index().stream_index(),
    )
}

pub fn proxy_identity(
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
) -> ProxyOriginal {
    ProxyOriginal {
        blake3: original.object().content().digest().to_owned(),
        sha256: hex(&original.sha256()),
        byte_length: original.object().byte_length(),
        stream_index: video.index().stream_index(),
    }
}

/// Inspect the cache without building or hashing. A damaged or mismatched
/// entry is reported as `Missing`, so a builder replaces it.
pub fn proxy_status(
    cache: &ProxyCache,
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
) -> Result<ProxyStatus, ProxyCacheError> {
    let plan = match proxy_plan(video.interpretation(), video.index().index()) {
        Ok(Some(plan)) => plan,
        Ok(None) => return Ok(ProxyStatus::NotNeeded),
        Err(reason) => return Ok(ProxyStatus::Ineligible(reason)),
    };
    if let Some(entry) = cached_proxy(cache, original, video)? {
        return Ok(ProxyStatus::Ready(entry));
    }
    if let Some(message) = cache.failure(&proxy_key(original, video)?) {
        return Ok(ProxyStatus::Failed(message));
    }
    Ok(ProxyStatus::Missing(plan))
}

/// The current entry for this Original, if the cache holds one, whatever
/// the eligibility policy says. A damaged, stale or mismatched entry is
/// `None`.
pub fn cached_proxy(
    cache: &ProxyCache,
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
) -> Result<Option<ProxyEntry>, ProxyCacheError> {
    let key = proxy_key(original, video)?;
    match cache.lookup(&key) {
        Ok(Some(entry))
            if entry
                .sidecar()
                .validate_for(
                    &proxy_identity(original, video),
                    video.index().index(),
                    video.interpretation(),
                )
                .is_ok() =>
        {
            Ok(Some(entry))
        }
        Ok(_) | Err(ProxyCacheError::Damaged(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Opens the verified Original bytes for one build.
pub type OpenOriginal<'a> =
    Box<dyn FnOnce(&AtomicBool) -> Result<Box<dyn Read + 'a>, ProxyBuildError> + 'a>;

/// Build, verify and publish the proxy of one Original if it needs one and
/// the cache has none. Call on a background thread; it spawns the isolated
/// media worker and can take minutes. `worker` is the vetted
/// `deadpan-media-worker` beside the host executable. A failure that is not
/// a cancellation or a machine condition is remembered for this Original
/// and recipe.
pub fn build_proxy(
    cache: &ProxyCache,
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
    worker: &Path,
    open_original: OpenOriginal<'_>,
    control: BuildControl<'_>,
) -> Result<ProxyStatus, ProxyBuildError> {
    let plan = match proxy_status(cache, original, video)? {
        ProxyStatus::Missing(plan) => plan,
        ProxyStatus::Ready(entry) => match cache.open(&entry, control.cancelled) {
            // Verified once per file state; this repeats only after a change.
            Ok(_) => return Ok(ProxyStatus::Ready(entry)),
            Err(ProxyCacheError::Damaged(_)) => {
                cache.remove(entry.key())?;
                match proxy_plan(video.interpretation(), video.index().index()) {
                    Ok(Some(plan)) => plan,
                    Ok(None) => return Ok(ProxyStatus::NotNeeded),
                    Err(reason) => return Ok(ProxyStatus::Ineligible(reason)),
                }
            }
            Err(error) => return Err(error.into()),
        },
        other => return Ok(other),
    };
    match build_proxy_with_plan(cache, original, video, worker, plan, open_original, control) {
        Ok(entry) => Ok(ProxyStatus::Ready(entry)),
        Err(error) => {
            if !error.is_cancellation() && !error.is_environmental() {
                cache.record_failure(&proxy_key(original, video)?, &error.to_string())?;
            }
            Err(error)
        }
    }
}

/// Build and publish a proxy with an explicit plan, replacing any entry.
/// [`build_proxy`] chooses the plan by policy; tests and benchmarks may
/// force one, for example for a small fixture. The raster must still be the
/// recipe's raster for the Original, or verification refuses the result.
pub fn build_proxy_with_plan(
    cache: &ProxyCache,
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
    worker: &Path,
    plan: ProxyPlan,
    open_original: OpenOriginal<'_>,
    control: BuildControl<'_>,
) -> Result<ProxyEntry, ProxyBuildError> {
    let started = Instant::now();
    deadpan_media::proxy::expressible(video.index().index())?;
    let key = proxy_key(original, video)?;
    let info = video.interpretation();
    let frames = video.index().index().frames().len() as u64;
    let estimate = estimated_proxy_bytes(&plan, frames);
    let used = cache.usage()?;
    if used.saturating_add(estimate) > DEFAULT_PROXY_BUDGET_BYTES {
        cache.cleanup(std::slice::from_ref(&key), ProxyCleanupPolicy::default())?;
        let used = cache.usage()?;
        if used.saturating_add(estimate) > DEFAULT_PROXY_BUDGET_BYTES {
            return Err(ProxyCacheError::Budget {
                used,
                budget: DEFAULT_PROXY_BUDGET_BYTES,
            }
            .into());
        }
    }
    check_space("cache", cache.available_bytes()?, estimate)?;
    // The verified Original snapshot is a private temporary file.
    let temporary = rustix::fs::open(
        std::env::temp_dir(),
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    check_space(
        "temporary",
        cache::available(&temporary)?,
        original.object().byte_length(),
    )?;
    control.wait_while_paused()?;
    let mut source = open_original(control.cancelled)?;
    let input = VerifiedSourceInput::copy_verified(
        &mut source,
        video.index().content(),
        original.object().byte_length().max(1),
        remaining(started)?,
        control.cancelled,
    )?;
    drop(source);
    control.wait_while_paused()?;
    let request = proxy_request(
        input.identity().byte_length(),
        info,
        frames,
        &plan,
        remaining(started)?,
    )?;
    // One VideoToolbox proxy session per user at a time, across processes:
    // its encoder service has hung under many concurrent sessions.
    let _slot = cache.encoder_slot(control.cancelled)?;
    // Each attempt writes a fresh staging file; a failed one is removed.
    let (staging, _) = encode_proxy_retrying(
        worker,
        &input,
        &request,
        || {
            let staging = cache
                .stage()
                .map_err(|error| deadpan_media::ConversionError::Protocol(error.to_string()))?;
            let file = staging.movie().try_clone()?;
            Ok((staging, file))
        },
        control.cancelled,
        ProxyEncodeOptions {
            stall: control.stall,
            pause: control.pause,
        },
    )?;
    let sidecar = verify_proxy(
        staging.movie(),
        &input,
        original.object().content().digest(),
        Arc::new(video.index().clone()),
        info,
        plan.reason,
        VerifyControl {
            timeout: remaining(started)?,
            cancelled: control.cancelled,
            pause: control.pause,
        },
    )?;
    control.wait_while_paused()?;
    Ok(cache.publish(&key, staging, &sidecar)?)
}

fn check_space(volume: &'static str, available: u64, bytes: u64) -> Result<(), ProxyBuildError> {
    let needed = bytes.saturating_add(FREE_SPACE_MARGIN);
    if available < needed {
        return Err(ProxyBuildError::Space {
            volume,
            needed,
            available,
        });
    }
    Ok(())
}

fn remaining(started: Instant) -> Result<Duration, ProxyBuildError> {
    let left = PROXY_BUILD_TIMEOUT.saturating_sub(started.elapsed());
    if left.is_zero() {
        return Err(deadpan_media::ConversionError::Deadline.into());
    }
    Ok(left)
}

/// The current verified proxy of one Original, opened in place for
/// interactive preview: the published read-only movie (its bytes verified
/// once per file state, and protected from eviction while open) and its
/// sidecar, already checked against the receipt's index. No bytes are
/// copied.
pub fn open_proxy_file(
    cache: &ProxyCache,
    original: &OriginalMediaRecord,
    video: &QualifiedVideoSnapshot,
    cancelled: &AtomicBool,
) -> Result<Option<(File, Arc<ProxySidecar>)>, ProxyBuildError> {
    let Some(entry) = cached_proxy(cache, original, video)? else {
        return Ok(None);
    };
    let read = cache.open(&entry, cancelled)?;
    Ok(Some((read.into_file(), Arc::clone(entry.sidecar()))))
}

/// The isolated media worker installed beside this executable.
pub fn media_worker() -> Option<std::path::PathBuf> {
    let worker = std::env::current_exe()
        .ok()?
        .parent()?
        .join("deadpan-media-worker");
    worker
        .is_file()
        .then(|| std::fs::canonicalize(worker).ok())?
}

/// Why automatic proxy work should wait now: Low Power Mode, battery power
/// or thermal pressure, as `pmset` reports them. None when it may run, or
/// when the state cannot be read.
pub fn power_guard() -> Option<String> {
    let batt = pmset(&["-g", "batt"])?;
    if batt.contains("'Battery Power'") {
        return Some("running on battery".into());
    }
    let settings = pmset(&["-g"]).unwrap_or_default();
    if settings.lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(
            (words.next(), words.next()),
            (Some("lowpowermode" | "powermode"), Some("1"))
        )
    }) {
        return Some("Low Power Mode is on".into());
    }
    let thermal = pmset(&["-g", "therm"]).unwrap_or_default();
    let limited = thermal.lines().any(|line| {
        line.split_once('=').is_some_and(|(name, value)| {
            name.trim().ends_with("Speed_Limit")
                && value.trim().parse::<u32>().is_ok_and(|limit| limit < 100)
        })
    });
    let warned = thermal
        .lines()
        .any(|line| line.contains("warning level") && !line.contains("No "));
    (limited || warned).then(|| "the Mac is under thermal pressure".into())
}

fn pmset(arguments: &[&str]) -> Option<String> {
    let mut child = deadpan_native_process::spawn(
        Command::new("/usr/bin/pmset")
            .args(arguments)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .ok()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    child
        .stdout
        .take()?
        .take(64 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    /// Proxies never reach export, render, verification, conditioning,
    /// tracking, shot analysis or thumbnails: outside the proxy modules
    /// themselves and the app's private preview reader, no source in any
    /// crate names the proxy modules, the cache or the proxy-only input.
    #[test]
    fn export_paths_never_reach_proxies() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let allowed = [
            // Builders, contract and cache.
            "crates/deadpan-cli/src/proxy.rs",
            "crates/deadpan-cli/src/proxy/cache.rs",
            "crates/deadpan-cli/src/proxy/cache_tests.rs",
            "crates/deadpan-cli/src/lib.rs",
            // Explicit cache cleanup: removes entries, never reads pictures.
            "crates/deadpan-cli/src/storage.rs",
            "crates/deadpan-media/src/proxy.rs",
            "crates/deadpan-media/src/conversion/proxy.rs",
            "crates/deadpan-media/src/conversion.rs",
            "crates/deadpan-media/src/lib.rs",
            "crates/deadpan-media/src/source_input.rs",
            "native/deadpan-media-worker/src/main.rs",
            "native/deadpan-media-worker/src/proxy.rs",
            // The native app: background job, private preview reader, tests.
            "crates/deadpan-app/src/preview.rs",
            "crates/deadpan-app/src/preview/proxies.rs",
            "crates/deadpan-app/src/worker.rs",
            "crates/deadpan-app/src/worker/proxy.rs",
            "crates/deadpan-app/src/worker/proxy_tests.rs",
            "crates/deadpan-app/src/worker/project_tests.rs",
            "crates/deadpan-app/src/preview/harness.rs",
            "crates/deadpan-app/src/preview/harness/proxy.rs",
            "crates/deadpan-app/src/preview/harness/scenarios.rs",
        ];
        let mut checked = 0;
        let mut stack = Vec::new();
        for group in ["crates", "native"] {
            for entry in std::fs::read_dir(root.join(group)).unwrap() {
                let source = entry.unwrap().path().join("src");
                if source.is_dir() {
                    stack.push(source);
                }
            }
        }
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if allowed.contains(&relative.as_str()) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                checked += 1;
                for forbidden in [
                    "proxy::",
                    "ProxyCache",
                    "ProxySidecar",
                    "open_proxy_file",
                    "from_verified_file",
                    "PreviewFrame",
                ] {
                    assert!(
                        !text.contains(forbidden),
                        "{relative} names {forbidden}; only the interactive preview may read proxies"
                    );
                }
            }
        }
        assert!(checked > 300, "{checked}");
    }
}
