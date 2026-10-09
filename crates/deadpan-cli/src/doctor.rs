use crate::CliError;
use deadpan_core::{FrameRate, MIX_SAMPLE_RATE, ProjectFrame};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod project;

/// The capability report plus read-only diagnostics for one package.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn project_report(package: &std::path::Path) -> Result<serde_json::Value, CliError> {
    let mut report = report()?;
    report["project"] = project::report(package)?;
    // Read after the probes so the counters include their work.
    report["diagnostics"] = diagnostics(&deadpan_diagnostics::snapshot());
    Ok(report)
}

/// Process-local Section 25.3 counters as JSON. In a CLI process they cover
/// only what this process did; a running app's sessions are not visible.
pub fn diagnostics(snapshot: &deadpan_diagnostics::Snapshot) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    let level =
        |level: deadpan_diagnostics::Level| json!({"current": level.current, "high": level.high});
    let io: Map<String, Value> = snapshot
        .io
        .paths()
        .into_iter()
        .map(|(name, io)| {
            (
                name.to_owned(),
                json!({
                    "read_ops": io.read_ops, "read_bytes": io.read_bytes,
                    "write_ops": io.write_ops, "write_bytes": io.write_bytes,
                }),
            )
        })
        .collect();
    let queues: Map<String, Value> = snapshot
        .queues
        .named()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), level(value)))
        .collect();
    let caches: Map<String, Value> = snapshot
        .caches
        .named()
        .into_iter()
        .map(|(name, cache)| {
            (
                name.to_owned(),
                json!({
                    "hits": cache.hits, "misses": cache.misses, "evictions": cache.evictions,
                    "hit_rate": cache.hit_rate(),
                    "entries": level(cache.entries), "bytes": level(cache.bytes),
                }),
            )
        })
        .collect();
    let workers = |memory: deadpan_diagnostics::WorkerMemorySnapshot| {
        json!({
            "live": level(memory.live),
            "phys_footprint_bytes": level(memory.footprint_bytes),
            "samples": memory.samples, "failed_samples": memory.failures,
        })
    };
    let gpu = snapshot.gpu;
    json!({
        "scope": "process-local counters for this deadpan-cli process only; a running app's sessions, GPU work and workers are not visible here",
        "file_io": io,
        "file_io_note": "the store's revision rows (logical payload bytes, not SQLite pages), object-store copies and snapshots, media snapshots, decoder descriptor reads, the private PCM cache file and proxy reads; one operation is one buffered call",
        "queue_depths": queues,
        "pcm_caches": caches,
        "gpu_submissions": {
            "submissions": gpu.submissions, "completions": gpu.completions,
            "last_us": gpu.last_us, "max_us": gpu.max_us,
            "recent_p50_us": gpu.p50_us, "recent_p95_us": gpu.p95_us,
            "note": "shared picture pipeline only; doctor submits no GPU work",
        },
        "model_workers": workers(snapshot.model_workers),
        "other_workers": workers(snapshot.other_workers),
        "workers_note": "physical footprint of live supervised workers sampled every 500 ms by their supervisor; doctor starts none",
    })
}

