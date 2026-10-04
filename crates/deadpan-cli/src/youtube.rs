//! One Original from a YouTube URL (DP-14), at the headless level.
//!
//! `url` normalizes a user URL to one video ID. `helpers` installs and verifies
//! the pinned yt-dlp/Deno bundle. `acquire` runs yt-dlp under a cleared
//! environment, selects admissible streams from its metadata, downloads them
//! into a private directory, assembles one MP4 through the isolated media
//! worker and creates the project through the ordinary single-Original path.

pub mod url;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod acquire;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod helpers;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod runner;

use std::fmt;

/// An actionable acquisition failure with a stable machine-readable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub code: &'static str,
    pub message: String,
}

impl ImportError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ImportError {}

impl From<url::UrlError> for ImportError {
    fn from(error: url::UrlError) -> Self {
        let code = match error {
            url::UrlError::PlaylistWithoutVideo => "YouTubePlaylistNeedsVideo",
            _ => "YouTubeUrlInvalid",
        };
        Self::new(code, error.to_string())
    }
}

/// One complete JSON object per stdout line, as long-running commands report.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn emit(value: &serde_json::Value) -> Result<(), crate::CliError> {
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    writeln!(output, "{value}")?;
    output.flush()?;
    Ok(())
}
