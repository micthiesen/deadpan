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
        "partial": ["structural-editing-commands", "sqlite-project-history", "schema-1-through-14-migration", "indexed-picture-plan", "indexed-audio-plan", "worker-dsp-adapter", "source-audio-preparation", "plan-driven-source-pcm", "exact-boundary-selectors", "persistent-marks", "sparse-play-overrides", "nested-occurrence-edits", "persistent-generation-requests", "persistent-generation-attempts", "authored-generated-hold-semantics", "generated-media-conversion", "native-bridge-bundle-qualification", "durable-generated-bundle-acceptance", "durable-original-byte-ownership", "identity-checked-relinking", "persistent-source-decoding", "measured-source-audio-indexes", "independent-source-audio-mapping", "independent-source-video-mapping", "exact-source-stream-placement", "measured-import-timing-candidates", "durable-source-qualification", "atomic-source-registration-and-insertion", "background-import-preparation", "automatic-presentation-basis", "timed-basis-locking", "explicit-canvas-geometry", "shared-sdr-picture-pipeline", "native-source-preview", "native-project-workspace"],
        "unimplemented": ["app-playback", "audio-output", "full-keyboard-editor", "timeline-playback", "analysis", "ai-generation", "youtube-import", "export", "distribution"],
    }))
}
