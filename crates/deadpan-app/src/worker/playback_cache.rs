//! During committed playback, retain one Original and one Generated decoder,
//! plus one upcoming Generated admission while Generated is serving.
//! The active slot remains the only decoder that produces the requested
//! picture. The other slot preserves its verified snapshot and (for an
//! Original) the already running cut/loop look-ahead while its provider is
//! temporarily off screen. No extra slot is retained after a stopped reply.

use super::*;
use provider_ahead::{Provider, ProviderAhead};

#[derive(Default)]
pub(super) struct PlaybackCache {
    parked: Option<RetainedSession>,
    generated: ProviderAhead,
    original: ProviderAhead,
}

impl PlaybackCache {
    #[cfg(test)]
    pub(super) fn generated_finished(&self) -> bool {
        self.generated.finished()
    }

    #[cfg(test)]
    pub(super) fn original_finished(&self) -> bool {
        self.original.finished()
    }

    pub(super) fn clear(&mut self) {
        self.generated.clear();
        self.original.clear();
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
            self.original.clear();
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
        let Some(key) = provider_ahead::key(workspace, view)? else {
            // Background preserves both slots; unsupported pictures still
            // fail through the ordinary reader.
            return Ok(());
        };
        let generated = matches!(key, SessionKey::Generated { .. });
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
        if playing && active.as_ref().is_none_or(|session| session.key != key) {
            // Drop the old provider before adopting its prepared successor.
            *active = None;
            let prepared = if generated {
                &mut self.generated
            } else {
                &mut self.original
            };
            *active = prepared.take_for(&key, &request.cancelled)?;
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
        let serving = match active.as_ref().map(|session| &session.key) {
            Some(SessionKey::Generated { media, .. }) => Some(Arc::clone(&media.artifact)),
            Some(SessionKey::Project { .. }) => None,
            _ => return,
        };
        // Starting playback on Generated has no parked Original. Admit only
        // that missing slot; an existing Original owns its own return look-ahead.
        if serving.is_some() && self.parked.is_none() {
            if let Some((workspace, frame, key)) = self.original.target(request, Provider::Original)
            {
                self.original.start(workspace, frame, key);
            } else {
                self.original.stop_job();
            }
        } else {
            self.original.stop_job();
        }
        match self
            .generated
            .target(request, Provider::Generated { serving })
        {
            Some((workspace, frame, key))
                if self
                    .parked
                    .as_ref()
                    .is_none_or(|session| session.key != key) =>
            {
                // Keep Original and its return look-ahead while Generated is
                // serving. An idle Generated can be replaced by its successor.
                if self
                    .parked
                    .as_ref()
                    .is_some_and(|session| matches!(session.key, SessionKey::Generated { .. }))
                {
                    self.parked = None;
                }
                self.generated.start(workspace, frame, key);
            }
            _ => self.generated.stop_job(),
        }
        let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
        mailbox.generated_ahead = self.generated.stop_flag();
        mailbox.original_ahead = self.original.stop_flag();
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
            mailbox.stop_original_ahead();
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
