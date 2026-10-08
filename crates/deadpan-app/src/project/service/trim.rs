//! Ordered entry-relative Trim intent and one private exact command per admission.

use super::*;
use crate::project::trim::{
    Acknowledged, AdjustmentFeedback, CommitReceipt, CommitUpdate, Event, EventOutcome, MAX_EVENTS,
    Prepared, Proposal, ProposalId, ProposalUpdate, Target,
};
use deadpan_core::{
    AudioTimingId, ProjectFrame, SourceTrimAdjustment, SourceTrimCapture, SourceTrimEditResolution,
    SourceTrimIntent, SourceTrimPolicy, SourceTrimResources, SplitIdentities,
};

struct Ready {
    request: CommandRequest,
    prepared: Arc<Prepared>,
}

pub(super) struct Draft {
    id: ProposalId,
    target: Target,
    base: Arc<Workspace>,
    accepted: SourceTrimIntent,
    ready: Option<Ready>,
}

impl Service {
    fn check_trim_context(&self, id: &ProposalId) -> Result<()> {
        if id.session == 0 || id.draft == 0 || id.change == 0 {
            return Err("Trim proposal identities must be nonzero.".into());
        }
        self.check_context(id.session, &id.base_revision)?;
        if self.pending_session_change.is_some()
            || self
                .workspace
                .as_ref()
                .is_none_or(|workspace| workspace.document.project_id() != &id.project)
        {
            return Err("Trim project session changed; reopen Trim.".into());
        }
        Ok(())
    }

