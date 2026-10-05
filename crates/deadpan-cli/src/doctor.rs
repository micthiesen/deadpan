use crate::CliError;
use deadpan_core::{FrameRate, MIX_SAMPLE_RATE, ProjectFrame};

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

/// Pinned downloader helpers and whether they are present where an import
/// would read them: the running packaged bundle's baseline (a missing or
/// damaged one is a reported problem, never a fallback), else the managed
/// root. Presence is not verification; `downloader status` hashes the files and
/// `downloader status --probe` runs them.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn downloader() -> serde_json::Value {
    use crate::youtube::helpers::{BUNDLE, EJS_VERSION, HelperSource};
    let source = HelperSource::default_source().ok();
    serde_json::json!({
        "source": source.as_ref().map(HelperSource::kind),
        "root": source.as_ref().map(HelperSource::root),
        "helpers": BUNDLE.iter().map(|pin| {
            let inspected = source.as_ref().map(|source| source.inspect(pin));
            serde_json::json!({
                "name": pin.name, "version": pin.version, "license": pin.license,
                "present": matches!(inspected, Some(Ok(_))),
                "path": source.as_ref().map(|source| pin.path(source.root())),
                "problem": match inspected {
                    Some(Err(error)) => Some(error.to_string()),
                    _ => None,
                },
            })
        }).collect::<Vec<_>>(),
        "ejs": EJS_VERSION,
        "distribution": match source {
            Some(HelperSource::Bundled(_)) => "bundled read-only baseline; signed update manifests and rollback remain open",
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
        "models": "downloaded on request into the models root; none ship in the bundle",
        "ai_bridge_runtime": crate::generation::runtime::lookup().describe(),
    })
}

#[cfg(not(target_os = "macos"))]
fn runtime() -> serde_json::Value {
    serde_json::Value::Null
}
