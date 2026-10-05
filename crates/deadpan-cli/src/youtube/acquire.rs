//! Supervised yt-dlp acquisition of one YouTube video as a project Original.
//!
//! Every helper runs by absolute path with an argument vector, a cleared
//! environment whose HOME, TMPDIR and XDG directories point into one private
//! temporary directory, no user/global configuration, no plugin discovery,
//! no remote components and only the pinned Deno as JavaScript runtime.
//! Metadata comes first: playlists, live streams and inadmissible videos are
//! refused before any media transfer or package creation. Deadpan, not
//! yt-dlp's selector, chooses the streams; the download reuses the inspected
//! metadata so the same video and formats are fetched. The separately
//! delivered picture and sound are assembled by stream copy in the isolated
//! media worker and then retained, qualified and initialized through the
//! ordinary single-Original path. Cookies are used only from an explicit file,
//! copied to an owner-only private file and removed after transfer.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadpan_store::original_provenance::{
    FormatRole, HelperVersion, PROVENANCE_SCHEMA, RemoteOriginalProvenance, SelectedFormat,
};
use serde::Serialize;
use serde_json::Value;

use super::ImportError;
use super::helpers::{DENO, EJS_VERSION, Helpers, YT_DLP};
pub use super::runner::{
    HelperCommand, HelperRun, PrivateCookies, Workspace, environment, run_helper,
};
use super::url::{VideoId, normalize};
use crate::CliError;
use crate::single_original::{self, CreatedOriginal};

/// Stream-copy assembly identity recorded in provenance.
pub const ASSEMBLY: &str = "deadpan-media-worker remux v1 (H.264 + AAC stream copy, FFmpeg 8.0.3)";
/// Download protocols that deliver one complete MP4 per format without FFmpeg.
const DIRECT_PROTOCOLS: [&str; 2] = ["https", "http_dash_segments"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    pub metadata_timeout: Duration,
    pub download_timeout: Duration,
    pub max_metadata_bytes: usize,
    /// Both downloaded streams together.
    pub max_download_bytes: u64,
    pub max_duration_seconds: u64,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            metadata_timeout: Duration::from_secs(120),
            download_timeout: Duration::from_secs(4 * 60 * 60),
            max_metadata_bytes: 32 * 1024 * 1024,
            max_download_bytes: 16 * 1024 * 1024 * 1024,
            max_duration_seconds: 6 * 60 * 60,
        }
    }
}

/// Arguments shared by every yt-dlp invocation.
pub fn base_arguments(helpers: &Helpers, cookies: Option<&Path>) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-js-runtimes",
    ]
    .map(OsString::from)
    .to_vec();
    let mut runtime = OsString::from("deno:");
    runtime.push(helpers.deno.as_os_str());
    arguments.push("--js-runtimes".into());
    arguments.push(runtime);
    arguments.extend(
        [
            "--no-cache-dir",
            "--no-cookies-from-browser",
            "--no-playlist",
            "--color",
            "never",
            "--socket-timeout",
            "30",
        ]
        .map(OsString::from),
    );
    match cookies {
        Some(cookies) => {
            arguments.push("--cookies".into());
            arguments.push(cookies.into());
        }
        None => arguments.push("--no-cookies".into()),
    }
    arguments
}

pub fn metadata_arguments(
    helpers: &Helpers,
    cookies: Option<&Path>,
    id: &VideoId,
) -> Vec<OsString> {
    let mut arguments = base_arguments(helpers, cookies);
    arguments.extend(["--dump-single-json", "--"].map(OsString::from));
    arguments.push(id.watch_url().into());
    arguments
}

pub fn download_arguments(
    helpers: &Helpers,
    cookies: Option<&Path>,
    info: &Path,
    selection: &Selection,
    max_bytes: u64,
) -> Vec<OsString> {
    let mut arguments = base_arguments(helpers, cookies);
    arguments.push("--load-info-json".into());
    arguments.push(info.into());
    arguments.push("--format".into());
    arguments.push(
        format!(
            "{},{}",
            selection.video.format_id, selection.audio.format_id
        )
        .into(),
    );
    arguments.push("--output".into());
    arguments.push("%(format_id)s.%(ext)s".into());
    arguments.extend(
        [
            "--quiet",
            "--max-filesize",
            &max_bytes.to_string(),
            "--no-mtime",
            "--no-progress",
            "--retries",
            "3",
            "--fragment-retries",
            "3",
            "--abort-on-error",
        ]
        .map(OsString::from),
    );
    arguments
}