    /// Validate the whole envelope before consuming any event or ready command.
    fn check_trim_prefix(&self, proposal: &Proposal) -> Result<()> {
        let id = proposal.id();
        self.check_trim_context(&id)?;
        if proposal.events.len() > MAX_EVENTS {
            return Err(format!("Trim input batch exceeds {MAX_EVENTS} events."));
        }
        if self.trim_seen.as_ref().is_some_and(|seen| {
            seen.session == id.session
                && (id.draft < seen.draft
                    || (id.draft == seen.draft
                        && (id.change <= seen.change || id.base_revision != seen.base_revision)))
        }) {
            return Err("Trim proposal identity was already used or superseded.".into());
        }
        proposal
            .target
            .validate(self.workspace.as_ref().ok_or("Open a project first.")?)?;
        match &self.trim_draft {
            Some(draft) if draft.id.draft == id.draft => {
                if draft.target != proposal.target
                    || proposal.previous_change != Some(draft.id.change)
                {
                    return Err(
                        "Trim input does not continue its exact acknowledged target and prefix."
                            .into(),
                    );
                }
            }
            _ if proposal.previous_change.is_some()
                || self
                    .trim_seen
                    .as_ref()
                    .is_some_and(|seen| seen.draft == id.draft) =>
            {
                return Err("Trim input has no live acknowledged predecessor; reopen Trim.".into());
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn prepare_trim_command(&mut self, proposal: Proposal) {
        let id = proposal.id();
        if let Err(error) = self.check_trim_prefix(&proposal) {
            self.trim = Some(ProposalUpdate {
                id,
                acknowledgment: None,
                result: Err(error),
            });
            return;
        }
        let mut draft = match self.trim_draft.take() {
            Some(draft) if draft.id.draft == id.draft => draft,
            _ => Draft {
                id: id.clone(),
                target: proposal.target.clone(),
                base: self
                    .workspace
                    .as_ref()
                    .expect("Trim prefix checked")
                    .clone(),
                accepted: SourceTrimIntent::default(),
                ready: None,
            },
        };
        // A valid new prefix immediately revokes prior Apply authority. Even a
        // store/admission failure below acknowledges this exact ordered input.
        draft.id = id.clone();
        draft.ready = None;
        self.trim_seen = Some(id.clone());
        let acknowledgment = replay_events(&mut draft, &proposal.events);
        let result = self.prepare_trim(&draft);
        if let Ok((prepared, request)) = &result
            && prepared.snapshot.is_some()
        {
            draft.ready = Some(Ready {
                request: request.clone(),
                prepared: prepared.clone(),
            });
        }
        self.trim_draft = Some(draft);
        self.trim = Some(ProposalUpdate {
            id,
            acknowledgment: Some(acknowledgment),
            result: result.map(|(prepared, _)| prepared),
        });
    }

    fn prepare_trim(&self, draft: &Draft) -> Result<(Arc<Prepared>, CommandRequest)> {
        let preflight = draft
            .base
            .document
            .source_trim_edit(
                &draft.target.parent,
                &draft.target.node,
                draft.target.right.as_ref(),
                draft.accepted,
            )
            .map_err(display)?;
        let new_revision = revision();
        let resources = resources(&preflight, &new_revision);
        let request = CommandRequest {
            project_id: draft.target.project.clone(),
            expected_revision: draft.target.base_revision.clone(),
            new_revision,
            command: Command::ApplySourceTrim {
                parent: draft.target.parent.clone(),
                node: draft.target.node.clone(),
                right: draft.target.right.clone(),
                intent: draft.accepted,
                resources: resources.clone(),
            },
        };
        let preview = self
            .store
            .as_ref()
            .ok_or("Open a project first.")?
            .preview_source_trim_edit(&request)
            .map_err(display)?;
        let result = preview
            .resolution
            .result_identities(&resources)
            .map_err(display)?;
        let cursor_after = ProjectFrame(
            draft
                .target
                .cursor
                .0
                .min(preview.resolution.geometry.project_duration_after.frames()),
        );
        let snapshot = match preview.edit {
            Some(edit) => {
                let document = Arc::new(edit.forward.apply(&draft.base.document).map_err(display)?);
                Some(Arc::new(
                    deadpan_playback::Snapshot::proposed(
                        &draft.base.playback_snapshot(),
                        document,
                        draft.id.draft,
                        draft.id.change,
                    )
                    .map_err(display)?,
                ))
            }
            None => None,
        };
        Ok((
            Arc::new(Prepared {
                base: draft.base.clone(),
                target: draft.target.clone(),
                accepted: draft.accepted,
                resolution: preview.resolution,
                result,
                cursor_after,
                cursor_clamped: cursor_after != draft.target.cursor,
                snapshot,
            }),
            request,
        ))
    }

    pub(super) fn commit_trim_command(&mut self, id: ProposalId) {
        if self.pending_session_change.is_none()
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == id.session && workspace.document.project_id() == &id.project
            })
            && let Some(saved) = &self.saved_trim
            && saved.id == id
        {
            self.trim_commit = Some(CommitUpdate {
                id,
                result: Ok(saved.committed.clone()),
            });
            return;
        }
        match self.commit_trim(&id) {
            Ok((committed, cursor_clamped)) => {
                self.committed = Some(committed.clone());
                self.saved_trim = Some(CommitReceipt {
                    id: id.clone(),
                    committed: committed.clone(),
                    refresh_error: None,
                });
                self.trim_commit = Some(CommitUpdate {
                    id,
                    result: Ok(committed),
                });
                #[cfg(test)]
                {
                    self.render_preview_refresh_failure = self
                        .shared
                        .trim_commit_refresh_failure
                        .swap(false, Ordering::AcqRel);
                }
                let refresh_error = self.refresh().err();
                self.message = Some(match &refresh_error {
                    Some(error) => format!(
                        "Trim saved, but the preview could not refresh: {error}. Reopen the project to view the saved edit."
                    ),
                    None if cursor_clamped => {
                        "Trim saved. Edit cursor moved to the shorter project's end. Undo with u."
                            .into()
                    }
                    None => "Trim saved. Undo with u.".into(),
                });
                self.saved_trim
                    .as_mut()
                    .expect("successful Trim receipt")
                    .refresh_error = refresh_error;
            }
            Err(error) => {
                self.trim_commit = Some(CommitUpdate {
                    id,
                    result: Err(error),
                })
            }
        }
    }

    fn commit_trim(&mut self, id: &ProposalId) -> Result<(CommittedEdit, bool)> {
        self.check_trim_context(id)?;
        let draft = self
            .trim_draft
            .as_mut()
            .ok_or("Trim proposal is no longer available.")?;
        if draft.id != *id {
            return Err("Trim proposal was superseded; preview the latest accepted values.".into());
        }
        draft
            .target
            .validate(self.workspace.as_ref().ok_or("Open a project first.")?)?;
        if draft.accepted.is_zero() {
            return Err("Source Trim resolves to no change.".into());
        }
        // Exact attempt is consumed before store work. A fresh acknowledged input
        // may retry preparation, but a retransmitted failed commit cannot retry.
        let ready = draft
            .ready
            .take()
            .ok_or("Trim proposal is not admitted for Apply.")?;
        let target = draft.target.clone();
        let outcome = self.writer()?.commit(&ready.request).map_err(display)?;
        self.capture_preparation_notices(&outcome.generation_preparation_notices);
        Ok((
            CommittedEdit {
                scoped: None,
                revision: outcome.revision_id,
                selected_node: Some(ready.prepared.result.target.clone()),
                preserve_cursor: true,
                cursor: Some(ready.prepared.cursor_after),
                scope: target.scope,
                sound: None,
                range_selection: None,
            },
            ready.prepared.cursor_clamped,
        ))
    }

    pub(super) fn abandon_trim(&mut self, id: &ProposalId) {
        if self
            .trim_draft
            .as_ref()
            .is_some_and(|draft| draft.id == *id)
        {
            self.invalidate_trim("Trim proposal was abandoned.");
        }
    }

    pub(super) fn invalidate_trim(&mut self, error: &str) {
        if let Some(draft) = self.trim_draft.take() {
            self.trim = Some(ProposalUpdate {
                id: draft.id,
                acknowledgment: None,
                result: Err(error.into()),
            });
        }
    }

    pub(super) fn reconcile_trim(&mut self) {
        let session = self
            .workspace
            .as_ref()
            .map(|workspace| (workspace.session, workspace.document.project_id().clone()));
        if self.trim_session != session {
            self.trim_session = session;
            self.trim_draft = None;
            self.trim_seen = None;
            self.trim = None;
            self.trim_commit = None;
            self.saved_trim = None;
        } else if let Some(draft) = &self.trim_draft
            && self.check_trim_context(&draft.id).is_err()
        {
            self.invalidate_trim("Project changed; reopen Trim to capture its target.");
        }
    }
}

fn resources(plan: &SourceTrimEditResolution, revision: &RevisionId) -> SourceTrimResources {
    SourceTrimResources {
        target_wrapper: plan.required_target_wrapper.then(node),
        right_wrapper: plan.required_right_wrapper.then(node),
        split: SplitIdentities {
            nodes: (0..plan.required_split_nodes).map(|_| node()).collect(),
        },
        fillers: (0..plan.required_filler_nodes).map(|_| node()).collect(),
        timing: (plan.capture != SourceTrimCapture::None).then(|| AudioTimingId {
            allocation: revision.clone(),
            ordinal: 0,
        }),
    }
}

fn replay_events(draft: &mut Draft, events: &[Event]) -> Acknowledged {
    let mut outcomes = Vec::with_capacity(events.len());
    for &event in events {
        let result = resolve_event(draft, event);
        let (adjustment, error) = match result {
            Ok((intent, adjustment)) => {
                draft.accepted = intent;
                (adjustment, None)
            }
            Err(error) => (None, Some(error)),
        };
        outcomes.push(EventOutcome {
            event,
            accepted: draft.accepted,
            adjustment,
            error,
        });
    }
    Acknowledged {
        accepted: draft.accepted,
        events: outcomes,
    }
}

fn resolve_event(
    draft: &Draft,
    event: Event,
) -> Result<(SourceTrimIntent, Option<AdjustmentFeedback>)> {
    let (intent, feedback) = match event {
        Event::Nudge { control, frames } => {
            let adjustment = draft
                .base
                .document
                .adjust_source_trim_geometry(
                    &draft.target.parent,
                    &draft.target.node,
                    draft.target.right.as_ref(),
                    draft.accepted,
                    control,
                    frames,
                )
                .map_err(display)?;
            (adjustment.geometry.intent, Some(feedback(adjustment)))
        }
        Event::SetAmount { control, frames } => {
            let step = frames
                .checked_sub(draft.accepted.value(control))
                .ok_or("Trim amount difference overflowed; accepted values are unchanged.")?;
            let adjustment = draft
                .base
                .document
                .adjust_source_trim_geometry(
                    &draft.target.parent,
                    &draft.target.node,
                    draft.target.right.as_ref(),
                    draft.accepted,
                    control,
                    step,
                )
                .map_err(display)?;
            (adjustment.geometry.intent, Some(feedback(adjustment)))
        }
        Event::SetPolicy(policy) => (
            SourceTrimIntent {
                policy,
                ..draft.accepted
            },
            None,
        ),
        Event::TogglePolicy => {
            let policy = match draft.accepted.policy {
                SourceTrimPolicy::Ripple => SourceTrimPolicy::Overwrite,
                SourceTrimPolicy::Overwrite => SourceTrimPolicy::Ripple,
            };
            (
                SourceTrimIntent {
                    policy,
                    ..draft.accepted
                },
                None,
            )
        }
    };
    // A structural preflight refusal is not a clamp. Leave the entire accepted
    // tuple intact, including policy, then continue later events from that tuple.
    draft
        .base
        .document
        .source_trim_edit(
            &draft.target.parent,
            &draft.target.node,
            draft.target.right.as_ref(),
            intent,
        )
        .map_err(display)?;
    Ok((intent, feedback))
}

fn feedback(value: SourceTrimAdjustment) -> AdjustmentFeedback {
    AdjustmentFeedback {
        control: value.control,
        previous_value: value.previous_value,
        requested_step: value.requested_step,
        requested_value: value.requested_value,
        applied_value: value.applied_value,
        minimum: value.minimum,
        maximum: value.maximum,
        minimum_value: value.minimum_value,
        maximum_value: value.maximum_value,
        clamp: value.clamp,
    }
}
