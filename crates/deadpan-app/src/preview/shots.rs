//! Shot detection of the Original, in the background.
//!
//! Shot detection needs no model, so it runs automatically once a
//! single-Original project's Original is ready and no stored analysis
//! exists. A job thread opens a read-only store, copies the verified
//! Original snapshot, decodes and measures every picture, and hands the
//! validated analysis to the project service, which saves it outside history
//! and publishes it with the workspace. Bounded source ranges follow the
//! latest visible Original picture; each completed range is privately cached
//! so cancellation retains useful work without claiming complete coverage.
//! The Original card shows progress or
//! the shot count, and a failure appears with Try again below the REUSE
//! heading.

use std::sync::atomic::AtomicU64;
use std::sync::mpsc;

use deadpan_analysis::ShotAnalysis;
use deadpan_store::{AccessMode, ProjectStore, ShotAnalysisKey};

use super::*;

enum Event {
    Progress {
        percent: u8,
        cache_failed: bool,
    },
    Done(Result<(ShotAnalysisKey, ShotAnalysis), String>),
    /// System sleep stopped the scan; its completed ranges stay cached.
    Interrupted,
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
    /// Sleep stopped the scan. It resumes from its cached ranges once awake.
    Interrupted,
}

#[derive(Default)]
pub(super) struct ShotJob {
    session: Option<u64>,
    interpretation: Option<deadpan_core::SourceInterpretation>,
    status: Status,
    events: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    /// The running scan's sleep epoch, kept until its result is submitted.
    run: Option<crate::jobs::RunCapture>,
    /// Latest visible Original ordinal, never a project-frame estimate.
    cursor: Arc<AtomicU64>,
    /// The running scan, and cancelled scans of earlier sessions that may
    /// still be finishing; all are joined briefly at exit.
    threads: Vec<std::thread::JoinHandle<()>>,
    /// A finished analysis not yet admitted by a busy project service.
    unsaved: Option<(ShotAnalysisKey, Arc<ShotAnalysis>)>,
    cache_failed: bool,
    attempts: u64,
    /// Shot count of the published analysis, keyed by its allocation, so the
    /// rule runs once per analysis rather than every frame.
    count: Option<(usize, usize)>,
    /// The package being scanned.
    scanning: Option<std::path::PathBuf>,
    /// Packages whose scan failed in this app run. Reopening one shows the
    /// failure instead of copying and decoding the whole Original again; Try
    /// again removes it.
    failed: Vec<(
        std::path::PathBuf,
        Option<deadpan_core::SourceInterpretation>,
        String,
    )>,
}

