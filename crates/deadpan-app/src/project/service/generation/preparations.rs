//! Automatic AI conditioning shares the one AI job thread. The
//! writer claims and fulfils durable intentions; decoding and provenance
//! recovery remain on that thread. No path here accepts generated pictures.

use super::*;

const PAGE_SIZE: usize = 256;
const SCAN_INTERVAL: Duration = Duration::from_millis(250);
const CLAIM_RETRY_DELAY: Duration = Duration::from_secs(5);
const AUTOMATIC_TICKETS: u64 = 1 << 61;

/// A continuation belongs to one captured writer state. A retry or edit
/// invalidates even a partly scanned page, so earlier IDs remain reachable.
#[derive(Default)]
pub(super) struct Scan {
    key: Option<(u64, RevisionId, u64)>,
    after: Option<PreparationId>,
    complete: bool,
    checked: Option<Instant>,
    retry_at: Option<Instant>,
}

impl Scan {
    fn next_page(
        &mut self,
        key: (u64, RevisionId, u64),
        now: Instant,
    ) -> Option<Option<PreparationId>> {
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.after = None;
            self.complete = false;
            self.retry_at = None;
        }
        if self.retry_at.is_some_and(|retry| now < retry) {
            return None;
        }
        if self.complete
            || self
                .checked
                .is_some_and(|checked| now.saturating_duration_since(checked) < SCAN_INTERVAL)
        {
            return None;
        }
        self.checked = Some(now);
        Some(self.after.clone())
    }

    fn received(&mut self, count: usize, last: Option<PreparationId>) {
        self.complete = count < PAGE_SIZE;
        self.after = if self.complete { None } else { last };
    }

    fn claim_failed(&mut self, now: Instant) {
        // A failed write left the row queued. Restart the page walk after a
        // bounded delay, including when that row was on the final page.
        self.after = None;
        self.complete = false;
        self.checked = None;
        self.retry_at = Some(now + CLAIM_RETRY_DELAY);
    }

    pub(super) fn recheck(&mut self) {
        self.checked = None;
        self.retry_at = None;
    }
}

fn capacity_notice(
    notices: &[deadpan_store::generation_preparations::PreparationNotice],
) -> Option<String> {
    let total = notices.iter().fold(0u64, |sum, notice| match notice {
        deadpan_store::generation_preparations::PreparationNotice::QueueCapacity {
            total, ..
        } => sum.saturating_add(*total),
    });
    (total > 0).then(|| format!("AI queue was full: {total} older pending preparations were discarded. Timing edits stay saved."))
}

impl Service {
    pub(in crate::project::service) fn capture_preparation_notices(
        &mut self,
        notices: &[deadpan_store::generation_preparations::PreparationNotice],
    ) {
        if let Some(notice) = capacity_notice(notices) {
            self.generation.preparation_capacity_notice = Some(notice.clone());
            self.generation.pending_preparation_notice = Some(notice);
        }
    }

    pub(in crate::project::service) fn capture_preparation_output(
        &mut self,
        output: &serde_json::Value,
    ) {
        if let Some(notices) = output.get("generation_preparation_notices").or_else(|| {
            output
                .get("outcome")
                .and_then(|outcome| outcome.get("generation_preparation_notices"))
        }) {
            match serde_json::from_value::<
                Vec<deadpan_store::generation_preparations::PreparationNotice>,
            >(notices.clone())
            {
                Ok(notices) => self.capture_preparation_notices(&notices),
                Err(error) => self.preparation_warning(format!(
                    "The edit saved, but its AI queue notice could not be read: {error}"
                )),
            }
        }
    }

    pub(in crate::project::service) fn publish_preparation_notice(&mut self) {
        if let Some(notice) = self.generation.pending_preparation_notice.take() {
            self.message = Some(match self.message.take() {
                Some(message) => format!("{message}. {notice}"),
                None => notice,
            });
        }
    }

