//! Native New-from-URL: one bounded YouTube import job at a time.
//!
//! The job thread runs the headless acquisition in two phases. It inspects the
//! video first (helper verification, metadata, stream selection and every
//! refusal that needs no transfer), then waits for the user's explicit
//! confirmation before any media transfer or package creation. Downloading,
//! assembly, retention and qualification then run through the ordinary
//! single-Original path; the UI opens the finished package like any other
//! project. Helper installation is a separate, explicit job and is never
//! started implicitly. Cancellation is cooperative: every stage tears down its
//! helper process group and private directory before the job reports.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deadpan_cli::CliError;
use deadpan_cli::youtube::acquire::VideoMetadata;
use deadpan_cli::youtube::helpers::{DENO, YT_DLP};

use crate::library::ProjectLibrary;

/// Bytes the explicit helper installation downloads.
pub const INSTALL_DOWNLOAD_BYTES: u64 = YT_DLP.download_bytes + DENO.download_bytes;
/// Bytes the installed helpers occupy.
pub const INSTALLED_BYTES: u64 = YT_DLP.executable_bytes + DENO.executable_bytes;

/// An actionable failure with the library's stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

impl Failure {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn cancelled() -> Self {
        Self::new("ImportCancelled", "Import was cancelled.")
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.code.as_str(),
            "ImportCancelled" | "DownloaderInstallCancelled"
        )
    }

    /// The helpers are not installed; offer the explicit install.
    pub fn needs_downloader(&self) -> bool {
        self.code == "DownloaderNotInstalled"
    }

    /// The video needs an explicit signed-in cookies file.
    pub fn needs_cookies(&self) -> bool {
        matches!(
            self.code.as_str(),
            "YouTubeAgeRestricted" | "YouTubeSignInRequired" | "YouTubeRateLimited"
        )
    }

    /// What the user can do next, in app terms. The library's own message
    /// remains the detail; its CLI instructions are never shown as guidance.
    pub fn guidance(&self) -> &'static str {
        match self.code.as_str() {
            "YouTubeUrlInvalid" => "Paste an https:// YouTube video link.",
            "YouTubePlaylistNeedsVideo" | "YouTubePlaylistRefused" => {
                "Open one video from the playlist and paste its link."
            }
            "DownloaderNotInstalled" => "Install the downloader to continue.",
            "DownloaderHelperInvalid" => {
                "A downloader file was changed or damaged. Deadpan never replaces it automatically: remove the folder named above, then install again."
            }
            "DownloaderUnsupportedPlatform" => {
                "The pinned downloader runs only on Apple Silicon Macs."
            }
            "DownloaderInstallFailed" => "Check the connection and try the install again.",
            "YouTubeVideoUnavailable" | "YouTubeVideoPrivate" => {
                "Choose a video that is public or unlisted."
            }
            "YouTubeRegionRestricted" => "This video is not available in your region.",
            "YouTubeAgeRestricted" | "YouTubeSignInRequired" => {
                "Choose a cookies file exported from a signed-in browser, then try again."
            }
            "YouTubeRateLimited" => {
                "YouTube is limiting requests. Wait a while, or use a cookies file."
            }
            "YouTubeLiveUnsupported" => "Try again once the recording is available.",
            "YouTubeFormatUnavailable" => {
                "Deadpan imports H.264 picture with AAC sound, which this video does not offer."
            }
            "YouTubeExtractorFailed" => "YouTube changed; the pinned downloader needs an update.",
            "YouTubeNetworkFailed" | "YouTubeDownloadFailed" | "YouTubeDownloadIncomplete" => {
                "Check the connection and try again."
            }
            "YouTubeTooLong" | "YouTubeTooLarge" => "Choose a shorter video.",
            "YouTubeInsufficientSpace" => "Free some disk space, then try again.",
            "YouTubeAssemblyFailed" | "DownloaderAssemblyUnavailable" => {
                "The media worker must be installed beside Deadpan."
            }
            "YouTubeCookiesInvalid" => "Choose a Netscape-format cookies file of at most 1 MiB.",
            "DownloaderTimeout" => "The download took too long. Try again.",
            "ProjectExists" => {
                "Another project took every candidate name; rename one, then try again."
            }
            "ProjectOpenFailed" => {
                "The project is complete in your library. Use Open project… (⌘O) and choose the package named above; importing again would download it again."
            }
            "YouTubeConfirmationExpired" => {
                "Press Enter to check the video again; its details and streams are fetched afresh."
            }
            _ => "Try again.",
        }
    }
}

