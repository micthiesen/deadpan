//! One cancellable admission of an upcoming picture provider. The cache owns
//! one job for Generated and, only without a retained Original, one for Original.

use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Provider {
    Original,
    Generated {
        serving: Option<Arc<deadpan_core::GeneratedArtifact>>,
    },
}

/// The same complete immutable key used by the foreground picture reader.
pub(super) fn key(workspace: &Workspace, view: &ProjectView) -> Result<Option<SessionKey>, String> {
    let asset = match view {
        ProjectView::Source { asset, .. } => asset.clone(),
        ProjectView::Sequence { frame } => match workspace
            .plan
            .picture(*frame)
            .map_err(|e| e.to_string())?
            .picture
        {
            deadpan_plan::Picture::Source { asset, .. }
            | deadpan_plan::Picture::Freeze { asset, .. } => asset,
            deadpan_plan::Picture::Accepted {
                generated: Some(artifact),
                ..
            } => {
                return Ok(Some(SessionKey::Generated {
                    session: workspace.session,
                    media: Arc::new(GeneratedMediaKey::new(&workspace.document, &artifact)?),
                }));
            }
            _ => return Ok(None),
        },
    };
    let source = media_source(
        PictureMedia::Committed(workspace),
        &workspace.document,
        &asset,
    )?;
    Ok(Some(SessionKey::Project {
        session: workspace.session,
        asset,
        receipt: source.receipt.id().clone(),
        interpretation: source.interpretation,
    }))
}

#[derive(Clone, Copy)]
struct Found {
    virtual_frame: i64,
    frame: i64,
}

#[derive(Default)]
pub(super) struct Scan {
    examined: Option<(i64, i64, Option<Found>)>,
}

impl Scan {
    fn next(
        &mut self,
        from: i64,
        through: i64,
        window: Option<(i64, i64)>,
        mut generated: impl FnMut(i64) -> Result<bool, String>,
    ) -> Result<Option<i64>, String> {
        let (start, mut examined, mut found) = self
            .examined
            .filter(|(start, end, found)| {
                *start <= from
                    && from <= *end
                    && found.is_none_or(|found| from < found.virtual_frame)
            })
            .unwrap_or((from, from, None));
        if found.is_none() {
            let limit = through
                .min(examined.saturating_add(deadpan_media::lookahead::MAX_SCAN_FRAMES as i64));
            while examined < limit {
                examined += 1;
                let frame = match window {
                    Some((start, end)) if examined >= end => start + (examined - end),
                    _ => examined,
                };
                if generated(frame)? {
                    found = Some(Found {
                        virtual_frame: examined,
                        frame,
                    });
                    break;
                }
            }
        }
        self.examined = Some((start, examined, found));
        Ok(found
            .filter(|found| found.virtual_frame <= through)
            .map(|found| found.frame))
    }
}

struct Job {
    key: SessionKey,
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<Result<RetainedSession, String>>,
}

#[derive(PartialEq, Eq)]
struct ScanScope {
    session: u64,
    revision: deadpan_core::RevisionId,
    generation: deadpan_output::Generation,
    window: Option<(i64, i64)>,
    /// Skip the provider already serving pictures. Changing it changes which
    /// entries are eligible, even within the same revision and generation.
    provider: Provider,
}

#[derive(Default)]
pub(super) struct ProviderAhead {
    job: Option<Job>,
    failed: Option<SessionKey>,
    scan: Scan,
    scope: Option<ScanScope>,
}

impl ProviderAhead {
    pub(super) fn clear(&mut self) {
        self.stop_job();
        self.failed = None;
        self.scope = None;
        self.scan = Scan::default();
    }

    pub(super) fn stop_job(&mut self) {
        if let Some(job) = self.job.take() {
            job.stop.store(true, Ordering::Release);
            // Admission observes stop between bounded reads/native calls.
            // A completed result is dropped here on this worker too.
            let _ = job.thread.join();
        }
    }

    pub(super) fn target(
        &mut self,
        request: &Request,
        provider: Provider,
    ) -> Option<(Arc<Workspace>, i64, SessionKey)> {
        let Work::Project {
            workspace,
            view: ProjectView::Sequence { frame },
        } = &request.work
        else {
            return None;
        };
        let generation = request.ticket.transport?;
        let from = frame.0;
        let (window, through) = if let Some(hint) = request.loop_ahead {
            let (start, end, through) = hint.window();
            if !(start..end).contains(&from) {
                return None;
            }
            (
                Some((start, end)),
                through.min(from.saturating_add(end - start)),
            )
        } else {
            let rate = workspace.document.presentation_basis().frame_rate;
            let horizon = u64::from(rate.numerator()) * LOOKAHEAD_HORIZON.as_secs()
                / u64::from(rate.denominator());
            (
                None,
                from.saturating_add(i64::try_from(horizon).unwrap_or(i64::MAX))
                    .min(workspace.plan.duration().frames().saturating_sub(1)),
            )
        };
        let scope = ScanScope {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            generation,
            window,
            provider: provider.clone(),
        };
        if self.scope.as_ref() != Some(&scope) {
            self.scan = Scan::default();
            self.scope = Some(scope);
        }
        let frame = self
            .scan
            .next(from, through, window, |frame| {
                workspace
                    .plan
                    .picture(ProjectFrame(frame))
                    .map(|sample| match &provider {
                        Provider::Original => matches!(
                            sample.picture,
                            deadpan_plan::Picture::Source { .. }
                                | deadpan_plan::Picture::Freeze { .. }
                        ),
                        Provider::Generated { serving } => matches!(
                            sample.picture,
                            deadpan_plan::Picture::Accepted {
                                generated: Some(artifact),
                                ..
                            } if serving.as_ref() != Some(&artifact)
                        ),
                    })
                    .map_err(|error| error.to_string())
            })
            .ok()
            .flatten()?;
        let key = key(
            workspace,
            &ProjectView::Sequence {
                frame: ProjectFrame(frame),
            },
        )
        .ok()??;
        Some((Arc::clone(workspace), frame, key))
    }