impl ShotJob {
    /// A different session or interpretation cancels the previous scan.
    fn reset(
        &mut self,
        session: Option<u64>,
        interpretation: Option<deadpan_core::SourceInterpretation>,
    ) {
        self.cancel.store(true, Ordering::Release);
        self.threads.retain(|thread| !thread.is_finished());
        *self = Self {
            session,
            interpretation,
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

    /// A scan that crossed sleep publishes nothing: drop its pending events
    /// and any buffered result. Its cached ranges remain for the resumed scan.
    fn interrupt(&mut self) {
        self.events = None;
        self.unsaved = None;
        self.scanning = None;
        self.run = None;
        self.status = Status::Interrupted;
    }

    /// A finished result still waiting for the busy service is rejected when
    /// its run crossed sleep.
    fn reject_interrupted_result(&mut self) {
        if self.unsaved.is_some()
            && self.run.as_ref().and_then(crate::jobs::RunCapture::stop)
                == Some(crate::jobs::Stop::Interrupted)
        {
            self.interrupt();
        }
    }

    /// Wake resumes an interrupted scan transparently from its cached
    /// ranges; this is not a retry of a failure, so nothing is remembered.
    fn resume_after_wake(&mut self, lifecycle: &crate::jobs::Lifecycle) {
        if self.status == Status::Interrupted && lifecycle.is_awake() {
            self.status = Status::Unchecked;
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
            Status::Interrupted => "interrupted",
        }
    }
}

/// The watcher stops the copy and decode at sleep without UI frames. A
/// scan that crossed sleep reports only its interruption, never its result.
fn scan_until_sleep(
    run: &crate::jobs::RunCapture,
    work: impl FnOnce() -> Result<(ShotAnalysisKey, ShotAnalysis), String>,
) -> Event {
    let result = run.run(work);
    if run.stop() == Some(crate::jobs::Stop::Interrupted) {
        Event::Interrupted
    } else {
        Event::Done(result)
    }
}

/// Scan the Original of the project at `package` from a read-only store.
/// Completed ranges resume from the private versioned cache.
fn scan(
    package: &std::path::Path,
    interpretation: Option<deadpan_core::SourceInterpretation>,
    cancel: &AtomicBool,
    handle: &crate::jobs::JobHandle,
    cursor: &AtomicU64,
    send: impl Fn(Event),
) -> Result<(ShotAnalysisKey, ShotAnalysis), String> {
    handle
        .wait_admitted(Some(cancel))
        .map_err(|stopped| format!("shot detection was {stopped} before it started"))?;
    handle.set_progress("Reading the Original", None);
    let deadline = std::time::Instant::now() + Duration::from_secs(6 * 60 * 60);
    let store = ProjectStore::open(package, AccessMode::ReadOnly).map_err(|e| e.to_string())?;
    let input = deadpan_cli::shots::prepare_shot_input(&store, None, cancel, deadline)
        .map_err(|e| e.to_string())?;
    if input.key.interpretation != interpretation {
        return Err("The Original interpretation changed before scanning started".into());
    }
    drop(store);
    let mut reported = (0, false);
    let options = deadpan_cli::shots::incremental::ShotRangeOptions {
        cache_directory: deadpan_cli::storage::UserStorage::current()
            .map(|storage| storage.caches.join("AnalysisRanges")),
        ..Default::default()
    };
    handle.set_progress("Finding shot ranges", None);
    let scan = deadpan_cli::shots::incremental::scan_shot_ranges(
        &input,
        cancel,
        deadline,
        options,
        cursor,
        || {
            handle.checkpoint(Some(cancel)).map_err(|stopped| {
                deadpan_cli::shots::ShotScanError::Failed(format!("shot detection was {stopped}"))
            })
        },
        |update| {
            // Only completed, disjoint source coverage contributes progress.
            let done = update.completed_pictures;
            let total = update.total_pictures.max(1);
            handle.set_progress("Finding shot ranges", Some(done as f32 / total as f32));
            let state = ((done * 100 / total) as u8, update.cache_failures > 0);
            // At most 102 tiny events, independent of source range count.
            if state != reported {
                reported = state;
                send(Event::Progress {
                    percent: state.0,
                    cache_failed: state.1,
                });
            }
        },
    )
    .map_err(|e| e.to_string())?;
    Ok((scan.key, scan.analysis))
}

impl DeadpanApp {
    /// Resolve the actual visible Original picture, including freezes,
    /// repeated plays, retimes and cutaways. Generated/blank pictures supply
    /// no invented source coordinate and retain the last meaningful priority.
    fn visible_shot_cursor(&self) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        let asset = original_asset(workspace)?;
        let index = workspace.sources.get(asset)?.video_index.as_deref()?;
        match self.view {
            View::Source
                if self.raw_source.is_none() && self.selected_source.as_ref() == Some(asset) =>
            {
                Some(
                    self.source_cursor
                        .min(index.frames().len().saturating_sub(1) as u64),
                )
            }
            View::Sequence => {
                let cursor = self
                    .sequence_cursor
                    .min(self.sequence_length().saturating_sub(1));
                let sample = workspace.plan.picture(ProjectFrame(cursor as i64)).ok()?;
                shot_picture_ordinal(index, &sample.picture)
            }
            _ => None,
        }
    }

    /// Advance shot detection once per outer frame.
    pub(super) fn reconcile_shots(&mut self, context: &egui::Context) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        let interpretation = self.workspace.as_ref().and_then(|workspace| {
            let Some(SingleSourceState::Ready { asset, .. }) = workspace.single_source.as_ref()
            else {
                return None;
            };
            workspace
                .document
                .source_interpretations()
                .get(asset)
                .copied()
        });
        if self.shots.session != session || self.shots.interpretation != interpretation {
            self.shots.reset(session, interpretation);
        }
        if (self.shots.events.is_some() || self.shots.status == Status::Unchecked)
            && let Some(cursor) = self.visible_shot_cursor()
        {
            self.shots.cursor.store(cursor, Ordering::Release);
        }
        while let Some(event) = self
            .shots
            .events
            .as_ref()
            .and_then(|events| events.try_recv().ok())
        {
            match event {
                Event::Progress {
                    percent,
                    cache_failed,
                } => {
                    self.shots.status = Status::Scanning(percent);
                    self.shots.cache_failed = cache_failed;
                }
                Event::Interrupted => self.shots.interrupt(),
                Event::Done(result) => {
                    self.shots.events = None;
                    match result {
                        Ok((key, analysis)) => {
                            self.shots.unsaved = Some((key, Arc::new(analysis)));
                        }
                        Err(error) => {
                            if let Some(package) = self.shots.scanning.take() {
                                self.shots
                                    .failed
                                    .push((package, interpretation, error.clone()));
                            }
                            self.shots.status = Status::Failed(error);
                        }
                    }
                }
            }
            context.request_repaint();
        }
        self.shots.reject_interrupted_result();
        self.shots
            .resume_after_wake(&self.service.jobs().lifecycle());
        if self.shots.status == Status::Interrupted {
            // Nothing else may repaint an idle window after wake.
            context.request_repaint_after(Duration::from_secs(1));
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
        if let Some((_, _, error)) = self
            .shots
            .failed
            .iter()
            .find(|(failed, choice, _)| *failed == package && *choice == interpretation)
        {
            self.shots.status = Status::Failed(error.clone());
            return;
        }
        self.shots.scanning = Some(package.clone());
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let run =
            crate::jobs::RunCapture::new(self.service.jobs().lifecycle(), Arc::clone(&cancel));
        self.shots.run = Some(run.clone());
        self.shots.cancel = Arc::clone(&cancel);
        self.shots.events = Some(receiver);
        self.shots.status = Status::Scanning(0);
        let repaint = context.clone();
        let cursor = self.shots.cursor.clone();
        let handle = self.service.jobs().register(
            crate::jobs::JobSpec::new(crate::jobs::JobKind::Shots, session)
                .detail("the Original")
                .cancel_flag(&cancel),
        );
        let spawned = std::thread::Builder::new()
            .name("deadpan-shots".into())
            .spawn(move || {
                let done = scan_until_sleep(&run, || {
                    scan(
                        &package,
                        interpretation,
                        &cancel,
                        &handle,
                        &cursor,
                        |event| {
                            let _ = sender.send(event);
                            repaint.request_repaint();
                        },
                    )
                });
                let _ = sender.send(done);
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
            Status::Scanning(0)
                if self
                    .service
                    .jobs()
                    .waiting_for(crate::jobs::JobKind::Shots, self.shots.session)
                    .is_some() =>
            {
                format!("{frames} frames · shots queued")
            }
            Status::Scanning(percent) => format!("{frames} frames · shots {percent}%"),
            Status::Saving(_) => format!("{frames} frames · shots 100%"),
            Status::Ready => match self.shot_count() {
                Some(1) => format!("{frames} frames · 1 shot"),
                Some(shots) => format!("{frames} frames · {shots} shots"),
                None => format!("{frames} decoded video frames"),
            },
            Status::Failed(_) => format!("{frames} decoded video frames"),
            Status::Interrupted => format!("{frames} frames · shots resume after sleep"),
        }
    }

    /// The REUSE heading, followed by a shot detection failure with Try again.
    pub(super) fn reuse_heading(&mut self, ui: &mut egui::Ui) {
        ui.label(style::section_title("REUSE", false));
        if self.shots.cache_failed && matches!(self.shots.status, Status::Scanning(_)) {
            ui.weak("Some private shot ranges could not be reused or saved; scanning continues.");
        }
        if let Status::Failed(error) = self.shots.status.clone() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Shot detection failed: {error}"),
            );
            if ui.add(style::row_action(ui, "Try again", "")).clicked() {
                if let Some(package) = self.workspace.as_ref().map(|workspace| &workspace.path) {
                    let interpretation = self.shots.interpretation;
                    self.shots.failed.retain(|(failed, choice, _)| {
                        failed != package || *choice != interpretation
                    });
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

fn shot_picture_ordinal(
    index: &deadpan_core::SourceFrameIndex,
    picture: &deadpan_plan::Picture,
) -> Option<u64> {
    picture
        .select_source_frame(index)
        .ok()
        .map(|picture| picture.identity.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis() -> (ShotAnalysisKey, ShotAnalysis) {
        let key = ShotAnalysisKey {
            content: "a".repeat(64),
            video_stream: 0,
            signature_version: deadpan_analysis::SIGNATURE_VERSION.into(),
            interpretation: None,
        };
        let changes = (0..4).map(|picture| if picture == 0 { [0; 3] } else { [1; 3] });
        (key, ShotAnalysis::from_changes(changes.collect()).unwrap())
    }

    #[test]
    fn sleep_stops_a_running_scan_and_wake_resumes_it_from_cached_ranges() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = crate::jobs::RunCapture::new(lifecycle.clone(), Arc::clone(&cancel));
        let (sender, receiver) = mpsc::channel();
        let mut job = ShotJob {
            session: Some(1),
            status: Status::Scanning(40),
            events: Some(receiver),
            cancel: Arc::clone(&cancel),
            run: Some(run.clone()),
            scanning: Some("package".into()),
            ..ShotJob::default()
        };
        // The scan stops only through its cancellation flag, as decoding does,
        // and would otherwise have produced a complete analysis.
        let event = scan_until_sleep(&run, || {
            lifecycle.will_sleep();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !cancel.load(Ordering::Acquire) {
                assert!(std::time::Instant::now() < deadline, "scan not stopped");
                std::thread::park_timeout(Duration::from_millis(5));
            }
            Ok(analysis())
        });
        assert!(matches!(event, Event::Interrupted));
        sender.send(event).unwrap();
        let Ok(Event::Interrupted) = job.events.as_ref().unwrap().try_recv() else {
            panic!("no interruption")
        };
        job.interrupt();
        assert_eq!(job.status, Status::Interrupted);
        assert!(job.unsaved.is_none() && job.events.is_none());
        assert!(job.failed.is_empty(), "sleep is not a remembered failure");
        job.resume_after_wake(&lifecycle);
        assert_eq!(job.status, Status::Interrupted, "asleep: no new scan");
        lifecycle.did_wake();
        job.resume_after_wake(&lifecycle);
        assert_eq!(job.status, Status::Unchecked, "wake resumes automatically");
    }

    #[test]
    fn a_buffered_result_from_a_run_that_crossed_sleep_is_never_saved() {
        for (slept, expected) in [(true, Status::Interrupted), (false, Status::Scanning(100))] {
            let lifecycle = crate::jobs::Lifecycle::default();
            let cancel = Arc::new(AtomicBool::new(false));
            let run = crate::jobs::RunCapture::new(lifecycle.clone(), Arc::clone(&cancel));
            // The scan finished before the notification was observed.
            let event = scan_until_sleep(&run, || Ok(analysis()));
            let Event::Done(Ok((key, analysis))) = event else {
                panic!("a scan that did not cross sleep keeps its result")
            };
            let mut job = ShotJob {
                session: Some(1),
                status: Status::Scanning(100),
                run: Some(run),
                unsaved: Some((key, Arc::new(analysis))),
                ..ShotJob::default()
            };
            if slept {
                lifecycle.will_sleep();
                lifecycle.did_wake();
            }
            job.reject_interrupted_result();
            assert_eq!(job.status, expected);
            assert_eq!(job.unsaved.is_some(), !slept);
        }
    }

    #[test]
    fn cancellation_before_sleep_stays_a_failure_with_try_again() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(true));
        let run = crate::jobs::RunCapture::new(lifecycle.clone(), cancel);
        lifecycle.will_sleep();
        lifecycle.did_wake();
        let event = scan_until_sleep(&run, || Err("shot detection was cancelled".into()));
        assert!(matches!(event, Event::Done(Err(_))));
    }

    #[test]
    fn visible_priority_uses_exact_pts_on_the_original_index_and_rejects_other_assets() {
        use deadpan_core::{
            AssetId, ExactRatio, IndexedSourceFrame, SourceFrameId, SourceFrameIndex, SourcePoint,
            SourceTimeBase, TerminalProvenance,
        };
        let asset = AssetId::new("original").unwrap();
        let time_base = SourceTimeBase::new(1, 1_000).unwrap();
        let frames = [10, 22, 59]
            .into_iter()
            .enumerate()
            .map(|(number, pts)| IndexedSourceFrame {
                identity: SourceFrameId(number as u64),
                pts,
                reported_duration: None,
                keyframe: number == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect();
        let index = SourceFrameIndex::new(
            asset.clone(),
            time_base,
            frames,
            75,
            TerminalProvenance::Explicit,
        )
        .unwrap();
        let picture = |asset, ticks| deadpan_plan::Picture::Freeze {
            asset,
            point: SourcePoint {
                ticks: ExactRatio::integer(ticks),
                time_base,
            },
        };
        assert_eq!(
            shot_picture_ordinal(&index, &picture(asset.clone(), 22)),
            Some(1)
        );
        assert_eq!(
            shot_picture_ordinal(&index, &picture(asset.clone(), 74)),
            Some(2)
        );
        assert_eq!(shot_picture_ordinal(&index, &picture(asset, 75)), None);
        assert_eq!(
            shot_picture_ordinal(&index, &picture(AssetId::new("generated").unwrap(), 22)),
            None
        );
        assert_eq!(
            shot_picture_ordinal(&index, &deadpan_plan::Picture::Blank),
            None
        );
    }
}