/// Map yt-dlp's final error to an actionable code.
pub fn classify_failure(stderr: &str) -> ImportError {
    let line = stderr
        .lines()
        .rev()
        .find(|line| line.starts_with("ERROR:"))
        .unwrap_or_else(|| {
            stderr
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
        });
    let detail: String = line.chars().take(400).collect();
    let lower = line.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    let (code, advice) = if has(&["private video", "video is private"]) {
        (
            "YouTubeVideoPrivate",
            "This video is private; choose a public or unlisted video.",
        )
    } else if has(&[
        "not made this video available in your country",
        "not available in your country",
        "geo restrict",
        "geo-restrict",
    ]) {
        (
            "YouTubeRegionRestricted",
            "This video is not available in your region.",
        )
    } else if has(&[
        "confirm your age",
        "age-restricted",
        "age restricted",
        "inappropriate for some users",
    ]) {
        (
            "YouTubeAgeRestricted",
            "This video is age-restricted; retry with an explicit --cookies file from a signed-in account.",
        )
    } else if has(&[
        "not a bot",
        "http error 429",
        "too many requests",
        "rate-limit",
        "rate limit",
    ]) {
        (
            "YouTubeRateLimited",
            "YouTube is limiting requests from this network; wait and retry later, or use an explicit --cookies file.",
        )
    } else if has(&[
        "members-only",
        "members only",
        "join this channel",
        "premium",
    ]) {
        (
            "YouTubeSignInRequired",
            "This video requires a signed-in account with access; retry with an explicit --cookies file.",
        )
    } else if has(&["sign in", "login required", "log in"]) {
        (
            "YouTubeSignInRequired",
            "YouTube requires sign-in for this video; retry with an explicit --cookies file.",
        )
    } else if has(&[
        "live event will begin",
        "premieres in",
        "is_upcoming",
        "this live stream",
        "is live",
    ]) {
        (
            "YouTubeLiveUnsupported",
            "Live and upcoming streams cannot be imported; retry after the recording is available.",
        )
    } else if has(&[
        "requested format is not available",
        "no video formats",
        "format is not available",
    ]) {
        (
            "YouTubeFormatUnavailable",
            "YouTube offered no downloadable stream that Deadpan can import.",
        )
    } else if has(&[
        "video unavailable",
        "video is unavailable",
        "has been removed",
        "no longer available",
        "account associated with this video has been terminated",
        "does not exist",
    ]) {
        (
            "YouTubeVideoUnavailable",
            "This video is unavailable or was removed.",
        )
    } else if has(&[
        "unable to download webpage",
        "urlopen error",
        "timed out",
        "name resolution",
        "nodename nor servname",
        "connection reset",
        "network is unreachable",
        "temporary failure",
        "ssl",
    ]) {
        (
            "YouTubeNetworkFailed",
            "The network request failed; check the connection and retry.",
        )
    } else if has(&[
        "http error 403",
        "http error 5",
        "did not get any data",
        "incomplete",
        "content too short",
        "unable to download video data",
    ]) {
        (
            "YouTubeDownloadFailed",
            "The transfer was interrupted or refused; retry the import.",
        )
    } else if has(&[
        "unsupported url",
        "unable to extract",
        "signature",
        "nsig",
        "challenge",
        "jsc",
        "javascript",
        "player",
    ]) {
        (
            "YouTubeExtractorFailed",
            "YouTube changed in a way the pinned downloader cannot handle; a downloader update is required.",
        )
    } else {
        ("DownloaderFailed", "The downloader failed.")
    };
    let message = if detail.is_empty() {
        advice.to_owned()
    } else {
        format!("{advice} Downloader said: {detail}")
    };
    ImportError::new(code, message)
}

/// One chosen remote stream.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Format {
    pub format_id: String,
    pub ext: String,
    pub codec: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub bitrate_kbps: Option<f64>,
    pub sample_rate: Option<u32>,
    pub declared_bytes: Option<u64>,
    pub approximate_bytes: Option<u64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Selection {
    pub video: Format,
    pub audio: Format,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoMetadata {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub license: Option<String>,
    pub upload_date: Option<String>,
    pub duration_seconds: f64,
    pub thumbnail_url: Option<String>,
    pub selection: Selection,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| {
            text.chars()
                .filter(|c| !c.is_control())
                .take(1024)
                .collect::<String>()
        })
        .filter(|text| !text.trim().is_empty())
}

