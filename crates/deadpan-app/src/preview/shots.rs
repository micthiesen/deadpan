//! Shot detection of the Original, in the background.
//!
//! Shot detection needs no model, so it runs automatically once a
//! single-Original project's Original is ready and no stored analysis
//! exists. A job thread opens a read-only store, copies the verified
//! Original snapshot, decodes and measures every picture, and hands the
//! validated analysis to the project service, which saves it outside history
//! and publishes it with the workspace. The Original card shows progress or
//! the shot count, and a failure appears with Try again below the REUSE
//! heading.

use std::sync::mpsc;

use deadpan_analysis::ShotAnalysis;
use deadpan_store::{AccessMode, ProjectStore, ShotAnalysisKey};

use super::*;

enum Event {
    Progress(u8),
    Done(Result<(ShotAnalysisKey, ShotAnalysis), String>),
}

#[derive(Debug, Clone, PartialEq, Default)]
enum Status {
    /// Not yet checked for this project session.
    #[default]
    Unchecked,
    Scanning(u8),
    /// Waiting for the service to store this attempt.
    Saving(u64),
    Failed(String),
    Ready,
}

#[derive(Default)]
pub(super) struct ShotJob {
    session: Option<u64>,
    status: Status,
    events: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    /// The running scan, and cancelled scans of earlier sessions that may
    /// still be finishing; all are joined briefly at exit.
    threads: Vec<std::thread::JoinHandle<()>>,
    /// A finished analysis not yet admitted by a busy project service.
    unsaved: Option<(ShotAnalysisKey, Arc<ShotAnalysis>)>,
    attempts: u64,
    /// Shot count of the published analysis, keyed by its allocation, so the
    /// rule runs once per analysis rather than every frame.
    count: Option<(usize, usize)>,
    /// The package being scanned.
    scanning: Option<std::path::PathBuf>,
    /// Packages whose scan failed in this app run. Reopening one shows the
    /// failure instead of copying and decoding the whole Original again; Try
    /// again removes it.
    failed: Vec<(std::path::PathBuf, String)>,
}

impl ShotJob {
    /// A new project session cancels the previous session's scan.
    fn reset(&mut self, session: Option<u64>) {
        self.cancel.store(true, Ordering::Release);
        self.threads.retain(|thread| !thread.is_finished());
        *self = Self {
            session,
            attempts: self.attempts,
            threads: std::mem::take(&mut self.threads),
            failed: std::mem::take(&mut self.failed),
            ..Self::default()
        };
    }

    pub(super) fn receive_save(&mut self, save: Option<crate::project::TranscriptSave>) {
        let Some(save) = save else {
            return;
        };
        if self.session != Some(save.session) || self.status != Status::Saving(save.attempt) {
            return;
        }
        if let Some(error) = save.error {
            self.status = Status::Failed(format!("the shots were not saved: {error}"));
        }
    }

    /// Cancel and wait briefly, so no decoder outlives the process's exit.
    pub(super) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        for thread in std::mem::take(&mut self.threads) {
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }

    /// Replay-visible status name.
    #[cfg(feature = "ui-harness")]
    pub(super) fn status_name(&self) -> &'static str {
        match self.status {
            Status::Unchecked => "unchecked",
            Status::Scanning(_) => "scanning",
            Status::Saving(_) => "saving",
            Status::Failed(_) => "failed",
            Status::Ready => "ready",
        }
    }
}

/// Scan the Original of the project at `package` from a read-only store.
fn scan(
    package: &std::path::Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u8),
) -> Result<(ShotAnalysisKey, ShotAnalysis), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60 * 60);
    let store = ProjectStore::open(package, AccessMode::ReadOnly).map_err(|e| e.to_string())?;
    let input = deadpan_cli::shots::prepare_shot_input(&store, None, cancel, deadline)
        .map_err(|e| e.to_string())?;
    drop(store);
    let mut reported = 0;
    let scan = deadpan_cli::shots::scan_shots(&input, cancel, deadline, |done, total| {
        let percent = (done * 100 / total.max(1)) as u8;
        if percent != reported {
            reported = percent;
            progress(percent);
        }
    })
    .map_err(|e| e.to_string())?;
    Ok((scan.key, scan.analysis))
}

