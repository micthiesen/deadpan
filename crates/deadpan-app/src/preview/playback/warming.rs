use super::*;
use deadpan_playback::{WarmRequestError, WarmStatus, WarmTicket, WarmUpdate};

const SETTLE_SECONDS: f64 = 0.4;

pub(in crate::preview) struct Warming {
    workspace: Arc<Workspace>,
    domain: Domain,
    start: AudioSample,
    since: f64,
    ticket: Option<WarmTicket>,
    update: Option<WarmUpdate>,
}

impl DeadpanApp {
    /// Called after final input/layout decisions. Preparing audio cannot take
    /// priority over current picture work, a modal draft, playback or a render.
    pub(in crate::preview) fn reconcile_audio_warming(&mut self, context: &egui::Context) {
        if self.transport.is_some() {
            // Engine::play transfers matching in-flight work. Do not cancel it.
            self.audio_warming = None;
            return;
        }
        #[cfg(feature = "ui-harness")]
        if self.feedback.suppress_audio_warming {
            return;
        }
        let eligible = !self.close_pending
            && !self.dialogs.is_open()
            && !self.service.is_busy()
            && !self.render.blocking()
            && !self.presentation.loading()
            && !self.presentation.needs_render()
            && self.camera.is_none()
            && self.camera_pending.is_none()
            && self.gain.is_none()
            && self.room_tone.is_none()
            && self.splice.is_none()
            && self.slip.is_none()
            && self.trim.is_none();
        let candidate = eligible
            .then(|| {
                let workspace = Arc::clone(self.workspace.as_ref()?);
                // A proposed AI provider has a separate admission and lifecycle.
                if self.ai_preview_audio(&workspace).is_some() {
                    return None;
                }
                let domain = self.playback_domain().ok()?;
                let end = domain.end().ok()?;
                if end.0 <= 0 {
                    return None;
                }
                let cursor = if domain.is_sound() {
                    self.sound_cursor
                } else if self.view == View::Source {
                    self.source_cursor
                } else {
                    self.sequence_cursor
                };
                let start = domain.sample_at_boundary(cursor).ok()?;
                // Match ordinary Play at the endpoint, which restarts at zero.
                let start = if start >= end { AudioSample(0) } else { start };
                Some((workspace, domain, start))
            })
            .flatten();
        let Some((workspace, domain, start)) = candidate else {
            if self.audio_warming.take().is_some() {
                self.playback.cancel_warming();
            }
            return;
        };
        let now = context.input(|input| input.time);
        if self.audio_warming.as_ref().is_none_or(|old| {
            !Arc::ptr_eq(&old.workspace, &workspace) || old.domain != domain || old.start != start
        }) {
            self.playback.cancel_warming();
            self.audio_warming = Some(Warming {
                workspace,
                domain,
                start,
                since: now,
                ticket: None,
                update: None,
            });
        }
        let warming = self
            .audio_warming
            .as_mut()
            .expect("captured stopped cursor");
        if now - warming.since < SETTLE_SECONDS {
            context.request_repaint_after(Duration::from_secs_f64(
                SETTLE_SECONDS - (now - warming.since).max(0.0),
            ));
            return;
        }
        if warming.ticket.is_none() {
            let target = match &warming.domain {
                Domain::Sequence { .. } => Target::Sequence,
                Domain::Original(original) => Target::Original(Arc::clone(original)),
                Domain::Sound(sound) => Target::Sound(Arc::clone(sound)),
                Domain::AudioRange(_) => return,
            };
            match self.playback.warm_audio(
                Arc::new(warming.workspace.playback_snapshot()),
                target,
                warming.start,
            ) {
                Ok(ticket) => warming.ticket = Some(ticket),
                Err(WarmRequestError::PlaybackActive) => {
                    context.request_repaint_after(Duration::from_millis(100));
                    return;
                }
                Err(_) => return,
            }
        }
        let update = self
            .playback
            .audio_warming(warming.ticket.expect("issued idle request"));
        let changed = warming
            .update
            .as_ref()
            .map(|u| (u.status, u.prepared_until, u.activity, u.elapsed_millis))
            != update
                .as_ref()
                .map(|u| (u.status, u.prepared_until, u.activity, u.elapsed_millis));
        warming.update = update;
        if changed {
            context.request_repaint();
        }
        if warming
            .update
            .as_ref()
            .is_some_and(|u| matches!(u.status, WarmStatus::Queued | WarmStatus::Preparing))
        {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(in crate::preview) fn idle_audio_label(&self) -> Option<String> {
        if self.transport.is_some() {
            return None;
        }
        let warming = self.audio_warming.as_ref()?;
        if !Arc::ptr_eq(&warming.workspace, self.workspace.as_ref()?)
            || self.playback_domain().ok()? != warming.domain
        {
            return None;
        }
        let cursor = if warming.domain.is_sound() {
            self.sound_cursor
        } else if self.view == View::Source {
            self.source_cursor
        } else {
            self.sequence_cursor
        };
        let start = warming.domain.sample_at_boundary(cursor).ok()?;
        let start = if start >= warming.domain.end().ok()? {
            AudioSample(0)
        } else {
            start
        };
        if start != warming.start {
            return None;
        }
        let update = warming.update.as_ref()?;
        Some(match update.status {
            WarmStatus::Queued => "Audio cache queued".into(),
            WarmStatus::Preparing => format!(
                "Audio ahead · {}",
                Run::preparation_status_label(deadpan_playback::Preparation {
                    activity: update.activity,
                    elapsed_millis: update.elapsed_millis,
                })
            ),
            WarmStatus::Complete => "Audio cache checked ahead".into(),
            WarmStatus::Interrupted => "Audio cache deferred".into(),
            WarmStatus::Unavailable => "Audio cache unavailable".into(),
        })
    }

    pub(in crate::preview) fn idle_audio_detail(&self) -> String {
        self.audio_warming.as_ref().and_then(|warming| warming.update.as_ref())
            .and_then(|update| update.error.clone()).unwrap_or_else(||
                "Prepares up to 30 seconds ahead while stopped. Play rechecks media and reuses retained PCM; cache limits still apply.".into())
    }
}