    pub(super) fn start(&mut self, workspace: Arc<Workspace>, frame: i64, key: SessionKey) {
        if self.failed.as_ref() == Some(&key)
            || self
                .job
                .as_ref()
                .is_some_and(|job| job.key == key && !job.stop.load(Ordering::Acquire))
        {
            return;
        }
        self.stop_job();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let prepared_key = key.clone();
        let thread = std::thread::Builder::new()
            .name("provider-ahead".into())
            .spawn(move || {
                if matches!(prepared_key, SessionKey::Project { .. }) {
                    let mut retained = None;
                    project_picture(
                        &workspace,
                        &workspace.document,
                        &workspace.plan,
                        &ProjectView::Sequence {
                            frame: ProjectFrame(frame),
                        },
                        &cancelled,
                        &mut retained,
                    )?;
                    return retained
                        .filter(|session| session.key == prepared_key)
                        .ok_or_else(|| "Prepared Original has a different media key.".into());
                }
                let mut session = admit_generated(
                    &workspace.generated,
                    &workspace.document,
                    prepared_key,
                    &cancelled,
                )?;
                let picture = workspace
                    .plan
                    .picture(ProjectFrame(frame))
                    .map_err(|error| error.to_string())?;
                let id = picture
                    .picture
                    .select_source_frame(session.source.index().index())
                    .map_err(|error| error.to_string())?
                    .identity;
                session
                    .source
                    .frame(id, FRAME_TIMEOUT, &cancelled)
                    .map_err(|error| error.to_string())?;
                workspace
                    .generated
                    .check_live(&cancelled)
                    .map_err(|error| error.to_string())?;
                Ok(session)
            });
        match thread {
            Ok(thread) => self.job = Some(Job { key, stop, thread }),
            Err(_) => self.failed = Some(key),
        }
    }

    pub(super) fn stop_flag(&self) -> Option<Arc<AtomicBool>> {
        self.job.as_ref().map(|job| Arc::clone(&job.stop))
    }

    #[cfg(test)]
    pub(super) fn finished(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
    }

    /// The requested picture has caught the preparation. Finish the same
    /// admission, observing foreground cancellation instead of opening a
    /// second decoder. Every later read still rechecks authority.
    pub(super) fn take_for(
        &mut self,
        key: &SessionKey,
        cancelled: &AtomicBool,
    ) -> Result<Option<RetainedSession>, String> {
        if self.job.as_ref().is_none_or(|job| &job.key != key) {
            self.stop_job();
            return Ok(None);
        }
        while self
            .job
            .as_ref()
            .is_some_and(|job| !job.thread.is_finished())
        {
            if cancelled.load(Ordering::Acquire) {
                self.stop_job();
                return Err("Picture preparation was cancelled.".into());
            }
            std::thread::park_timeout(Duration::from_millis(2));
        }
        let job = self.job.take().expect("matching job");
        match job.thread.join() {
            Ok(Ok(session)) if !job.stop.load(Ordering::Acquire) => Ok(Some(session)),
            _ => {
                self.failed = Some(job.key);
                // The ordinary reader retries once when the actual picture
                // is requested and reports any authoritative failure.
                Ok(None)
            }
        }
    }
}

impl Drop for ProviderAhead {
    fn drop(&mut self) {
        self.stop_job();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_hold_precedes_a_later_loop_seam_and_excludes_outside_frames() {
        let mut scan = Scan::default();
        let mut visited = Vec::new();
        let found = scan
            .next(12, 42, Some((10, 40)), |frame| {
                visited.push(frame);
                Ok(frame == 18 || frame == 11)
            })
            .unwrap();
        assert_eq!(found, Some(18));
        assert_eq!(visited, (13..=18).collect::<Vec<_>>());
        let found = scan
            .next(38, 68, Some((10, 40)), |frame| {
                assert!((10..40).contains(&frame));
                Ok(frame == 11)
            })
            .unwrap();
        assert_eq!(found, Some(11));
    }

    #[test]
    fn scan_is_bounded_and_reuses_examined_original_pictures() {
        let mut scan = Scan::default();
        let mut count = 0;
        assert_eq!(
            scan.next(0, 5000, None, |_| {
                count += 1;
                Ok(false)
            })
            .unwrap(),
            None
        );
        assert_eq!(count, 1024);
        let mut first = None;
        assert_eq!(
            scan.next(1, 5000, None, |frame| {
                first.get_or_insert(frame);
                Ok(frame == 1500)
            })
            .unwrap(),
            Some(1500)
        );
        assert_eq!(first, Some(1025));
        assert_eq!(
            scan.next(2, 499, None, |_| panic!("cached horizon"))
                .unwrap(),
            None
        );
        assert_eq!(
            scan.next(3, 1500, None, |_| panic!("cached hit")).unwrap(),
            Some(1500)
        );
    }

    #[test]
    fn backward_navigation_restarts_the_scan() {
        let mut scan = Scan::default();
        assert_eq!(
            scan.next(50, 80, None, |frame| Ok(frame == 60)).unwrap(),
            Some(60)
        );
        assert_eq!(
            scan.next(10, 30, None, |frame| Ok(frame == 20)).unwrap(),
            Some(20)
        );
    }
}