impl DeadpanApp {
    /// Advance shot detection once per outer frame.
    pub(super) fn reconcile_shots(&mut self, context: &egui::Context) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        if self.shots.session != session {
            self.shots.reset(session);
        }
        while let Some(event) = self
            .shots
            .events
            .as_ref()
            .and_then(|events| events.try_recv().ok())
        {
            match event {
                Event::Progress(percent) => self.shots.status = Status::Scanning(percent),
                Event::Done(result) => {
                    self.shots.events = None;
                    match result {
                        Ok((key, analysis)) => {
                            self.shots.unsaved = Some((key, Arc::new(analysis)));
                        }
                        Err(error) => {
                            if let Some(package) = self.shots.scanning.take() {
                                self.shots.failed.push((package, error.clone()));
                            }
                            self.shots.status = Status::Failed(error);
                        }
                    }
                }
            }
            context.request_repaint();
        }
        self.save_shots(session, context);
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.shot_analysis.is_some() {
            self.shots.status = Status::Ready;
            return;
        }
        if self.shots.status == Status::Ready {
            // A newly opened workspace without an analysis re-checks.
            self.shots.status = Status::Unchecked;
        }
        if self.shots.status != Status::Unchecked
            || self.shots.events.is_some()
            || self.shots.unsaved.is_some()
            || !matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
        {
            return;
        }
        let package = workspace.path.clone();
        if let Some((_, error)) = self
            .shots
            .failed
            .iter()
            .find(|(failed, _)| *failed == package)
        {
            self.shots.status = Status::Failed(error.clone());
            return;
        }
        self.shots.scanning = Some(package.clone());
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.shots.cancel = Arc::clone(&cancel);
        self.shots.events = Some(receiver);
        self.shots.status = Status::Scanning(0);
        let repaint = context.clone();
        let spawned = std::thread::Builder::new()
            .name("deadpan-shots".into())
            .spawn(move || {
                let result = scan(&package, &cancel, |percent| {
                    let _ = sender.send(Event::Progress(percent));
                    repaint.request_repaint();
                });
                let _ = sender.send(Event::Done(result));
                repaint.request_repaint();
            });
        match spawned {
            Ok(thread) => self.shots.threads.push(thread),
            Err(error) => {
                self.shots.events = None;
                self.shots.status = Status::Failed(error.to_string());
            }
        }
    }

    /// Submit a finished analysis without the side effects of a user
    /// command, retrying while the service is busy.
    fn save_shots(&mut self, session: Option<u64>, context: &egui::Context) {
        let Some(session) = session else {
            self.shots.unsaved = None;
            return;
        };
        let Some((key, analysis)) = self.shots.unsaved.as_ref() else {
            return;
        };
        // Saves use their own lane and never occupy the user-command slot,
        // but still yield while a user command is pending.
        if self.service.is_busy() || self.service.annotation_busy() {
            context.request_repaint_after(Duration::from_millis(100));
            return;
        }
        self.shots.attempts += 1;
        let attempt = self.shots.attempts;
        let request = ProjectRequest::SaveShotAnalysis {
            expected_session: session,
            attempt,
            key: key.clone(),
            analysis: Arc::clone(analysis),
        };
        match self.service.submit(request) {
            Ok(()) => {
                self.shots.unsaved = None;
                self.shots.status = Status::Saving(attempt);
            }
            Err(_) if self.service.annotation_busy() => {
                context.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.shots.unsaved = None;
                self.shots.status = Status::Failed(format!("the shots were not saved: {error}"));
            }
        }
    }

    /// The REUSE heading, with the shot status on its right so progress and
    /// the result add no rail height. A failure adds its message and Try
    /// again below.
    /// The Original card's detail line: its picture count and shot status.
    pub(super) fn original_detail(&mut self) -> String {
        let detail = self.shot_detail();
        match self.proxies.detail() {
            Some(proxy) => format!("{detail} · {proxy}"),
            None => detail,
        }
    }

    fn shot_detail(&mut self) -> String {
        let frames = self.source_length();
        match self.shots.status.clone() {
            Status::Unchecked => format!("{frames} frames · finding shots"),
            Status::Scanning(percent) => format!("{frames} frames · shots {percent}%"),
            Status::Saving(_) => format!("{frames} frames · shots 100%"),
            Status::Ready => match self.shot_count() {
                Some(1) => format!("{frames} frames · 1 shot"),
                Some(shots) => format!("{frames} frames · {shots} shots"),
                None => format!("{frames} decoded video frames"),
            },
            Status::Failed(_) => format!("{frames} decoded video frames"),
        }
    }

    /// The REUSE heading, followed by a shot detection failure with Try again.
    pub(super) fn reuse_heading(&mut self, ui: &mut egui::Ui) {
        ui.label(style::section_title("REUSE", false));
        if let Status::Failed(error) = self.shots.status.clone() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Shot detection failed: {error}"),
            );
            if ui.add(style::row_action(ui, "Try again", "")).clicked() {
                if let Some(package) = self.workspace.as_ref().map(|workspace| &workspace.path) {
                    self.shots.failed.retain(|(failed, _)| failed != package);
                }
                self.shots.status = Status::Unchecked;
            }
        }
    }

    /// Shots in the published analysis, computed once per analysis.
    fn shot_count(&mut self) -> Option<usize> {
        let published = self.workspace.as_ref()?.shot_analysis.as_ref()?;
        let identity = Arc::as_ptr(published) as usize;
        if let Some((cached, count)) = self.shots.count
            && cached == identity
        {
            return Some(count);
        }
        let analysis = &published.analysis;
        let count = analysis.boundaries().len() + usize::from(analysis.pictures() > 0);
        self.shots.count = Some((identity, count));
        Some(count)
    }
}
