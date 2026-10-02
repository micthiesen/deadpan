//! Admitted root-output windows share the existing idle analysis lane.

use super::*;
use deadpan_audio::{EditWaveform, WaveformLimits};
use deadpan_core::AudioSample;
use std::ops::Range;

pub struct EditWaveformRequest {
    /// Current immutable committed base, including for a Before request.
    pub base: Arc<Snapshot>,
    /// Either that committed document or a genuine proposal from that base.
    pub snapshot: Arc<Snapshot>,
    pub samples: Range<AudioSample>,
    pub limits: WaveformLimits,
}

#[derive(Debug, Clone)]
pub struct EditWaveformUpdate {
    pub ticket: WaveformTicket,
    pub session: u64,
    pub project_id: ProjectId,
    pub base_revision: RevisionId,
    pub revision_id: RevisionId,
    pub content: ContentIdentity,
    pub samples: Range<AudioSample>,
    pub status: WaveformStatus,
    pub waveform: Option<Arc<EditWaveform>>,
    pub examined_samples: u64,
    pub error: Option<String>,
}

impl EditWaveformUpdate {
    pub(super) fn retain_live_media(&mut self, snapshot: &Snapshot) {
        if self.waveform.is_none() {
            return;
        }
        // Existing opaque-view checks read two revocation flags only. Do not
        // re-open/revalidate Original files or change warm committed semantics.
        // Cancellation remains the shared worker's separate lifecycle decision.
        if let Err(error) = snapshot.check_media_live(&AtomicBool::new(false)) {
            self.status = WaveformStatus::Unavailable;
            self.waveform = None;
            self.examined_samples = 0;
            self.error = Some(format!("Edit waveform media admission expired: {error}"));
        }
    }
}

#[cfg(test)]
impl State {
    /// Observe the publication boundary without invoking poll's later liveness
    /// check. This keeps worker and consumer admission regressions independent.
    pub(crate) fn edit_reply_for_test(&self) -> Option<EditWaveformUpdate> {
        match self.reply.as_ref()? {
            AnalysisUpdate::Edit(update) => Some(update.clone()),
            AnalysisUpdate::Definition(_) => None,
        }
    }
}

impl EditWaveformRequest {
    pub(crate) fn validate(&self) -> Result<(), WaveformRequestError> {
        self.base.validate_admission().map_err(invalid_admission)?;
        self.snapshot
            .validate_admission()
            .map_err(invalid_admission)?;
        if self.base.content != ContentIdentity::Committed {
            return Err(WaveformRequestError::InvalidAdmission(
                "Edit waveform requires a committed base".into(),
            ));
        }
        if !self.snapshot.originals.same_session(&self.base.originals) {
            return Err(WaveformRequestError::InvalidAdmission(
                "Edit waveform Original session differs from its captured base".into(),
            ));
        }
        match &self.snapshot.content {
            ContentIdentity::Committed => {
                if self.snapshot.session != self.base.session
                    || !Arc::ptr_eq(&self.snapshot.document, &self.base.document)
                    || !Arc::ptr_eq(&self.snapshot.sources, &self.base.sources)
                {
                    return Err(WaveformRequestError::InvalidAdmission(
                        "Edit waveform committed input differs from its captured base".into(),
                    ));
                }
            }
            ContentIdentity::Proposed { .. } => self
                .snapshot
                .validate_proposed_base(self.base.session, &self.base.document)
                .map_err(invalid_admission)?,
        }
        if self.samples.start.0 < 0 || self.samples.end < self.samples.start {
            return Err(WaveformRequestError::InvalidRange);
        }
        Ok(())
    }
}

fn invalid_admission(error: crate::SnapshotError) -> WaveformRequestError {
    WaveformRequestError::InvalidAdmission(error.to_string())
}

impl Job {
    pub(crate) fn edit_update(&self, status: WaveformStatus) -> EditWaveformUpdate {
        let Target::Edit { base, samples, .. } = &self.target else {
            unreachable!("Edit waveform job")
        };
        EditWaveformUpdate {
            ticket: self.ticket,
            session: self.snapshot.session,
            project_id: self.snapshot.document.project_id().clone(),
            base_revision: base.document.revision_id().clone(),
            revision_id: self.snapshot.document.revision_id().clone(),
            content: self.snapshot.content.clone(),
            samples: samples.clone(),
            status,
            waveform: None,
            examined_samples: 0,
            error: None,
        }
    }
}

impl Engine {
    /// Latest-only analysis of an exact absolute root sample interval. No output
    /// device opens; active playback takes priority and can interrupt this job.
    pub fn request_edit_waveform(
        &self,
        request: EditWaveformRequest,
    ) -> Result<WaveformTicket, WaveformRequestError> {
        request.validate()?;
        let mut state = self.shared.lock();
        if state.shutdown() {
            return Err(WaveformRequestError::Shutdown);
        }
        let ticket = state.waveform.request_target(
            request.snapshot,
            Target::Edit {
                base: request.base,
                samples: request.samples,
                limits: request.limits,
            },
        )?;
        drop(state);
        self.shared.wake.notify_all();
        self.shared.repaint();
        Ok(ticket)
    }

    /// A definition consumer cannot accidentally consume an Edit reply, or vice
    /// versa. Both kinds still share one superseding request and one reply slot.
    pub fn poll_edit_waveform(&self) -> Option<EditWaveformUpdate> {
        let mut state = self.shared.lock();
        if !matches!(state.waveform.reply, Some(AnalysisUpdate::Edit(_))) {
            return None;
        }
        let snapshot = state.waveform.current.as_ref()?.snapshot.clone();
        let mut update = match state.waveform.reply.take() {
            Some(AnalysisUpdate::Edit(update)) => update,
            _ => unreachable!("checked reply kind"),
        };
        drop(state);
        // A successful result may wait in the mailbox while the admitting store
        // closes. Keep its identity, but never deliver revoked peaks as usable.
        update.retain_live_media(&snapshot);
        Some(update)
    }
}