    pub(super) fn refresh_preparations(&mut self) {
        if self.workspace.is_none() {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        match store.generation_preparations(None, PAGE_SIZE) {
            Ok(records) => {
                self.generation.preparation_warning = None;
                if records.len() == PAGE_SIZE {
                    self.preparation_warning("Jobs shows the first 256 AI preparations. Later queued preparations still run.".into());
                }
                let workspace = self.workspace.as_ref().expect("workspace retained");
                self.generation.preparations = Arc::new(
                    records
                        .into_iter()
                        .map(|item| crate::project::generation::Preparation {
                            label: interrupted_pause_label(&workspace.document, &item.target)
                                .unwrap_or_else(|| {
                                    format!("Pause {} (scope unavailable)", item.target.node)
                                }),
                            id: item.id,
                            target: item.target,
                            revision: item.current_revision,
                            sequence: item.claim_sequence,
                            frames: item.duration.frames(),
                            state: item.state,
                            reason: item.reason,
                        })
                        .collect(),
                );
            }
            Err(error) => {
                self.generation.preparations = Arc::default();
                self.preparation_warning(format!("AI preparations could not be read: {error}"));
            }
        }
    }

    fn preparation_warning(&mut self, warning: String) {
        let mut warning = warning;
        if warning.len() > 2048 {
            let mut end = 2048;
            while !warning.is_char_boundary(end) {
                end -= 1;
            }
            warning.truncate(end);
        }
        self.generation.preparation_warning = Some(warning);
    }

    pub(super) fn discard_preparation(
        &mut self,
        session: u64,
        id: &PreparationId,
        sequence: u64,
    ) -> Result<()> {
        if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.session != session)
        {
            return Err("Project session changed before the request".into());
        }
        self.writer()?
            .cancel_generation_preparation(id, sequence)
            .map_err(display)?;
        if let Some(running) = &self.generation.running
            && running
                .preparation
                .as_ref()
                .is_some_and(|claim| &claim.preparation.id == id)
        {
            running.cancelled.store(true, Ordering::Release);
        }
        self.generation.variants_changed();
        self.message =
            Some("Discarded the AI preparation. The pause's timing is unchanged.".into());
        Ok(())
    }

    pub(super) fn pump_preparations(&mut self) -> bool {
        if self.pending_session_change.is_some()
            || self.shared.stopping.load(Ordering::Acquire)
            || self.shared.busy.load(Ordering::Acquire)
            || self.generation.running.is_some()
            || self.store.is_none()
            || self.workspace.is_none()
        {
            return false;
        }
        let workspace = self.workspace.as_ref().expect("checked workspace");
        let scan_key = (
            workspace.session,
            workspace.document.revision_id().clone(),
            self.generation.epoch,
        );
        let Some(after) = self
            .generation
            .preparation_scan
            .next_page(scan_key, Instant::now())
        else {
            return false;
        };
        // One bounded page per turn. A full page of blocked work cannot hide
        // later queued work; terminal records are excluded by the store.
        let records = match self
            .store
            .as_ref()
            .expect("checked store")
            .generation_preparations(after.as_ref(), PAGE_SIZE)
        {
            Ok(records) => records,
            Err(error) => {
                let message = format!("AI preparations could not be read: {error}");
                if self.message.as_ref() == Some(&message) {
                    return false;
                }
                self.message = Some(message);
                return true;
            }
        };
        self.generation
            .preparation_scan
            .received(records.len(), records.last().map(|item| item.id.clone()));
        let Some(item) = records
            .into_iter()
            .find(|item| item.state == PreparationState::Queued)
        else {
            return false;
        };
        let workspace = self.workspace.as_ref().expect("checked workspace");
        let session = workspace.session;
        let revision = workspace.document.revision_id().clone();
        let package = workspace.path.clone();
        let label = interrupted_pause_label(&workspace.document, &item.target)
            .unwrap_or_else(|| format!("Pause {}", item.target.node));
        // Reset only when idle and changing sessions, before retaining a claim.
        if self.generation.session != session {
            self.generation.reset(session);
        }
        let claim = match self.writer().and_then(|store| {
            store
                .claim_generation_preparation(&item.id, &revision)
                .map_err(display)
        }) {
            Ok(claim) => claim,
            Err(error) => {
                self.generation
                    .preparation_scan
                    .claim_failed(Instant::now());
                let message =
                    format!("AI preparation could not start: {error}. Retrying in 5 seconds.");
                let changed = self.message.as_ref() != Some(&message);
                self.message = Some(message);
                return changed;
            }
        };
        self.generation.variants_changed();
        let worker = match &self.generation.backend {
            Backend::Environment => BridgeRuntime::from_environment()
                .map(|runtime| Worker::Real(Box::new(runtime)))
                .map_err(display),
            #[cfg(any(test, feature = "ui-harness"))]
            Backend::Scripted(queue) => match queue.next() {
                Some(script) => match &script.unavailable {
                    Some(error) => Err(error.clone()),
                    None => Ok(Worker::Scripted {
                        queue: queue.clone(),
                        first: Some(script),
                    }),
                },
                None => Err("The scripted AI worker has no runs.".into()),
            },
        };
        let worker = match worker {
            Ok(worker) => worker,
            Err(reason) => {
                self.finish_preparation(&claim, PreparationFailure::Unavailable(reason.clone()));
                let reason = reason.trim();
                let stop = if reason.ends_with(['.', '!', '?', '…']) {
                    ""
                } else {
                    "."
                };
                self.message = Some(format!(
                    "AI pictures unavailable: {reason}{stop} Retry or Discard in Jobs."
                ));
                return true;
            }
        };
        let provider = match &worker {
            Worker::Real(runtime) => runtime.provider(0),
            #[cfg(any(test, feature = "ui-harness"))]
            Worker::Scripted { .. } => deadpan_cli::generation::development_provider(0),
        };
        self.generation.automatic = self.generation.automatic.wrapping_add(1).max(1);
        let prepared = &claim.preparation;
        let mut options = prepared.origin.options().cloned().unwrap_or_default();
        options.resolve_target(None);
        let job = Job {
            ticket: AUTOMATIC_TICKETS + self.generation.automatic,
            session,
            hold: prepared.target.node.clone(),
            target: prepared.target.clone(),
            revision: prepared.current_revision.clone(),
            options: options.clone(),
            controls_pending: prepared.origin.options().is_none(),
            started: Instant::now(),
            request: None,
            plan: None,
            variants: 1,
            variant: 1,
            ready: 0,
            phase: Phase::Conditioning,
            outcome: None,
            note: Some("AI pictures for this pause. Acceptance remains your choice.".into()),
            selected_before: None,
            unprotected: None,
        };
        let input = JobInput {
            package,
            revision: prepared.current_revision.clone(),
            target: prepared.target.clone(),
            options,
            preparation: Some(prepared.clone()),
        };
        if let Err(error) = self.launch_generation(
            WorkerSelection {
                worker,
                seed: None,
                provider,
            },
            input,
            job,
            format!("{label} · AI pictures"),
            Some(claim.clone()),
        ) {
            self.finish_preparation(&claim, PreparationFailure::Interrupted(error.clone()));
            self.message = Some(error);
        }
        true
    }

