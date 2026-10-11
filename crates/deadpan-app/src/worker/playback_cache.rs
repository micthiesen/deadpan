//! During committed playback, keep one Original and one Generated decoder.
//! The active slot remains the only decoder that produces the requested
//! picture. The other slot preserves its verified snapshot and (for an
//! Original) the already running cut/loop look-ahead while its provider is
//! temporarily off screen. No extra slot is retained after a stopped reply.

use super::*;

#[derive(Default)]
pub(super) struct PlaybackCache {
    parked: Option<RetainedSession>,
    generated: generated_ahead::GeneratedAhead,
}

impl PlaybackCache {
    #[cfg(test)]
    pub(super) fn generated_finished(&self) -> bool {
        self.generated.finished()
    }

    pub(super) fn clear(&mut self) {
        self.generated.clear();
        self.parked = None;
    }

    /// Run on the worker, outside the mailbox lock. Keep the current
    /// picture's decoder, but release every playback-only decoder even when
    /// stopping does not request another picture.
    pub(super) fn release(&mut self, active: &mut Option<RetainedSession>) {
        self.clear();
        if let Some(lookahead) = active
            .as_mut()
            .and_then(|session| session.ahead.lookahead.as_mut())
        {
            lookahead.release();
        }
    }

    pub(super) fn select(
        &mut self,
        request: &Request,
        active: &mut Option<RetainedSession>,
        playing: bool,
    ) -> Result<(), String> {
        if !playing {
            self.generated.clear();
        }
        // Ordinary stopped/candidate/copied readers keep the existing one-slot
        // policy. A first stopped request may recover the playback slot it
        // needs before `clear` releases the other one after publication.
        if !playing && self.parked.is_none() {
            return Ok(());
        }
        let Work::Project { workspace, view } = &request.work else {
            self.clear();
            return Ok(());
        };
        let belongs = |session: &RetainedSession| match &session.key {
            SessionKey::Project { session, .. } | SessionKey::Generated { session, .. } => {
                *session == workspace.session
            }
            SessionKey::Raw(_) => false,
        };
        if active.as_ref().is_some_and(|session| !belongs(session)) {
            *active = None;
        }
        if self
            .parked
            .as_ref()
            .is_some_and(|session| !belongs(session))
        {
            self.clear();
        }
        let generated = match view {
            ProjectView::Source { .. } => false,
            ProjectView::Sequence { frame } => match workspace.plan.picture(*frame) {
                Ok(sample) => match sample.picture {
                    deadpan_plan::Picture::Accepted {
                        generated: Some(_), ..
                    } => true,
                    deadpan_plan::Picture::Source { .. } | deadpan_plan::Picture::Freeze { .. } => {
                        false
                    }
                    // Background preserves both slots; unsupported/invalid
                    // pictures still fail through the ordinary reader.
                    _ => return Ok(()),
                },
                Err(_) => return Ok(()),
            },
        };
        let is_generated =
            |session: &RetainedSession| matches!(session.key, SessionKey::Generated { .. });
        let change = active
            .as_ref()
            .is_some_and(|session| is_generated(session) != generated)
            || (active.is_none()
                && self
                    .parked
                    .as_ref()
                    .is_some_and(|session| is_generated(session) == generated));
        if change {
            std::mem::swap(active, &mut self.parked);
        }
        if generated && playing {
            let ProjectView::Sequence { frame } = view else {
                unreachable!()
            };
            let sample = workspace
                .plan
                .picture(*frame)
                .map_err(|error| error.to_string())?;
            let deadpan_plan::Picture::Accepted {
                generated: Some(artifact),
                ..
            } = sample.picture
            else {
                unreachable!()
            };
            let key = SessionKey::Generated {
                session: workspace.session,
                media: Arc::new(GeneratedMediaKey::new(&workspace.document, &artifact)?),
            };
            if active.as_ref().is_none_or(|session| session.key != key) {
                // A future admission owns the sole Generated slot until taken.
                // Drop a different retained artifact before admitting this one.
                *active = None;
                *active = self.generated.take_for(&key, &request.cancelled)?;
            }
        }
        // This chooses a provider role only. The ordinary reader still checks
        // the complete key, media authority and liveness before using a slot.
        Ok(())
    }

    pub(super) fn prepare(
        &mut self,
        shared: &Shared,
        request: &Request,
        active: &Option<RetainedSession>,
    ) {
        // Never have two Generated decoders. Its next admission can begin
        // only while Original is serving and its previous Generated is idle.
        if !active
            .as_ref()
            .is_some_and(|session| matches!(session.key, SessionKey::Project { .. }))
        {
            return;
        }
        let Some((workspace, frame, key)) = self.generated.target(request) else {
            self.generated.stop_job();
            return;
        };
        if self
            .parked
            .as_ref()
            .is_some_and(|session| session.key == key)
        {
            self.generated.stop_job();
            return;
        }
        self.parked = None;
        self.generated.start(workspace, frame, key);
        let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
        mailbox.generated_ahead = self.generated.stop_flag();
        if mailbox.shutdown
            || mailbox.clear_requested
            || mailbox
                .pending
                .as_ref()
                .is_some_and(|pending| !matches!(pending.work, Work::Project { .. }))
            || mailbox.latest.is_none_or(|latest| {
                latest.transport != request.ticket.transport
                    || latest.source != request.ticket.source
            })
        {
            mailbox.stop_generated_ahead();
        }
    }

    pub(super) fn original<'a>(
        &'a mut self,
        active: &'a mut Option<RetainedSession>,
    ) -> &'a mut Option<RetainedSession> {
        if active
            .as_ref()
            .is_some_and(|session| matches!(session.key, SessionKey::Project { .. }))
        {
            active
        } else {
            &mut self.parked
        }
    }
}