fn https(value: Option<String>) -> Option<String> {
    value.filter(|url| url.starts_with("https://") && url.len() <= 2048)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn count(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn format(value: &Value, codec_key: &str) -> Option<Format> {
    Some(Format {
        format_id: text(value, "format_id")?,
        ext: text(value, "ext")?,
        codec: text(value, codec_key)?,
        width: count(value, "width").and_then(|n| u32::try_from(n).ok()),
        height: count(value, "height").and_then(|n| u32::try_from(n).ok()),
        fps: number(value, "fps"),
        bitrate_kbps: number(value, "tbr"),
        sample_rate: count(value, "asr").and_then(|n| u32::try_from(n).ok()),
        declared_bytes: count(value, "filesize"),
        approximate_bytes: count(value, "filesize_approx"),
        note: text(value, "format_note"),
    })
}

fn safe_format_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn direct(value: &Value) -> bool {
    value
        .get("protocol")
        .and_then(Value::as_str)
        .is_some_and(|protocol| DIRECT_PROTOCOLS.contains(&protocol))
        && !value
            .get("has_drm")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// Choose the best admissible H.264 picture and AAC sound.
///
/// Deadpan's qualified decoder admits MP4/H.264 picture and AAC sound, so VP9
/// and AV1 renditions are not candidates even when larger. Among admissible
/// picture streams prefer resolution, then frame rate, then bitrate. Among
/// sound streams prefer the original-language track, then the uncompressed
/// dynamic range (not YouTube's `-drc` variant), then bitrate.
pub fn select_formats(formats: &[Value]) -> Result<Selection, ImportError> {
    let video = formats
        .iter()
        .filter(|f| {
            direct(f)
                && f.get("vcodec")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.starts_with("avc1"))
                && f.get("acodec").and_then(Value::as_str) == Some("none")
                && f.get("ext").and_then(Value::as_str) == Some("mp4")
        })
        .filter_map(|f| format(f, "vcodec"))
        .filter(|f| {
            safe_format_id(&f.format_id)
                && f.width.is_some_and(|w| w > 0)
                && f.height.is_some_and(|h| h > 0)
        })
        .max_by(|a, b| {
            (a.height, a.width)
                .cmp(&(b.height, b.width))
                .then(a.fps.unwrap_or(0.0).total_cmp(&b.fps.unwrap_or(0.0)))
                .then(
                    a.bitrate_kbps
                        .unwrap_or(0.0)
                        .total_cmp(&b.bitrate_kbps.unwrap_or(0.0)),
                )
        });
    let audio = formats
        .iter()
        .filter(|f| {
            direct(f)
                && f.get("acodec")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.starts_with("mp4a"))
                && f.get("vcodec").and_then(Value::as_str) == Some("none")
                && matches!(f.get("ext").and_then(Value::as_str), Some("m4a" | "mp4"))
        })
        .filter_map(|f| {
            let language = f
                .get("language_preference")
                .and_then(Value::as_i64)
                .unwrap_or(-1);
            let format = format(f, "acodec")?;
            Some((language, format))
        })
        .filter(|(_, f)| safe_format_id(&f.format_id))
        .max_by(|(la, a), (lb, b)| {
            let compressed = |f: &Format| f.format_id.ends_with("-drc");
            la.cmp(lb)
                .then(compressed(b).cmp(&compressed(a)))
                .then(
                    a.bitrate_kbps
                        .unwrap_or(0.0)
                        .total_cmp(&b.bitrate_kbps.unwrap_or(0.0)),
                )
                .then(a.sample_rate.cmp(&b.sample_rate))
        })
        .map(|(_, format)| format);
    match (video, audio) {
        (Some(video), Some(audio)) => Ok(Selection { video, audio }),
        (None, _) => Err(ImportError::new(
            "YouTubeFormatUnavailable",
            "YouTube offered no direct H.264 MP4 picture stream; Deadpan currently imports only H.264 picture",
        )),
        (_, None) => Err(ImportError::new(
            "YouTubeFormatUnavailable",
            "YouTube offered no direct AAC sound stream; Deadpan currently imports only AAC sound",
        )),
    }
}

/// Validate yt-dlp's single-video metadata for `id`.
pub fn parse_metadata(
    json: &[u8],
    id: &VideoId,
    limits: &ImportLimits,
) -> Result<VideoMetadata, ImportError> {
    let invalid = |why: &str| {
        ImportError::new(
            "YouTubeMetadataInvalid",
            format!("the downloader returned unusable metadata: {why}"),
        )
    };
    let value: Value = serde_json::from_slice(json).map_err(|_| invalid("not JSON"))?;
    if value.is_null() {
        return Err(ImportError::new(
            "YouTubeVideoUnavailable",
            "This video is unavailable.",
        ));
    }
    match value.get("_type").and_then(Value::as_str) {
        None | Some("video") => {}
        Some(_) => {
            return Err(ImportError::new(
                "YouTubePlaylistRefused",
                "the URL resolved to a playlist or several videos; choose a specific video",
            ));
        }
    }
    if value.get("id").and_then(Value::as_str) != Some(id.as_str()) {
        return Err(invalid("it describes a different video"));
    }
    if value.get("extractor_key").and_then(Value::as_str) != Some("Youtube") {
        return Err(invalid("it did not come from the YouTube extractor"));
    }
    let live = value
        .get("is_live")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || matches!(
            value.get("live_status").and_then(Value::as_str),
            Some("is_live" | "is_upcoming" | "post_live")
        );
    if live {
        return Err(ImportError::new(
            "YouTubeLiveUnsupported",
            "live, upcoming and still-processing streams cannot be imported; retry once the recording is available",
        ));
    }
    let duration = number(&value, "duration")
        .filter(|d| *d > 0.0)
        .ok_or_else(|| invalid("no duration"))?;
    if duration > limits.max_duration_seconds as f64 {
        return Err(ImportError::new(
            "YouTubeTooLong",
            format!(
                "the video lasts {duration:.0} s; imports are limited to {} s",
                limits.max_duration_seconds
            ),
        ));
    }
    let formats = value
        .get("formats")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("no formats"))?;
    Ok(VideoMetadata {
        id: id.as_str().to_owned(),
        title: text(&value, "title").ok_or_else(|| invalid("no title"))?,
        author: text(&value, "channel").or_else(|| text(&value, "uploader")),
        author_id: text(&value, "channel_id").or_else(|| text(&value, "uploader_id")),
        author_url: https(text(&value, "channel_url").or_else(|| text(&value, "uploader_url"))),
        license: text(&value, "license"),
        upload_date: text(&value, "upload_date"),
        duration_seconds: duration,
        thumbnail_url: https(text(&value, "thumbnail")),
        selection: select_formats(formats)?,
    })
}

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub yt_dlp: Option<String>,
    pub ejs: Option<String>,
    pub js_runtimes: Option<String>,
    pub deno: Option<String>,
    pub matches_pins: bool,
}