    /// Cancellation/supersession invalidates preparation before its late
    /// boundary reply can allocate. Fulfilled requests use normal relevance.
    pub(super) fn check_preparation_claim(&mut self) -> bool {
        let Some(running) = &mut self.generation.running else {
            return false;
        };
        if running.started || running.cancelled.load(Ordering::Acquire) {
            return false;
        }
        let Some(claim) = &running.preparation else {
            return false;
        };
        if running.preparation_checked_epoch == self.generation.epoch
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.document.revision_id() == &claim.preparation.current_revision
            })
        {
            return false;
        }
        running.preparation_checked_epoch = self.generation.epoch;
        match self
            .store
            .as_ref()
            .map(|store| store.generation_preparation_claim_is_current(claim))
        {
            Some(Ok(true)) => false,
            result => {
                running.cancelled.store(true, Ordering::Release);
                if let Some(job) = &mut self.generation.job {
                    job.phase = Phase::Cancelling;
                    job.note = Some(match result {
                        Some(Err(error)) => format!("Could not verify the AI preparation: {error}"),
                        _ => "The edit changed or this preparation was discarded; its late preparation will not create a request.".into(),
                    });
                }
                true
            }
        }
    }

    pub(super) fn finish_preparation(
        &mut self,
        claim: &PreparationClaim,
        failure: PreparationFailure,
    ) {
        let bound = |mut text: String| {
            if text.len() > 2048 {
                let mut end = 2048;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
            }
            if text.is_empty() {
                text.push_str("AI preparation stopped.");
            }
            text
        };
        let failure = match failure {
            PreparationFailure::Unavailable(reason) => {
                PreparationFailure::Unavailable(bound(reason))
            }
            PreparationFailure::Interrupted(reason) => {
                PreparationFailure::Interrupted(bound(reason))
            }
            PreparationFailure::Cancelled(reason) => PreparationFailure::Cancelled(bound(reason)),
        };
        let result = self.writer().and_then(|store| {
            store
                .finish_generation_preparation(claim, failure)
                .map_err(display)
        });
        match result {
            Ok(_) => self.generation.variants_changed(),
            Err(error) => self
                .preparation_warning(format!("Could not record AI preparation recovery: {error}")),
        }
    }

    pub(super) fn finish_preparation_for_outcome(&mut self, outcome: &Outcome) {
        let Some(claim) = self
            .generation
            .running
            .as_ref()
            .filter(|running| !running.started)
            .and_then(|running| running.preparation.clone())
        else {
            return;
        };
        let failure = match outcome {
            Outcome::Cancelled
                if self.shared.stopping.load(Ordering::Acquire)
                    || self.pending_session_change.is_some() =>
            {
                PreparationFailure::Interrupted(
                    "Deadpan closed before AI conditioning finished. Retry in Jobs.".into(),
                )
            }
            Outcome::Cancelled => {
                PreparationFailure::Cancelled("AI preparation was cancelled.".into())
            }
            Outcome::Unavailable(reason) | Outcome::Failed(reason) => {
                PreparationFailure::Unavailable(reason.clone())
            }
            Outcome::Ready(_) => return,
        };
        self.finish_preparation(&claim, failure);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_notices_keep_the_complete_count_without_listing_or_losing_ids() {
        use deadpan_store::generation_preparations::PreparationNotice;
        assert_eq!(capacity_notice(&[]), None);
        let notice = PreparationNotice::QueueCapacity {
            displaced: vec![PreparationId::new("first-displaced").unwrap()],
            total: 301,
        };
        let text = capacity_notice(&[notice]).unwrap();
        assert!(text.contains("301 older pending preparations"));
        assert!(text.contains("Timing edits stay saved"));
        assert!(!text.contains("first-displaced"));
    }

    #[test]
    fn retrying_an_earlier_id_restarts_a_partly_scanned_queue() {
        let mut scan = Scan::default();
        let now = Instant::now();
        let revision = RevisionId::new("duration-edit").unwrap();
        let key = (7, revision.clone(), 20);
        assert_eq!(scan.next_page(key.clone(), now), Some(None));
        let last = PreparationId::new("preparation-256").unwrap();
        scan.received(PAGE_SIZE, Some(last.clone()));
        // Ordinary continuation proceeds to page 2, with no busy scan.
        assert_eq!(scan.next_page(key.clone(), now), None);
        assert_eq!(scan.next_page(key, now + SCAN_INTERVAL), Some(Some(last)));
        scan.received(
            PAGE_SIZE,
            Some(PreparationId::new("preparation-512").unwrap()),
        );
        // Retry has queued preparation-001, before the saved cursor. Its
        // operational epoch invalidates that cursor even at the same revision.
        let retry = (7, revision, 21);
        assert_eq!(
            scan.next_page(retry.clone(), now + SCAN_INTERVAL * 2),
            Some(None)
        );
        scan.received(1, Some(PreparationId::new("preparation-001").unwrap()));
        assert_eq!(scan.next_page(retry, now + SCAN_INTERVAL * 3), None);
    }

    #[test]
    fn claim_failure_retries_a_queued_row_after_a_bounded_delay() {
        let mut scan = Scan::default();
        let now = Instant::now();
        let key = (7, RevisionId::new("duration-edit").unwrap(), 20);
        assert_eq!(scan.next_page(key.clone(), now), Some(None));
        scan.received(1, Some(PreparationId::new("queued-row").unwrap()));
        scan.claim_failed(now);
        assert_eq!(scan.next_page(key.clone(), now + SCAN_INTERVAL), None);
        assert_eq!(
            scan.next_page(key.clone(), now + CLAIM_RETRY_DELAY),
            Some(None)
        );
        // A failure after advancing a full page also returns to the queued
        // row, rather than continuing beyond it forever.
        scan.received(PAGE_SIZE, Some(PreparationId::new("later-row").unwrap()));
        scan.claim_failed(now + CLAIM_RETRY_DELAY);
        assert_eq!(scan.next_page(key, now + CLAIM_RETRY_DELAY * 2), Some(None));
    }

    #[test]
    fn a_different_edit_or_session_restarts_a_completed_scan() {
        let mut scan = Scan::default();
        let now = Instant::now();
        let key = (7, RevisionId::new("before").unwrap(), 1);
        assert_eq!(scan.next_page(key, now), Some(None));
        scan.received(0, None);
        assert_eq!(
            scan.next_page(
                (7, RevisionId::new("after").unwrap(), 1),
                now + SCAN_INTERVAL
            ),
            Some(None)
        );
        scan.received(0, None);
        assert_eq!(
            scan.next_page(
                (8, RevisionId::new("after").unwrap(), 1),
                now + SCAN_INTERVAL * 2
            ),
            Some(None)
        );
    }
}