impl From<CliError> for Failure {
    fn from(error: CliError) -> Self {
        match error {
            CliError::Import(error) => Self::new(error.code, error.message),
            CliError::Usage(message) if message.ends_with("already exists") => {
                Self::new("ProjectExists", message)
            }
            error => Self::new("DownloaderFailed", error.to_string()),
        }
    }
}

/// What the user starts: one URL and an optional explicit cookies file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub cookies: Option<PathBuf>,
}

/// The inspected video and its selected streams, shown before any transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub metadata: VideoMetadata,
    pub estimated_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    CheckingDownloader,
    FetchingDetails,
    Downloading {
        downloaded: u64,
        estimated: Option<u64>,
    },
    Assembling,
    Qualifying,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstallProgress {
    pub helper: &'static str,
    pub completed: u64,
    pub total: u64,
}

/// The acquisition backend. Production uses [`Pinned`]; tests and UI replay
/// substitute a scripted downloader at this seam only.
pub trait Downloader: Send + Sync {
    /// Download and verify the pinned helpers. Only called on explicit request.
    fn install(
        &self,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<(), Failure>;

    /// Inspect, ask `confirm` for the destination, then download and create.
    /// `confirm` blocks until the user decides; an error stops the import
    /// before any transfer.
    fn import(
        &self,
        request: &Request,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(Stage),
        confirm: &mut dyn FnMut(&Preview) -> Result<PathBuf, Failure>,
    ) -> Result<PathBuf, Failure>;
}

/// The pinned helpers under the managed install root.
pub struct Pinned {
    root: Option<PathBuf>,
    media_worker: Option<PathBuf>,
}

impl Pinned {
    /// `None` uses `~/Library/Application Support/Deadpan/helpers`.
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            media_worker: None,
        }
    }

    /// Use this media worker instead of the one beside the executable.
    #[cfg(test)]
    pub fn with_media_worker(mut self, worker: PathBuf) -> Self {
        self.media_worker = Some(worker);
        self
    }

    fn root(&self) -> Result<PathBuf, Failure> {
        match &self.root {
            Some(root) => Ok(root.clone()),
            None => Ok(deadpan_cli::youtube::helpers::default_root()?),
        }
    }
}

impl Downloader for Pinned {
    fn install(
        &self,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(InstallProgress),
    ) -> Result<(), Failure> {
        use deadpan_cli::youtube::helpers::{self, BUNDLE, USER_AGENT};
        helpers::supported_platform()?;
        let root = self.root()?;
        let transport = deadpan_models::packs::HttpsTransport::with_user_agent(USER_AGENT);
        let mut before = 0;
        for pin in &BUNDLE {
            progress(InstallProgress {
                helper: pin.name,
                completed: before,
                total: INSTALL_DOWNLOAD_BYTES,
            });
            helpers::install(pin, &root, &transport, cancelled, |done| {
                progress(InstallProgress {
                    helper: pin.name,
                    completed: before + done,
                    total: INSTALL_DOWNLOAD_BYTES,
                });
            })?;
            before += pin.download_bytes;
        }
        Ok(())
    }