/// Run both helpers and report the versions they actually load.
pub fn probe(helpers: &Helpers) -> Result<ProbeReport, CliError> {
    let workspace = Workspace::new()?;
    let cancelled = AtomicBool::new(false);
    let mut arguments = base_arguments(helpers, None);
    arguments.push("--verbose".into());
    // Without a URL yt-dlp prints its diagnostics, then a usage error.
    helpers.recheck()?;
    let run = run_helper(
        HelperCommand {
            executable: &helpers.yt_dlp,
            arguments: &arguments,
            private: workspace.path(),
            current_dir: &workspace.path().join("work"),
            max_stdout: 64 * 1024,
            overflow: ImportError::new("DownloaderOutputTooLarge", "yt-dlp printed too much"),
            timeout: Duration::from_secs(60),
        },
        &cancelled,
        || Ok(()),
    )?;
    let debug = |prefix: &str| {
        run.stderr
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .map(|rest| rest.trim().chars().take(400).collect::<String>())
    };
    let yt_dlp = debug("[debug] yt-dlp version ");
    let ejs = debug("[debug] Optional libraries:").and_then(|libraries| {
        libraries.split(", ").find_map(|library| {
            library
                .trim()
                .strip_prefix("yt_dlp_ejs-")
                .map(str::to_owned)
        })
    });
    let js_runtimes = debug("[debug] JS runtimes:");
    let deno = run_helper(
        HelperCommand {
            executable: &helpers.deno,
            arguments: &["--version".into()],
            private: workspace.path(),
            current_dir: &workspace.path().join("work"),
            max_stdout: 64 * 1024,
            overflow: ImportError::new("DownloaderOutputTooLarge", "deno printed too much"),
            timeout: Duration::from_secs(30),
        },
        &cancelled,
        || Ok(()),
    )?;
    let deno = String::from_utf8_lossy(&deno.stdout)
        .lines()
        .next()
        .map(|line| line.trim().chars().take(200).collect::<String>());
    let matches_pins = yt_dlp
        .as_deref()
        .is_some_and(|v| v.contains(YT_DLP.version))
        && ejs.as_deref() == Some(EJS_VERSION)
        && js_runtimes.as_deref() == Some(&format!("deno-{}", DENO.version))
        && deno
            .as_deref()
            .is_some_and(|v| v.starts_with(&format!("deno {}", DENO.version)));
    Ok(ProbeReport {
        yt_dlp,
        ejs,
        js_runtimes,
        deno,
        matches_pins,
    })
}

