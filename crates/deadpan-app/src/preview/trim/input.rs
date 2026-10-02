//! Ordered UI input waits for exact service acknowledgments before another batch.

use std::{collections::VecDeque, sync::Arc};

use deadpan_core::SourceTrimIntent;

use crate::project::trim::{
    Event, EventOutcome, MAX_EVENTS, Prepared, Proposal, ProposalId, ProposalUpdate, Target,
};

const MAX_WAITING: usize = 64;

pub(super) struct Input {
    pub(super) target: Target,
    draft: u64,
    change: u64,
    acknowledged: Option<u64>,
    pub(super) accepted: SourceTrimIntent,
    queued: VecDeque<Event>,
    pending: Option<Proposal>,
    prepared: Option<Arc<Prepared>>,
    issued: Option<ProposalId>,
    retry: bool,
    pub(super) invalidated: bool,
    pub(super) error: Option<String>,
    pub(super) input_error: Option<String>,
    pub(super) text_error: Option<String>,
    pub(super) feedback: Vec<EventOutcome>,
}

impl Input {
    pub(super) fn new(target: Target, draft: u64) -> Self {
        Self {
            target,
            draft,
            change: 0,
            acknowledged: None,
            accepted: SourceTrimIntent::default(),
            queued: VecDeque::new(),
            pending: None,
            prepared: None,
            issued: None,
            retry: true,
            invalidated: false,
            error: None,
            input_error: None,
            text_error: None,
            feedback: Vec::new(),
        }
    }

    pub(super) fn waiting(&self) -> usize {
        self.queued.len()
            + self
                .pending
                .as_ref()
                .map_or(0, |request| request.events.len())
    }

    pub(super) fn queue(&mut self, event: Event) -> bool {
        if self.invalidated {
            return false;
        }
        if self.waiting() >= MAX_WAITING {
            self.input_error = Some("Trim input queue is full. This key was not queued; wait and enter the amount again.".into());
            return false;
        }
        self.queued.push_back(event);
        self.prepared = None;
        self.error = None;
        self.input_error = None;
        true
    }

    pub(super) fn set_text_error(&mut self, error: String) {
        self.text_error = Some(error);
        self.prepared = None;
    }

    /// Only a successfully parsed amount may clear invalid native field text.
    /// Unrelated keyboard input must not make that invalid text applyable.
    pub(super) fn accept_text(&mut self, event: Event) -> bool {
        if self.queue(event) {
            self.text_error = None;
            true
        } else {
            false
        }
    }

    pub(super) fn retry(&mut self) {
        if !self.invalidated && self.text_error.is_none() {
            self.retry = true;
            self.prepared = None;
            self.error = None;
        }
    }

    /// Call only when the service can accept work. A failed submit must return
    /// the envelope through submission_failed; no accepted input is discarded.
    pub(super) fn request(&mut self) -> Option<Proposal> {
        if self.invalidated || self.pending.is_some() || (!self.retry && self.queued.is_empty()) {
            return None;
        }
        let Some(change) = self.change.checked_add(1) else {
            self.invalidate("Trim input counter exhausted. Cancel and reopen Trim.");
            return None;
        };
        self.change = change;
        let request = Proposal {
            target: self.target.clone(),
            draft: self.draft,
            change,
            previous_change: self.acknowledged,
            events: self
                .queued
                .drain(..self.queued.len().min(MAX_EVENTS))
                .collect(),
        };
        self.retry = false;
        self.issued = Some(request.id());
        self.pending = Some(request.clone());
        self.prepared = None;
        Some(request)
    }