pub fn report() -> Result<serde_json::Value, CliError> {
    let rate = FrameRate::new(30_000, 1_001)?;
    Ok(serde_json::json!({
        "schema_version": 1, "application": "deadpan", "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "stage": "development-foundation", "internal_audio_sample_rate": MIX_SAMPLE_RATE,
        "sqlite_version": deadpan_store::sqlite_version(),
        "timing_probe": {
            "frame_rate_numerator": rate.numerator(), "frame_rate_denominator": rate.denominator(),
            "frame": 1, "sample_boundary": rate.audio_boundary(ProjectFrame(1))?.0,
        },
        "document_schema": deadpan_core::DOCUMENT_SCHEMA_VERSION,
        "database_schema": deadpan_store::DATABASE_SCHEMA_VERSION,
        "partial": ["structural-editing-commands", "sqlite-project-history", "schema-1-through-57-development-format-refusal", "persistent-copy-registers", "resolved-compound-transactions", "native-semantic-macros", "headless-semantic-macros", "single-original-project-baseline", "headless-youtube-original-import", "native-documents-project-library", "indexed-picture-plan", "indexed-audio-plan", "worker-dsp-adapter", "source-audio-preparation", "plan-driven-source-pcm", "transparent-audio-partitions", "retained-context-split", "atomic-sequence-pause-insertion", "logical-mark-bindings", "exact-boundary-selectors", "persistent-marks", "sparse-play-overrides", "nested-occurrence-edits", "persistent-generation-requests", "persistent-generation-attempts", "authored-generated-hold-semantics", "generated-media-conversion", "native-bridge-bundle-qualification", "durable-generated-bundle-acceptance", "durable-original-byte-ownership", "identity-checked-relinking", "persistent-source-decoding", "measured-source-audio-indexes", "independent-source-audio-mapping", "independent-source-video-mapping", "exact-source-stream-placement", "measured-import-timing-candidates", "measured-original-moment-candidates", "exact-source-audio-selections", "durable-source-qualification", "atomic-source-registration-and-insertion", "background-import-preparation", "automatic-presentation-basis", "timed-basis-locking", "explicit-canvas-geometry", "shared-sdr-picture-pipeline", "authored-framing-envelopes", "ordered-canvas-framing", "captured-hold-framing", "native-camera-draft", "native-source-preview", "native-project-workspace", "native-sequence-audition", "native-original-audition", "selection-loop-audition", "native-structural-speed-editing", "root-sound-event-commands-and-mixing", "saved-beat-sound-commands", "root-sound-ripple-edit-history", "hold-audio-policy-commands"],
        "time_mapped_pcm": "bounded-continuous-preserve-before-effects",
        "room_tone_pcm": "explicit-source-range-exact-overlap-before-effects",
        "edge_faded_pcm": "authored-boundary-exceptions-after-time-mapping-before-voice-effects",
        "limited_pcm": "original-and-root-sound-bus-with-finite-oversampled-limiter-before-missing-voice-effects",
        "device_output": "macos-bounded-limited-sequence-audition",
        "sequence_audition": "immutable-revision-device-clock-pictures-exact-paused-sample-resume",
        "render": "automatic-sdr-committed-revision-macos-apfs",
        "render_entrypoints": ["native-cmd-e-and-render-command", "closed-project-headless-render-and-recovery"],
        "render_preview_choices": ["commit-and-render", "discard-and-render", "keep-editing"],
        "downloader": downloader(),
        "runtime": runtime(),
        "unimplemented": ["mastered-preview-audio", "full-device-and-acoustic-qualification", "full-keyboard-editor", "analysis", "ai-generation", "native-youtube-import", "full-render-mastering", "hdr-render", "open-project-render-ipc", "native-render-recovery-browser", "signed-downloader-updates", "developer-id-notarized-distribution"],
    }))
}

/// Downloader helpers and whether they are present where an import would
/// read them: a compatible active signed update under the managed root, else
/// the running packaged bundle's baseline (a missing or damaged one is a
/// reported problem, never a fallback), else the managed root's compiled pins.
/// Presence is not verification; `downloader status` hashes the files and
/// `downloader status --probe` runs them.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn downloader() -> serde_json::Value {
    use crate::youtube::helpers::{BUNDLE, HelperSource, default_root};
    let selection = HelperSource::default_selection();
    let source = selection.as_ref().ok().map(|selection| &selection.source);
    serde_json::json!({
        "source": source.map(HelperSource::kind),
        "root": source.map(HelperSource::root),
        "helpers": BUNDLE.iter().map(|pin| {
            let inspected = source.map(|source| source.inspect(pin));
            let release = source
                .and_then(|source| source.release(pin).ok())
                .unwrap_or_else(|| pin.into());
            serde_json::json!({
                "name": pin.name,
                "version": release.version.clone(),
                "license": release.license.clone(),
                "present": matches!(inspected, Some(Ok(_))),
                "path": source.map(|source| release.path(source.root())),
                "problem": match inspected {
                    Some(Err(error)) => Some(error.to_string()),
                    _ => None,
                },
            })
        }).collect::<Vec<_>>(),
        "ejs": source.map_or_else(|| crate::youtube::helpers::EJS_VERSION.into(), HelperSource::ejs_version),
        "baseline": HelperSource::default_baseline().ok().map(|baseline| baseline.kind()),
        "update": match &selection {
            Ok(selection) => default_root()
                .map(|root| crate::youtube::updates::report(&root, selection))
                .unwrap_or_default(),
            Err(error) => serde_json::json!({ "problem": error.to_string() }),
        },
        "distribution": match source {
            Some(HelperSource::Bundled(_)) => "bundled read-only baseline; signed updates install under the managed root",
            Some(HelperSource::Update(_)) => "signed update under the managed root; the baseline stays installed",
            _ => "managed development install; run inside Deadpan.app for the bundled baseline",
        },
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn downloader() -> serde_json::Value {
    serde_json::Value::Null
}

/// Where this process found its workers and FFmpeg libraries. Workers resolve
/// beside the running executable. Loaded FFmpeg images are matched to the
/// bundle's `Contents/Frameworks` files by mapped device and inode, so the
/// report never trusts a path the loader did not actually map.
#[cfg(target_os = "macos")]
fn runtime() -> serde_json::Value {
    use deadpan_encode::runtime::{RuntimeImageKind, RuntimeImageObservation};
    use std::os::unix::fs::MetadataExt;
    let executable = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    let contents = crate::bundle::running_contents();
    let directory = executable.as_deref().and_then(std::path::Path::parent);
    let workers = [
        "deadpan-media-worker",
        "deadpan-transcribe",
        "deadpan-track",
    ]
    .map(|name| {
        let path = directory.map(|directory| directory.join(name));
        serde_json::json!({
            "name": name,
            "present": path.as_ref().is_some_and(|path| path.is_file()),
            "inside_bundle": path.as_ref().zip(contents.as_ref())
                .is_some_and(|(path, contents)| path.starts_with(contents)),
            "path": path,
        })
    });
    let frameworks: Vec<(std::path::PathBuf, u64, u64)> = contents
        .as_ref()
        .and_then(|contents| {
            std::fs::read_dir(contents.join(crate::bundle::FRAMEWORKS_DIRECTORY)).ok()
        })
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let metadata = std::fs::metadata(&path).ok()?;
            Some((path, metadata.dev(), metadata.ino()))
        })
        .collect();
    let ffmpeg = [
        ("avcodec", RuntimeImageKind::Avcodec),
        ("avformat", RuntimeImageKind::Avformat),
        ("avutil", RuntimeImageKind::Avutil),
        ("swscale", RuntimeImageKind::Swscale),
        ("avfilter", RuntimeImageKind::Avfilter),
    ]
    .map(
        |(name, kind)| match RuntimeImageObservation::capture(kind) {
            Ok(observation) => {
                let identity = observation.identity();
                let file = frameworks.iter().find(|(_, device, inode)| {
                    *device == identity.device && *inode == identity.inode
                });
                serde_json::json!({
                    "library": name,
                    "loaded": true,
                    "inside_bundle": file.is_some(),
                    "path": file.map(|(path, _, _)| path),
                })
            }
            Err(error) => serde_json::json!({
                "library": name, "loaded": false, "problem": error.to_string(),
            }),
        },
    );
    serde_json::json!({
        "executable": executable,
        "bundle": contents.as_ref().and_then(|contents| contents.parent()),
        "packaged": contents.as_deref().is_some_and(crate::bundle::is_packaged),
        "workers": workers,
        "ffmpeg": ffmpeg,
        "models_root": crate::models::default_root().ok(),
        "models": models(),
        "ai_runtime": ai_runtime(),
    })
}