/// The SIGINT/SIGTERM cancellation flag for a long-running command.
///
/// Every signal only requests cancellation; there is no immediate exit. Each
/// stage polls the flag, tears down its helper process group and drops the
/// private workspace (with any cookie copy) before the command returns.
/// Workspaces of processes that were killed outright are removed by the
/// next import's sweep.
pub(crate) fn interrupt_flag() -> Result<Arc<AtomicBool>, CliError> {
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    Ok(cancelled)
}

/// Inputs for one acquisition.
pub struct UrlImport<'a> {
    pub package: &'a Path,
    pub url: &'a str,
    pub cookies: Option<&'a Path>,
    pub helpers: &'a Helpers,
    pub media_worker: &'a Path,
    pub limits: ImportLimits,
    pub cancelled: &'a AtomicBool,
}

#[derive(Debug, Serialize)]
pub struct CreatedFromUrl {
    /// The package that holds the Ready project.
    pub package: PathBuf,
    pub created: CreatedOriginal,
    pub provenance: RemoteOriginalProvenance,
}

/// Room kept free beyond what an import step needs.
const SPACE_MARGIN: u64 = 512 * 1024 * 1024;

fn require_space(path: &Path, bytes: u64) -> Result<(), ImportError> {
    let available = deadpan_models::packs::available_space(path).map_err(|error| {
        ImportError::new(
            "YouTubeInsufficientSpace",
            format!("cannot read free space at {}: {error}", path.display()),
        )
    })?;
    if available < bytes.saturating_add(SPACE_MARGIN) {
        return Err(ImportError::new(
            "YouTubeInsufficientSpace",
            format!(
                "{} has {available} bytes free; this import needs about {} more",
                path.display(),
                bytes.saturating_add(SPACE_MARGIN)
            ),
        ));
    }
    Ok(())
}

fn directory_bytes(directory: &Path) -> u64 {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.metadata().ok())
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len())
                .sum()
        })
        .unwrap_or(0)
}

fn downloaded(directory: &Path, format: &Format) -> Result<File, ImportError> {
    let path = directory.join(format!("{}.{}", format.format_id, format.ext));
    let incomplete = || {
        ImportError::new(
            "YouTubeDownloadIncomplete",
            format!(
                "format {} did not download completely; retry the import",
                format.format_id
            ),
        )
    };
    let metadata = fs::symlink_metadata(&path).map_err(|_| incomplete())?;
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || format
            .declared_bytes
            .is_some_and(|bytes| bytes != metadata.len())
    {
        return Err(incomplete());
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|_| incomplete())
}

/// Filesystem name for the assembled Original; untrusted text never forms a path.
fn original_file_name(title: &str, id: &VideoId) -> String {
    /// The stem's byte budget, leaving room for " [ID].mp4" in a 255-byte name.
    const MAX_STEM_BYTES: usize = 200;
    let replaced: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | ':' | '\\') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let label = single_original::display_label(&replaced);
    let mut end = label.len().min(MAX_STEM_BYTES);
    while !label.is_char_boundary(end) {
        end -= 1;
    }
    let stem = label[..end].trim().trim_start_matches('.').trim();
    if stem.is_empty() {
        format!("YouTube {id}.mp4")
    } else {
        format!("{stem} [{id}].mp4")
    }
}

/// Inputs for metadata inspection, before any transfer or package creation.
pub struct Inspection<'a> {
    pub url: &'a str,
    pub cookies: Option<&'a Path>,
    pub helpers: &'a Helpers,
    pub limits: ImportLimits,
    pub cancelled: &'a AtomicBool,
}

/// One inspected video, ready to download once the user confirms.
///
/// It owns the locked private workspace, the inspected metadata JSON that the
/// download reuses and any private cookie copy. Dropping it (for example when
/// the user declines) removes all of them; nothing has been transferred and no
/// package exists.
pub struct Inspected {
    id: VideoId,
    metadata: VideoMetadata,
    estimate: Option<u64>,
    info: Vec<u8>,
    workspace: Workspace,
    cookies: Option<PrivateCookies>,
}

impl Inspected {
    pub fn video_id(&self) -> &VideoId {
        &self.id
    }

    pub fn metadata(&self) -> &VideoMetadata {
        &self.metadata
    }

