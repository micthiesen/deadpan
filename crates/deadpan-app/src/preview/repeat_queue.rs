//! Bounded semantic continuations for explicit Repeat wraps. Never stores a
//! stale ProjectRequest or relaxes the project writer's revision checks.

use std::collections::VecDeque;

use deadpan_core::{NodeId, NodeKind, ProjectDocument, ProjectFrame, RevisionId};

use crate::navigation::Pane;
use crate::project::{CommittedEdit, ProjectEdit, ProjectRequest, SequenceScope};

pub(super) const MAX_WAITING: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub session: u64,
    pub revision: RevisionId,
    pub scope: SequenceScope,
    pub node: NodeId,
    pub cursor: ProjectFrame,
    pub pane: Pane,
}

impl Target {
    pub fn request(&self, plays: u32) -> ProjectRequest {
        ProjectRequest::Edit {
            expected_session: self.session,
            expected_revision: self.revision.clone(),
            scope: self.scope.clone(),
            cursor: self.cursor,
            edit: ProjectEdit::WrapRepeat {
                node: self.node.clone(),
                plays,
            },
        }
    }
}

enum Chain {
    Saving { target: Target, plays: u32 },
    Ready(Target),
}

#[derive(Default)]
pub(super) struct Queue {
    chain: Option<Chain>,
    waiting: VecDeque<u32>,
    notice: Option<String>,
    rejected: usize,
}

impl Queue {
    pub fn active(&self) -> bool {
        self.chain.is_some()
    }
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    fn target(&self) -> Option<&Target> {
        match &self.chain {
            Some(Chain::Saving { target, .. } | Chain::Ready(target)) => Some(target),
            None => None,
        }
    }

    /// True means retained locally; false asks the caller to submit the first
    /// wrap normally and call started only if the service admits it.
    pub fn offer(&mut self, target: &Target, plays: u32) -> Result<bool, String> {
        let Some(current) = self.target() else {
            self.notice = None;
            self.rejected = 0;
            return Ok(false);
        };
        if current != target {
            return Err("Repeat context changed; pending wraps must be cancelled first.".into());
        }
        if self.waiting.len() == MAX_WAITING {
            self.rejected = self.rejected.saturating_add(1);
            let message = format!(
                "{} Repeat intents not queued: {MAX_WAITING} already waiting.",
                self.rejected
            );
            self.notice = Some(message.clone());
            return Err(message);
        }
        self.waiting.push_back(plays);
        Ok(true)
    }

    pub fn started(&mut self, target: Target, plays: u32) {
        self.chain = Some(Chain::Saving { target, plays });
    }

    pub fn context_matches(&self, current: Option<&Target>) -> bool {
        self.target().is_none_or(|target| Some(target) == current)
    }

    /// Clearing a queue never rolls back its submitted edit. Return the exact
    /// waiting count so diagnostic input origins can be discarded too.
    pub fn cancel(&mut self, reason: &str) -> usize {
        let count = self.waiting.len();
        self.waiting.clear();
        self.chain = None;
        if count > 0 {
            self.notice = Some(format!(
                "Cancelled {count} queued Repeats: {reason}. The submitted edit may finish."
            ));
        }
        count
    }

    /// Ignore same-revision background progress. Any changed revision must be
    /// the exact submitted wrap before it can authorize a continuation.
    pub fn matching_completion(
        &self,
        session: Option<u64>,
        document: Option<&ProjectDocument>,
        committed: Option<&CommittedEdit>,
        failed: bool,
    ) -> Result<bool, &'static str> {
        let Some(Chain::Saving { target, plays }) = &self.chain else {
            return Ok(false);
        };
        if failed {
            return Err("the submitted edit failed");
        }
        let Some(document) = document.filter(|_| session == Some(target.session)) else {
            return Err("the project session changed");
        };
        if document.revision_id() == &target.revision {
            return Ok(false);
        }
        let Some(commit) = committed.filter(|commit| {
            &commit.revision == document.revision_id()
                && commit.scope == target.scope
                && !commit.preserve_cursor
                && commit.cursor.is_none()
        }) else {
            return Err("the edit completion did not match");
        };
        let Some(wrapper) = commit
            .selected_node
            .as_ref()
            .filter(|id| *id != &target.node)
        else {
            return Err("the Repeat result has no new wrapper");
        };
        if !matches!(document.nodes().get(wrapper).map(|node| &node.kind),
            Some(NodeKind::Repeat { child, iterations, gap: None, escalation: None })
            if child == &target.node && iterations.len() == *plays)
        {
            return Err("the Repeat result did not match the requested wrap");
        }
        Ok(true)
    }

    /// Called only after adopting the checked completion's workspace, scope,
    /// selected wrapper and cursor. Dispatch waits until this frame's input
    /// has had the opportunity to cancel or change context.
    pub fn completed(&mut self, target: Target) {
        self.chain = (!self.waiting.is_empty()).then_some(Chain::Ready(target));
    }

    pub fn next(&mut self) -> Option<(Target, u32)> {
        match self.chain.take()? {
            Chain::Ready(target) => {
                let plays = self.waiting.pop_front()?;
                self.chain = Some(Chain::Saving {
                    target: target.clone(),
                    plays,
                });
                Some((target, plays))
            }
            saving @ Chain::Saving { .. } => {
                self.chain = Some(saving);
                None
            }
        }
    }

    pub fn status(&self) -> Option<String> {
        let waiting = (self.waiting() > 0).then(|| {
            format!(
                "{} Repeats waiting · Esc cancels pending wraps",
                self.waiting()
            )
        });
        match (waiting, &self.notice) {
            (Some(waiting), Some(notice)) => Some(format!("{waiting}. {notice}")),
            (Some(waiting), None) => Some(waiting),
            (None, notice) => notice.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