/// Approved model packs, the version selected for each (an activated signed
/// update, else the compiled one) and whether it is installed.
#[cfg(target_os = "macos")]
fn models() -> serde_json::Value {
    let Ok(root) = crate::models::default_root() else {
        return serde_json::Value::Null;
    };
    let store = deadpan_models::packs::PackStore::new(root);
    serde_json::Value::Array(
        deadpan_models::packs::approved_packs()
            .into_iter()
            .map(|approved| {
                let manifest = store
                    .selected(&approved.pack_id)
                    .ok()
                    .flatten()
                    .unwrap_or(approved);
                let pointer = store.pointer(&manifest.pack_id).ok().flatten();
                serde_json::json!({
                    "pack_id": manifest.pack_id,
                    "pack_version": manifest.pack_version,
                    "previous_version": pointer.and_then(|pointer| pointer.previous),
                    "note": store.selection_note(&manifest.pack_id).ok().flatten(),
                    "bytes": manifest.total_bytes(),
                    "installed": store.installed(&manifest).ok().flatten().map(|pack| pack.directory),
                })
            })
            .collect(),
    )
}

/// Where AI pauses would find their runtime and model data, and what is
/// missing. Presence only: the worker verifies every pinned file per attempt.
#[cfg(target_os = "macos")]
fn ai_runtime() -> serde_json::Value {
    use crate::generation::runtime::{BridgeRuntime, Lookup, lookup};
    let lookup = lookup();
    let bundled = match &lookup {
        Lookup::Bundled { runtime } => Some(runtime.clone()),
        _ => None,
    };
    let identity = bundled.as_ref().and_then(|runtime| {
        let bytes = std::fs::read(runtime.join("runtime.json")).ok()?;
        serde_json::from_slice::<serde_json::Value>(&bytes).ok()
    });
    let resolved = BridgeRuntime::from_environment();
    serde_json::json!({
        "lookup": lookup.describe(),
        "bundled": bundled,
        "identity": identity,
        "ready": resolved.is_ok(),
        "python": resolved.as_ref().ok().map(|runtime| &runtime.python),
        "worker": resolved.as_ref().ok().map(|runtime| &runtime.worker_script),
        "ffmpeg": resolved.as_ref().ok().map(|runtime| &runtime.ffmpeg),
        "model_data": resolved.as_ref().ok().map(|runtime| &runtime.model_cache),
        "missing": resolved.as_ref().err().map(|error| &error.missing),
    })
}

#[cfg(not(target_os = "macos"))]
fn runtime() -> serde_json::Value {
    serde_json::Value::Null
}