    /// Declared or approximate bytes of both selected streams, when known.
    pub fn estimated_bytes(&self) -> Option<u64> {
        self.estimate
    }
}

/// Progress events are reported at most this often during the transfer.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Run `--dump-single-json` and validate the result for one video.
///
/// Playlists, live streams, inadmissible formats and size bounds are refused
/// here, before any media transfer or package creation.
pub fn inspect(
    request: &Inspection<'_>,
    events: &mut dyn FnMut(Value) -> Result<(), CliError>,
) -> Result<Inspected, CliError> {
    let id = normalize(request.url).map_err(ImportError::from)?;
    let workspace = Workspace::new()?;
    let cookies = request
        .cookies
        .map(|path| PrivateCookies::copy(path, &workspace))
        .transpose()?;
    let cookie_path = cookies.as_ref().map(PrivateCookies::path);

    events(
        serde_json::json!({ "event": "fetching_metadata", "video_id": id.as_str(), "source_url": id.watch_url() }),
    )?;
    request.helpers.recheck()?;
    let run = run_helper(
        HelperCommand {
            executable: &request.helpers.yt_dlp,
            arguments: &metadata_arguments(request.helpers, cookie_path, &id),
            private: workspace.path(),
            current_dir: &workspace.path().join("work"),
            max_stdout: request.limits.max_metadata_bytes,
            overflow: ImportError::new(
                "YouTubeMetadataTooLarge",
                "the video's metadata exceeded its size bound",
            ),
            timeout: request.limits.metadata_timeout,
        },
        request.cancelled,
        || Ok(()),
    )?;
    if !run.status.success() {
        return Err(classify_failure(&run.stderr).into());
    }
    let metadata = parse_metadata(&run.stdout, &id, &request.limits)?;
    let selection = &metadata.selection;
    let estimate = [&selection.video, &selection.audio]
        .iter()
        .map(|f| f.declared_bytes.or(f.approximate_bytes))
        .sum::<Option<u64>>();
    if estimate.is_some_and(|bytes| bytes > request.limits.max_download_bytes) {
        return Err(ImportError::new(
            "YouTubeTooLarge",
            format!(
                "the selected streams need about {} bytes; imports are limited to {} bytes",
                estimate.unwrap_or(0),
                request.limits.max_download_bytes
            ),
        )
        .into());
    }
    events(serde_json::json!({
        "event": "metadata",
        "video_id": metadata.id, "title": metadata.title, "author": metadata.author,
        "duration_seconds": metadata.duration_seconds, "thumbnail_url": metadata.thumbnail_url,
        "license": metadata.license, "selection": selection, "estimated_bytes": estimate,
        "notice": "You are responsible for having the rights to use this video.",
    }))?;
    Ok(Inspected {
        id,
        metadata,
        estimate,
        info: run.stdout,
        workspace,
        cookies,
    })
}

/// Names another sibling package when the given one is taken.
pub type Alternatives<'a> = &'a dyn Fn(&Path) -> Option<PathBuf>;

/// Inputs for the transfer and project creation of an inspected video.
pub struct Acquisition<'a> {
    pub package: &'a Path,
    /// Another sibling name when `package` is taken, even at the final
    /// rename; `None` refuses a taken path. The download is never discarded
    /// for a name another creator took first.
    pub alternatives: Option<Alternatives<'a>>,
    pub helpers: &'a Helpers,
    pub media_worker: &'a Path,
    pub limits: ImportLimits,
    pub cancelled: &'a AtomicBool,
}