    fn import(
        &self,
        request: &Request,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(Stage),
        confirm: &mut dyn FnMut(&Preview) -> Result<PathBuf, Failure>,
    ) -> Result<PathBuf, Failure> {
        use deadpan_cli::youtube::acquire::{self, Acquisition, ImportLimits, Inspection};
        use deadpan_cli::youtube::helpers::Helpers;
        progress(Stage::CheckingDownloader);
        // Refuse a malformed URL before hashing helpers or touching the network.
        deadpan_cli::youtube::url::normalize(&request.url)
            .map_err(deadpan_cli::youtube::ImportError::from)
            .map_err(CliError::from)?;
        let helpers = Helpers::resolve(&self.root()?)?;
        let media_worker = match &self.media_worker {
            Some(worker) => worker.clone(),
            None => acquire::media_worker()?,
        };
        let limits = ImportLimits::default();
        let inspected = acquire::inspect(
            &Inspection {
                url: &request.url,
                cookies: request.cookies.as_deref(),
                helpers: &helpers,
                limits,
                cancelled,
            },
            &mut |event| {
                if event["event"] == "fetching_metadata" {
                    progress(Stage::FetchingDetails);
                }
                Ok(())
            },
        )?;
        let preview = Preview {
            metadata: inspected.metadata().clone(),
            estimated_bytes: inspected.estimated_bytes(),
        };
        let package = confirm(&preview)?;
        progress(Stage::Downloading {
            downloaded: 0,
            estimated: preview.estimated_bytes,
        });
        let alternatives = |_: &Path| crate::library::next_free_package(&package);
        let created = acquire::download_and_create(
            inspected,
            &Acquisition {
                package: &package,
                alternatives: Some(&alternatives),
                helpers: &helpers,
                media_worker: &media_worker,
                limits,
                cancelled,
            },
            &mut |event| {
                match event["event"].as_str() {
                    Some("progress") => progress(Stage::Downloading {
                        downloaded: event["downloaded_bytes"].as_u64().unwrap_or(0),
                        estimated: event["estimated_bytes"].as_u64(),
                    }),
                    Some("assembling") => progress(Stage::Assembling),
                    Some("creating_project") => progress(Stage::Qualifying),
                    _ => {}
                }
                Ok(())
            },
        )?;
        Ok(created.package)
    }
}

/// The current job state, as the UI shows it.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Status {
    #[default]
    Idle,
    Working(Stage),
    /// Inspected; nothing transferred. Waiting for explicit confirmation.
    Confirm {
        preview: Box<Preview>,
        destination: PathBuf,
    },
    /// The helpers are missing; installing them needs explicit consent.
    NeedsDownloader,
    Installing(InstallProgress),
    /// Cancellation was requested; waiting for helper teardown.
    Cancelling,
    Cancelled,
    Failed(Failure),
    /// Ready and complete at this path; the UI opens it.
    Created(PathBuf),
}

impl Status {
    /// A job thread may be running or waiting for a decision.
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Working(_) | Self::Confirm { .. } | Self::Installing(_) | Self::Cancelling
        )
    }
}

enum Event {
    Stage(Stage),
    Install(InstallProgress),
    Preview(Box<Preview>, PathBuf),
    Finished(Result<Option<PathBuf>, Failure>),
}

/// The job thread's side of the event queue. Every event wakes the UI.
struct Sink {
    events: SyncSender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Sink {
    /// Progress is lossy: the UI only needs the latest value.
    fn progress(&self, event: Event) {
        if self.events.try_send(event).is_ok() {
            (self.wake)();
        }
    }

    /// Previews and the final result are never dropped.
    fn send(&self, event: Event) -> bool {
        let sent = self.events.send(event).is_ok();
        (self.wake)();
        sent
    }
}

struct Running {
    cancelled: Arc<AtomicBool>,
    events: Receiver<Event>,
    decision: Option<SyncSender<bool>>,
    thread: Option<JoinHandle<()>>,
    install: bool,
}

/// Owner of the single YouTube job. All methods are cheap and nonblocking
/// except [`Jobs::shutdown`], which waits a bounded time.
pub struct Jobs {
    downloader: Arc<dyn Downloader>,
    library: Option<ProjectLibrary>,
    wake: Arc<dyn Fn() + Send + Sync>,
    running: Option<Running>,
    status: Status,
    request: Option<Request>,
    /// After an explicit install succeeds, continue the retained import.
    resume_after_install: bool,
    /// Inspected details expire: the private workspace holds a cookie copy
    /// and the selected stream URLs stop working after a while.
    confirm_timeout: Duration,
    /// Cancellation arrived after the Ready package was already published.
    completed_after_cancel: bool,
}

/// How long inspected details wait for confirmation.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Events a job may queue ahead of the UI; progress beyond this is dropped.
const EVENT_CAPACITY: usize = 32;

impl Jobs {
    pub fn new(
        downloader: Arc<dyn Downloader>,
        library: Option<ProjectLibrary>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            downloader,
            library,
            wake,
            running: None,
            status: Status::Idle,
            request: None,
            resume_after_install: false,
            confirm_timeout: CONFIRM_TIMEOUT,
            completed_after_cancel: false,
        }
    }