    pub(super) fn submission_failed(&mut self, id: &ProposalId, error: String) {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| &pending.id() != id)
        {
            return;
        }
        let pending = self.pending.take().expect("matching pending envelope");
        for event in pending.events.into_iter().rev() {
            self.queued.push_front(event);
        }
        self.retry = true;
        self.error = Some(error);
    }

    pub(super) fn receive(&mut self, update: ProposalUpdate) -> bool {
        if self.invalidated
            || self
                .pending
                .as_ref()
                .is_none_or(|pending| pending.id() != update.id)
        {
            return false;
        }
        let pending = self.pending.take().expect("matching pending envelope");
        let Some(ack) = update.acknowledgment else {
            self.invalidate(&update.result.err().unwrap_or_else(|| {
                "Trim rejected its input envelope. Cancel and reopen Trim.".into()
            }));
            return true;
        };
        if ack.events.len() != pending.events.len()
            || ack
                .events
                .iter()
                .zip(&pending.events)
                .any(|(outcome, event)| &outcome.event != event)
            || ack
                .events
                .last()
                .map_or(self.accepted, |outcome| outcome.accepted)
                != ack.accepted
        {
            self.invalidate(
                "Trim acknowledged a different input sequence. Cancel and reopen Trim.",
            );
            return true;
        }
        self.acknowledged = Some(update.id.change);
        self.accepted = ack.accepted;
        self.feedback = ack.events;
        self.prepared = None;
        self.error = None;
        match update.result {
            Ok(prepared) => {
                if let Err(error) = validate_prepared(&pending, self.accepted, &prepared) {
                    self.invalidate(&error);
                } else if self.queued.is_empty() && self.text_error.is_none() {
                    self.prepared = Some(prepared);
                }
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    pub(super) fn ready(&self) -> Option<&Arc<Prepared>> {
        if self.invalidated
            || self.pending.is_some()
            || !self.queued.is_empty()
            || self.retry
            || self.error.is_some()
            || self.input_error.is_some()
            || self.text_error.is_some()
        {
            None
        } else {
            self.prepared.as_ref()
        }
    }

    pub(super) fn ready_id(&self) -> Option<&ProposalId> {
        self.ready().and(self.issued.as_ref())
    }

    /// A rejected or failed newer envelope may leave the acknowledged prefix
    /// in the service. Abandon both possible owners; exact service IDs make
    /// these bounded cleanup requests harmless when one no longer exists.
    pub(super) fn abandon_ids(&self) -> Vec<ProposalId> {
        let mut ids = self.issued.iter().cloned().collect::<Vec<_>>();
        if let Some(change) = self.acknowledged {
            let acknowledged = ProposalId {
                session: self.target.session,
                project: self.target.project.clone(),
                base_revision: self.target.base_revision.clone(),
                draft: self.draft,
                change,
            };
            if !ids.contains(&acknowledged) {
                ids.push(acknowledged);
            }
        }
        ids
    }

    pub(super) fn invalidate(&mut self, error: &str) {
        self.invalidated = true;
        self.pending = None;
        self.prepared = None;
        self.queued.clear();
        self.retry = false;
        self.error = Some(error.into());
    }
}

fn validate_prepared(
    request: &Proposal,
    accepted: SourceTrimIntent,
    prepared: &Prepared,
) -> Result<(), String> {
    request.target.validate(&prepared.base)?;
    if prepared.target != request.target
        || prepared.accepted != accepted
        || prepared.resolution.geometry.intent != accepted
        || accepted.is_zero() != prepared.snapshot.is_none()
    {
        return Err(
            "Trim returned a different target or accepted values. Cancel and reopen Trim.".into(),
        );
    }
    if let Some(snapshot) = &prepared.snapshot {
        let expected = deadpan_playback::ContentIdentity::Proposed {
            base_revision: request.target.base_revision.clone(),
            draft: request.draft,
            change: request.change,
        };
        if snapshot.session != request.target.session
            || snapshot.document.project_id() != &request.target.project
            || snapshot.content != expected
        {
            return Err("Trim returned a different proposed edit. Cancel and reopen Trim.".into());
        }
        snapshot
            .validate_proposed_base(prepared.base.session, &prepared.base.document)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{SequenceScope, trim::Acknowledged};
    use deadpan_core::{
        FrameRange, NodeId, ProjectFrame, ProjectId, RevisionId, SourceTrimControl,
    };

    fn input() -> Input {
        Input::new(
            Target {
                session: 1,
                project: ProjectId::new("project").unwrap(),
                base_revision: RevisionId::new("entry").unwrap(),
                scope: SequenceScope::default(),
                parent: NodeId::new("root").unwrap(),
                node: NodeId::new("a").unwrap(),
                right: None,
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(10)).unwrap(),
                cursor: ProjectFrame(3),
            },
            7,
        )
    }

    fn nudge(frames: i64) -> Event {
        Event::Nudge {
            control: SourceTrimControl::In,
            frames,
        }
    }

    fn admitted_input(request: &Proposal, accepted: SourceTrimIntent) -> ProposalUpdate {
        ProposalUpdate {
            id: request.id(),
            acknowledgment: Some(Acknowledged {
                accepted,
                events: request
                    .events
                    .iter()
                    .map(|event| EventOutcome {
                        event: *event,
                        accepted,
                        adjustment: None,
                        error: None,
                    })
                    .collect(),
            }),
            // Candidate failure is independent of whether the input was consumed.
            result: Err("fixture receipt unavailable".into()),
        }
    }

    #[test]
    fn pending_prefix_is_acknowledged_before_later_reverse_and_toggle_events() {
        let mut input = input();
        for _ in 0..MAX_EVENTS {
            assert!(input.queue(nudge(10)));
        }
        let first = input.request().unwrap();
        assert_eq!(first.previous_change, None);
        assert_eq!(first.events, vec![nudge(10); MAX_EVENTS]);
        assert!(input.queue(nudge(-1)));
        assert!(input.queue(Event::TogglePolicy));
        assert!(input.queue(Event::TogglePolicy));
        assert!(input.request().is_none());
        let accepted = SourceTrimIntent {
            in_frames: 10,
            ..Default::default()
        };
        assert!(input.receive(admitted_input(&first, accepted)));
        assert_eq!(input.accepted, accepted);
        assert!(input.ready().is_none());
        let next = input.request().unwrap();
        assert_eq!(next.previous_change, Some(first.change));
        assert_eq!(
            next.events,
            vec![nudge(-1), Event::TogglePolicy, Event::TogglePolicy]
        );
    }

    #[test]
    fn failed_submission_restores_exact_prefix_before_new_input_without_reusing_change() {
        let mut input = input();
        assert!(input.queue(nudge(10)));
        let first = input.request().unwrap();
        assert!(input.queue(nudge(-1)));
        input.submission_failed(&first.id(), "busy".into());
        assert_eq!(input.waiting(), 2);
        let next = input.request().unwrap();
        assert!(next.change > first.change);
        assert_eq!(next.previous_change, None);
        assert_eq!(next.events, vec![nudge(10), nudge(-1)]);
    }

    #[test]
    fn stale_reply_does_not_consume_pending_input_and_wrong_ack_requires_reentry() {
        let mut input = input();
        assert!(input.queue(nudge(1)));
        let request = input.request().unwrap();
        let mut wrong = admitted_input(&request, SourceTrimIntent::default());
        wrong.id.change += 1;
        assert!(!input.receive(wrong));
        assert_eq!(input.waiting(), 1);
        let mut wrong = admitted_input(&request, SourceTrimIntent::default());
        wrong.acknowledgment.as_mut().unwrap().events[0].event = nudge(-1);
        assert!(input.receive(wrong));
        assert!(input.invalidated);
        assert!(input.request().is_none());
        assert!(input.ready().is_none());
    }

    #[test]
    fn waiting_bound_counts_inflight_input_and_text_failure_survives_acknowledgment() {
        let mut input = input();
        for _ in 0..MAX_EVENTS {
            assert!(input.queue(nudge(1)));
        }
        let first = input.request().unwrap();
        for _ in MAX_EVENTS..MAX_WAITING {
            assert!(input.queue(nudge(1)));
        }
        assert!(!input.queue(nudge(1)));
        assert_eq!(input.waiting(), MAX_WAITING);
        assert!(input.input_error.is_some());
        assert!(input.receive(admitted_input(&first, SourceTrimIntent::default())));
        assert_eq!(input.waiting(), MAX_EVENTS);
        assert!(input.input_error.is_some());
        assert!(input.ready().is_none());
        assert!(input.queue(nudge(-1)));
        assert!(input.input_error.is_none());
        input.set_text_error("invalid frame text".into());
        assert!(input.queue(Event::TogglePolicy));
        assert!(input.text_error.is_some());
        input.retry();
        assert!(input.ready().is_none());
        assert!(input.accept_text(Event::SetAmount {
            control: SourceTrimControl::In,
            frames: 2,
        }));
        assert!(input.text_error.is_none());
    }

    #[test]
    fn exhausted_counter_cannot_wrap_to_an_old_proposal() {
        let mut input = input();
        input.change = u64::MAX;
        assert!(input.request().is_none());
        assert!(input.invalidated);
        assert!(input.error.as_ref().unwrap().contains("counter exhausted"));
    }

    #[test]
    fn cancellation_covers_acknowledged_owner_after_failed_or_rejected_newer_envelope() {
        let mut input = input();
        let first = input.request().unwrap();
        assert!(input.receive(admitted_input(&first, SourceTrimIntent::default())));
        input.retry();
        let second = input.request().unwrap();
        input.submission_failed(&second.id(), "busy".into());
        assert_eq!(input.abandon_ids(), vec![second.id(), first.id()]);
        let third = input.request().unwrap();
        assert!(input.receive(ProposalUpdate {
            id: third.id(),
            acknowledgment: None,
            result: Err("envelope refused".into()),
        }));
        assert!(input.invalidated);
        assert_eq!(input.abandon_ids(), vec![third.id(), first.id()]);
    }
}