/// Download exactly the inspected streams, assemble them and create the
/// single-Original project at `package`.
pub fn download_and_create(
    inspected: Inspected,
    import: &Acquisition<'_>,
    events: &mut dyn FnMut(Value) -> Result<(), CliError>,
) -> Result<CreatedFromUrl, CliError> {
    let Inspected {
        id,
        metadata,
        estimate,
        info: metadata_json,
        workspace,
        cookies,
    } = inspected;
    if import.alternatives.is_none() && fs::symlink_metadata(import.package).is_ok() {
        return Err(CliError::Usage(format!(
            "{} already exists",
            import.package.display()
        )));
    }
    let cookie_path = cookies.as_ref().map(PrivateCookies::path);
    let selection = &metadata.selection;
    if let Some(estimate) = estimate {
        // Downloads, then the assembled copy beside them, then retention.
        require_space(workspace.path(), estimate.saturating_mul(2))?;
        if let Some(parent) = import.package.parent() {
            require_space(parent, estimate)?;
        }
    }
    let info = workspace.path().join("info.json");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&info)
        .and_then(|mut file| file.write_all(&metadata_json))?;
    drop(metadata_json);
    let directory = workspace.path().join("download");
    let mut reported = Instant::now();
    let limit = import.limits.max_download_bytes;
    import.helpers.recheck()?;
    let arguments = download_arguments(import.helpers, cookie_path, &info, selection, limit);
    let transfer = run_helper(
        HelperCommand {
            executable: &import.helpers.yt_dlp,
            arguments: &arguments,
            private: workspace.path(),
            // Downloads land in the working directory through a relative
            // template, so the private path is never parsed as a template.
            current_dir: &directory,
            max_stdout: 1024 * 1024,
            overflow: ImportError::new(
                "DownloaderOutputTooLarge",
                "the downloader printed more output than allowed",
            ),
            timeout: import.limits.download_timeout,
        },
        import.cancelled,
        || {
            let bytes = directory_bytes(&directory);
            if bytes > limit {
                return Err(ImportError::new(
                    "YouTubeTooLarge",
                    format!("the download exceeded {limit} bytes"),
                ));
            }
            if reported.elapsed() >= PROGRESS_INTERVAL {
                reported = Instant::now();
                // Progress output failure does not stop the transfer.
                let _ = events(
                    serde_json::json!({ "event": "progress", "downloaded_bytes": bytes, "estimated_bytes": estimate }),
                );
            }
            Ok(())
        },
    )?;
    drop(cookies);
    fs::remove_file(&info)?;
    if !transfer.status.success() {
        return Err(classify_failure(&transfer.stderr).into());
    }
    let retrieved_at_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let mut video = downloaded(&directory, &selection.video)?;
    let mut audio = downloaded(&directory, &selection.audio)?;
    let video_bytes = video.metadata()?.len();
    let audio_bytes = audio.metadata()?.len();
    // Assembly writes about as much again; retention may copy it once more.
    require_space(workspace.path(), video_bytes + audio_bytes)?;

    events(
        serde_json::json!({ "event": "assembling", "video_bytes": video_bytes, "audio_bytes": audio_bytes }),
    )?;
    let original = workspace
        .path()
        .join(original_file_name(&metadata.title, &id));
    let output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&original)?;
    let assembly_cancelled = |error| match error {
        deadpan_media::ConversionError::Cancelled => {
            ImportError::new("ImportCancelled", "import was cancelled")
        }
        error => ImportError::new(
            "YouTubeAssemblyFailed",
            format!("the downloaded streams could not be assembled: {error}"),
        ),
    };
    // The picture download is disposable: append the sound to it instead of
    // copying both into a third file.
    let (video_length, audio_length) =
        deadpan_media::append_for_remux(&mut video, &mut audio, import.cancelled)
            .map_err(assembly_cancelled)?;
    drop(audio);
    deadpan_media::remux_joined(
        import.media_worker,
        video,
        video_length,
        audio_length,
        &output,
        deadpan_media::RemuxLimits {
            // Stream copy adds only container overhead; never exceed what
            // retention accepts.
            max_output_bytes: (video_length + audio_length)
                .saturating_add((video_length + audio_length) / 50)
                .saturating_add(16 * 1024 * 1024)
                .min(crate::live_project::preparation::original_byte_limit())
                .min(deadpan_media::protocol::MAX_REMUX_OUTPUT_BYTES),
            ..deadpan_media::RemuxLimits::default()
        },
        import.cancelled,
    )
    .map_err(assembly_cancelled)?;
    drop(output);
    fs::remove_dir_all(&directory)?;

    let role = |role, format: &Format, bytes| SelectedFormat {
        role,
        format_id: format.format_id.clone(),
        container: Some(format.ext.clone()),
        codec: Some(format.codec.clone()),
        width: format.width,
        height: format.height,
        fps_millis: format.fps.map(|fps| (fps * 1000.0).round() as u32),
        bitrate_kbps: format.bitrate_kbps.map(|kbps| kbps.round() as u32),
        byte_length: bytes,
    };
    let provenance = RemoteOriginalProvenance {
        schema: PROVENANCE_SCHEMA,
        service: "youtube".into(),
        source_id: id.as_str().to_owned(),
        source_url: id.watch_url(),
        title: metadata.title.clone(),
        author: metadata.author.clone(),
        author_id: metadata.author_id.clone(),
        author_url: metadata.author_url.clone(),
        license: metadata.license.clone(),
        upload_date: metadata.upload_date.clone(),
        duration_millis: Some((metadata.duration_seconds * 1000.0).round() as u64),
        thumbnail_url: metadata.thumbnail_url.clone(),
        retrieved_at_unix_seconds,
        helpers: vec![
            HelperVersion {
                name: YT_DLP.name.into(),
                version: import.helpers.yt_dlp_version.clone(),
            },
            HelperVersion {
                name: "yt-dlp-ejs".into(),
                version: EJS_VERSION.into(),
            },
            HelperVersion {
                name: DENO.name.into(),
                version: import.helpers.deno_version.clone(),
            },
        ],
        formats: vec![
            role(FormatRole::Video, &selection.video, video_bytes),
            role(FormatRole::Audio, &selection.audio, audio_bytes),
        ],
        assembly: ASSEMBLY.into(),
    };
    provenance.validate()?;
    events(serde_json::json!({ "event": "creating_project", "package": import.package }))?;
    let (package, created) = single_original::create_at_free_name(
        import.package,
        import.alternatives.unwrap_or(&|_| None),
        &original,
        &metadata.title,
        import.cancelled,
        |store, record| Ok(store.save_original_provenance(record.object().content(), &provenance)?),
    )?;
    Ok(CreatedFromUrl {
        package,
        created,
        provenance,
    })
}