    #[cfg(test)]
    pub fn with_confirm_timeout(mut self, timeout: Duration) -> Self {
        self.confirm_timeout = timeout;
        self
    }

    /// The last import finished its package before a cancellation took
    /// effect, so it opens instead of being removed.
    pub fn completed_after_cancel(&self) -> bool {
        self.completed_after_cancel
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    #[cfg(test)]
    pub fn request(&self) -> Option<&Request> {
        self.request.as_ref()
    }

    /// A job thread exists, including one that is finishing cancellation.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Start inspecting `request`. Refused while another job runs.
    pub fn start(&mut self, request: Request) -> Result<(), String> {
        if self.running.is_some() {
            return Err("Finish or cancel the current YouTube import first.".into());
        }
        if let Err(error) = deadpan_cli::youtube::url::normalize(&request.url) {
            let error = deadpan_cli::youtube::ImportError::from(error);
            self.status = Status::Failed(Failure::new(error.code, error.message));
            return Ok(());
        }
        self.request = Some(request.clone());
        self.resume_after_install = false;
        self.completed_after_cancel = false;
        let downloader = self.downloader.clone();
        let library = self.library.clone();
        let confirm_timeout = self.confirm_timeout;
        self.spawn(false, move |cancelled, sink, decisions| {
            let mut confirm = |preview: &Preview| -> Result<PathBuf, Failure> {
                let library = match &library {
                    Some(library) => library.clone(),
                    None => ProjectLibrary::documents()
                        .map_err(|error| Failure::new("DownloaderFailed", error))?,
                };
                let fallback = format!("YouTube {}", preview.metadata.id);
                let destination = library
                    .unused_package(&preview.metadata.title, &fallback)
                    .map_err(|error| Failure::new("DownloaderFailed", error))?;
                if !sink.send(Event::Preview(
                    Box::new(preview.clone()),
                    destination.clone(),
                )) {
                    return Err(Failure::cancelled());
                }
                let expires = Instant::now() + confirm_timeout;
                loop {
                    if cancelled.load(Ordering::Acquire) {
                        return Err(Failure::cancelled());
                    }
                    if Instant::now() >= expires {
                        return Err(Failure::new(
                            "YouTubeConfirmationExpired",
                            format!(
                                "the details were not confirmed within {} minutes, so the private download directory was removed",
                                confirm_timeout.as_secs() / 60
                            ),
                        ));
                    }
                    match decisions.recv_timeout(Duration::from_millis(50)) {
                        Ok(true) => return Ok(destination),
                        Ok(false) | Err(RecvTimeoutError::Disconnected) => {
                            return Err(Failure::cancelled());
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
            };
            let result = downloader.import(
                &request,
                cancelled,
                &mut |stage| sink.progress(Event::Stage(stage)),
                &mut confirm,
            );
            result.map(Some)
        });
        self.status = Status::Working(Stage::CheckingDownloader);
        Ok(())
    }

    /// Explicitly install the pinned helpers, then continue the import.
    pub fn install(&mut self) -> Result<(), String> {
        if self.running.is_some() {
            return Err("Finish or cancel the current YouTube import first.".into());
        }
        let downloader = self.downloader.clone();
        self.spawn(true, move |cancelled, sink, _| {
            downloader
                .install(cancelled, &mut |progress| {
                    sink.progress(Event::Install(progress));
                })
                .map(|()| None)
        });
        self.resume_after_install = self.request.is_some();
        self.status = Status::Installing(InstallProgress {
            helper: YT_DLP.name,
            completed: 0,
            total: INSTALL_DOWNLOAD_BYTES,
        });
        Ok(())
    }

    fn spawn(
        &mut self,
        install: bool,
        work: impl FnOnce(&AtomicBool, &Sink, &Receiver<bool>) -> Result<Option<PathBuf>, Failure>
        + Send
        + 'static,
    ) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (events, receive) = mpsc::sync_channel(EVENT_CAPACITY);
        let (decision, decisions) = mpsc::sync_channel(1);
        let flag = cancelled.clone();
        let sink = Sink {
            events,
            wake: self.wake.clone(),
        };
        let thread = std::thread::Builder::new()
            .name("deadpan-youtube".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    work(&flag, &sink, &decisions)
                }))
                .unwrap_or_else(|_| {
                    Err(Failure::new(
                        "DownloaderFailed",
                        "The YouTube import stopped unexpectedly.",
                    ))
                });
                // The final report waits for queue space; a dropped owner
                // (shutdown) disconnects the queue instead of blocking.
                sink.send(Event::Finished(result));
            });
        match thread {
            Ok(thread) => {
                self.running = Some(Running {
                    cancelled,
                    events: receive,
                    decision: Some(decision),
                    thread: Some(thread),
                    install,
                });
            }
            Err(error) => {
                self.status = Status::Failed(Failure::new(
                    "DownloaderFailed",
                    format!("Cannot start the YouTube import: {error}"),
                ));
            }
        }
    }

    /// Start the transfer of the inspected video.
    pub fn confirm(&mut self) -> bool {
        let Status::Confirm { preview, .. } = &self.status else {
            return false;
        };
        let estimated = preview.estimated_bytes;
        let Some(sender) = self.running.as_mut().and_then(|r| r.decision.take()) else {
            return false;
        };
        if sender.try_send(true).is_err() {
            return false;
        }
        self.status = Status::Working(Stage::Downloading {
            downloaded: 0,
            estimated,
        });
        true
    }

    /// Request cooperative cancellation of the running job, or leave a
    /// finished state.
    pub fn cancel(&mut self) {
        if let Some(running) = &mut self.running {
            running.cancelled.store(true, Ordering::Release);
            if let Some(sender) = running.decision.take() {
                let _ = sender.try_send(false);
            }
            self.resume_after_install = false;
            self.status = Status::Cancelling;
        } else if !matches!(self.status, Status::Created(_)) {
            self.status = Status::Idle;
        }
    }

    /// Leave a finished, failed or cancelled state.
    pub fn dismiss(&mut self) {
        if self.running.is_none() && !matches!(self.status, Status::Created(_)) {
            self.status = Status::Idle;
        }
    }

    /// Report a failure that happened after the job, such as opening the
    /// created package. Ignored while a job runs.
    pub fn fail(&mut self, failure: Failure) {
        if self.running.is_none() {
            self.status = Status::Failed(failure);
        }
    }

    /// The finished package, once; the caller opens it.
    pub fn take_created(&mut self) -> Option<PathBuf> {
        match std::mem::take(&mut self.status) {
            Status::Created(path) => {
                self.request = None;
                Some(path)
            }
            status => {
                self.status = status;
                None
            }
        }
    }

    /// Apply queued job events. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(running) = &mut self.running else {
                return changed;
            };
            let event = match running.events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => Event::Finished(Err(Failure::new(
                    "DownloaderFailed",
                    "The YouTube import stopped unexpectedly.",
                ))),
            };
            changed = true;
            let cancelling = self.status == Status::Cancelling;
            match event {
                Event::Stage(stage) if !cancelling => {
                    // A late progress report never moves back to confirmation.
                    if !matches!(self.status, Status::Confirm { .. }) {
                        self.status = Status::Working(stage);
                    }
                }
                Event::Install(progress) if !cancelling => {
                    self.status = Status::Installing(progress);
                }
                Event::Preview(preview, destination) if !cancelling => {
                    self.status = Status::Confirm {
                        preview,
                        destination,
                    };
                }
                Event::Stage(_) | Event::Install(_) | Event::Preview(..) => {}
                Event::Finished(result) => {
                    let mut running = self.running.take().expect("checked above");
                    if let Some(thread) = running.thread.take() {
                        let _ = thread.join();
                    }
                    self.finish(running.install, cancelling, result);
                }
            }
        }
    }

    fn finish(
        &mut self,
        install: bool,
        cancelling: bool,
        result: Result<Option<PathBuf>, Failure>,
    ) {
        self.status = match result {
            Ok(Some(path)) => {
                // Publication is atomic: once the Ready package exists, a
                // late Escape cannot remove it, and it opens like any other.
                self.completed_after_cancel = cancelling;
                Status::Created(path)
            }
            Ok(None) if install && self.resume_after_install => {
                self.resume_after_install = false;
                if let Some(request) = self.request.clone() {
                    self.status = Status::Idle;
                    let _ = self.start(request);
                    return;
                }
                Status::Idle
            }
            Ok(None) => Status::Idle,
            Err(failure) if cancelling || failure.is_cancelled() => Status::Cancelled,
            Err(failure) if failure.needs_downloader() => Status::NeedsDownloader,
            Err(failure) => Status::Failed(failure),
        };
    }

    /// Cancel and wait up to `limit` for the job thread to finish its
    /// teardown. Returns whether no job remains.
    pub fn shutdown(&mut self, limit: Duration) -> bool {
        self.cancel();
        let deadline = Instant::now() + limit;
        while self.running.is_some() && Instant::now() < deadline {
            self.poll();
            if self.running.is_some() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.running.is_none()
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Release);
        }
    }
}

