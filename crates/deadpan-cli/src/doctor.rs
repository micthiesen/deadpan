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
        "partial": ["structural-editing-commands", "sqlite-project-history", "schema-1-migration", "indexed-picture-plan", "exact-boundary-selectors"],
        "unimplemented": ["media-decode", "audio-output", "keyboard-editor", "media-preview", "analysis", "ai-generation", "youtube-import", "export", "distribution"],
    }))
}