/// Metadata, transfer, assembly and single-Original project creation in one
/// call, without a confirmation step between inspection and transfer.
pub fn create_from_url(
    import: &UrlImport<'_>,
    events: &mut dyn FnMut(Value) -> Result<(), CliError>,
) -> Result<CreatedFromUrl, CliError> {
    normalize(import.url).map_err(ImportError::from)?;
    if import.package.exists() {
        return Err(CliError::Usage(format!(
            "{} already exists",
            import.package.display()
        )));
    }
    let inspected = inspect(
        &Inspection {
            url: import.url,
            cookies: import.cookies,
            helpers: import.helpers,
            limits: import.limits,
            cancelled: import.cancelled,
        },
        events,
    )?;
    download_and_create(
        inspected,
        &Acquisition {
            package: import.package,
            alternatives: None,
            helpers: import.helpers,
            media_worker: import.media_worker,
            limits: import.limits,
            cancelled: import.cancelled,
        },
        events,
    )
}

fn usage() -> CliError {
    CliError::Usage(
        "usage: project create-from-url <project.deadpan> <https-youtube-url> [--cookies <file>] [--helpers <dir>]".into(),
    )
}

/// `project create-from-url <project.deadpan> <url> [--cookies <file>] [--helpers <dir>]`
pub(crate) fn run(arguments: &[&str]) -> Result<(), CliError> {
    let [package, url, options @ ..] = arguments else {
        return Err(usage());
    };
    let mut options = options;
    let mut cookies = None;
    let mut root = None;
    while let Some((option, rest)) = options.split_first() {
        match (*option, rest) {
            ("--cookies", [value, rest @ ..]) if cookies.is_none() => {
                cookies = Some(PathBuf::from(value));
                options = rest;
            }
            ("--helpers", [value, rest @ ..]) if root.is_none() => {
                root = Some(PathBuf::from(value));
                options = rest;
            }
            _ => return Err(usage()),
        }
    }
    let root = match root {
        Some(root) if root.is_absolute() => root,
        Some(_) => return Err(CliError::Usage("--helpers must be an absolute path".into())),
        None => super::helpers::default_root()?,
    };
    // Refuse malformed URLs before verifying helpers or touching the network.
    normalize(url).map_err(ImportError::from)?;
    let helpers = Helpers::resolve(&root)?;
    let media_worker = media_worker()?;
    let cancelled = interrupt_flag()?;
    let created = create_from_url(
        &UrlImport {
            package: Path::new(package),
            url,
            cookies: cookies.as_deref(),
            helpers: &helpers,
            media_worker: &media_worker,
            limits: ImportLimits::default(),
            cancelled: &cancelled,
        },
        &mut |event| super::emit(&event),
    )?;
    crate::write_json(
        &serde_json::json!({ "protocol": 1, "created": created.created, "provenance": created.provenance }),
    )
}

/// The isolated media worker installed beside this executable.
pub fn media_worker() -> Result<PathBuf, CliError> {
    let worker = std::env::current_exe()?
        .parent()
        .map(|directory| directory.join("deadpan-media-worker"))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            ImportError::new(
                "DownloaderAssemblyUnavailable",
                "deadpan-media-worker must be installed beside this executable",
            )
        })?;
    Ok(fs::canonicalize(worker)?)
}

#[cfg(test)]
mod tests;