/// `yt-dlp 2026.08.19` or `Deno 2.9.7`.
pub fn helper_label(name: &str) -> String {
    match deadpan_cli::youtube::helpers::BUNDLE
        .iter()
        .find(|pin| pin.name == name)
    {
        Some(pin) if pin.name == DENO.name => format!("Deno {}", pin.version),
        Some(pin) => format!("{} {}", pin.name, pin.version),
        None => name.to_owned(),
    }
}

/// Human-readable byte count with one decimal (decimal megabytes).
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// `1:02:03` or `2:26` from seconds.
pub fn duration_label(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The selected picture stream in one line.
pub fn picture_summary(preview: &Preview) -> String {
    let video = &preview.metadata.selection.video;
    let mut parts = Vec::new();
    if let (Some(width), Some(height)) = (video.width, video.height) {
        parts.push(format!("{width} × {height}"));
    }
    if let Some(fps) = video.fps {
        parts.push(if fps.fract() == 0.0 {
            format!("{fps:.0} fps")
        } else {
            format!("{fps:.3} fps")
        });
    }
    parts.push(format!("H.264 ({})", video.codec));
    parts.join(" · ")
}

/// The selected sound stream in one line.
pub fn sound_summary(preview: &Preview) -> String {
    let audio = &preview.metadata.selection.audio;
    let mut parts = vec![format!("AAC ({})", audio.codec)];
    if let Some(rate) = audio.sample_rate {
        parts.push(format!("{:.1} kHz", f64::from(rate) / 1000.0));
    }
    if let Some(kbps) = audio.bitrate_kbps {
        parts.push(format!("{kbps:.0} kb/s"));
    }
    parts.join(" · ")
}

/// The package's last three path components, such as
/// `Documents/Deadpan/Name.deadpan`, for display.
pub fn destination_label(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .rev()
        .take(3)
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// A deterministic stand-in for tests and UI replay. It never runs yt-dlp or
/// touches the network: it reports the stages of a real import, then creates
/// the project from a local fixture through the real single-Original path.
#[cfg(any(test, feature = "ui-harness"))]
pub mod scripted {
    use std::collections::VecDeque;
    use std::sync::{Condvar, Mutex};

    use deadpan_cli::youtube::acquire::{Format, Selection};

    use super::*;

    /// Takes the destination name, as another creator would.
    pub type TakeName = Box<dyn FnOnce(&Path) + Send>;

    pub struct Scripted {
        pub original: PathBuf,
        pub preview: Preview,
        installed: AtomicBool,
        /// Failures returned by the next imports, before inspection completes.
        failures: Mutex<VecDeque<Failure>>,
        /// The transfer waits at 40% until released, so replay can observe
        /// progress and cancellation deterministically.
        gate: (Mutex<bool>, Condvar),
        /// Requests seen by `import`, in order.
        pub requests: Mutex<Vec<Request>>,
        /// Runs once with the confirmed destination before project creation,
        /// to witness a name another creator takes meanwhile.
        pub take_during_build: Mutex<Option<TakeName>>,
    }

    pub fn preview() -> Preview {
        Preview {
            metadata: VideoMetadata {
                id: "Z4C82eyhwgU".into(),
                title: "Caminandes 2: Gran Dillama".into(),
                author: Some("Blender".into()),
                author_id: None,
                author_url: None,
                license: Some("Creative Commons Attribution license (reuse allowed)".into()),
                upload_date: Some("20141130".into()),
                duration_seconds: 146.0,
                thumbnail_url: Some("https://i.ytimg.com/vi/Z4C82eyhwgU/maxresdefault.jpg".into()),
                selection: Selection {
                    video: Format {
                        format_id: "137".into(),
                        ext: "mp4".into(),
                        codec: "avc1.640028".into(),
                        width: Some(1920),
                        height: Some(1080),
                        fps: Some(24.0),
                        bitrate_kbps: Some(2890.0),
                        sample_rate: None,
                        declared_bytes: Some(52_740_343),
                        approximate_bytes: None,
                        note: Some("1080p".into()),
                    },
                    audio: Format {
                        format_id: "140".into(),
                        ext: "m4a".into(),
                        codec: "mp4a.40.2".into(),
                        width: None,
                        height: None,
                        fps: None,
                        bitrate_kbps: Some(129.5),
                        sample_rate: Some(44_100),
                        declared_bytes: Some(2_365_262),
                        approximate_bytes: None,
                        note: None,
                    },
                },
            },
            estimated_bytes: Some(55_105_605),
        }
    }

    impl Scripted {
        pub fn new(original: PathBuf, installed: bool) -> Self {
            Self {
                original,
                preview: preview(),
                installed: AtomicBool::new(installed),
                failures: Mutex::default(),
                gate: (Mutex::new(false), Condvar::new()),
                requests: Mutex::default(),
                take_during_build: Mutex::new(None),
            }
        }

        pub fn fail_next(&self, failure: Failure) {
            self.failures
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push_back(failure);
        }

        /// Let held transfers continue past 40%.
        pub fn release(&self) {
            *self
                .gate
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = true;
            self.gate.1.notify_all();
        }

        pub fn installed(&self) -> bool {
            self.installed.load(Ordering::Acquire)
        }

        fn wait(&self, cancelled: &AtomicBool) -> Result<(), Failure> {
            let mut released = self
                .gate
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while !*released {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Failure::cancelled());
                }
                released = self
                    .gate
                    .1
                    .wait_timeout(released, Duration::from_millis(10))
                    .unwrap_or_else(|error| error.into_inner())
                    .0;
            }
            Ok(())
        }
    }

    impl Downloader for Scripted {
        fn install(
            &self,
            cancelled: &AtomicBool,
            progress: &mut dyn FnMut(InstallProgress),
        ) -> Result<(), Failure> {
            for (helper, completed) in [
                (YT_DLP.name, YT_DLP.download_bytes / 2),
                (DENO.name, YT_DLP.download_bytes + DENO.download_bytes / 2),
            ] {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Failure::new(
                        "DownloaderInstallCancelled",
                        "installation was cancelled",
                    ));
                }
                progress(InstallProgress {
                    helper,
                    completed,
                    total: INSTALL_DOWNLOAD_BYTES,
                });
                std::thread::sleep(Duration::from_millis(20));
            }
            self.installed.store(true, Ordering::Release);
            Ok(())
        }

        fn import(
            &self,
            request: &Request,
            cancelled: &AtomicBool,
            progress: &mut dyn FnMut(Stage),
            confirm: &mut dyn FnMut(&Preview) -> Result<PathBuf, Failure>,
        ) -> Result<PathBuf, Failure> {
            self.requests
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(request.clone());
            progress(Stage::CheckingDownloader);
            if !self.installed() {
                return Err(Failure::new(
                    "DownloaderNotInstalled",
                    "yt-dlp 2026.08.19 is not installed",
                ));
            }
            progress(Stage::FetchingDetails);
            if let Some(failure) = self
                .failures
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
            {
                return Err(failure);
            }
            let package = confirm(&self.preview)?;
            let estimated = self.preview.estimated_bytes;
            let total = estimated.unwrap_or(0);
            progress(Stage::Downloading {
                downloaded: 0,
                estimated,
            });
            progress(Stage::Downloading {
                downloaded: total * 2 / 5,
                estimated,
            });
            self.wait(cancelled)?;
            progress(Stage::Downloading {
                downloaded: total,
                estimated,
            });
            progress(Stage::Assembling);
            progress(Stage::Qualifying);
            if let Some(take) = self
                .take_during_build
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
            {
                take(&package);
            }
            let alternatives = |_: &Path| crate::library::next_free_package(&package);
            let (package, _) = deadpan_cli::single_original::create_at_free_name(
                &package,
                &alternatives,
                &self.original,
                &self.preview.metadata.title,
                cancelled,
                |_, _| Ok(()),
            )?;
            Ok(package)
        }
    }
}

#[cfg(test)]
mod tests;
