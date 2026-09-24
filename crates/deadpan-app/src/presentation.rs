//! Picture presentation state, independent of the native window and GPU.

use deadpan_core::{ProjectId, RevisionId, SourceFrameId};

use crate::worker::{Picture, ProjectView, Reply, SourceSummary, Ticket, Work};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Location {
    Standalone(SourceFrameId),
    Project {
        session: u64,
        project: ProjectId,
        revision: RevisionId,
        view: ProjectView,
        empty_sequence: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RequestedPicture {
    ticket: Ticket,
    location: Location,
}

impl RequestedPicture {
    fn new(ticket: Ticket, work: &Work) -> Self {
        let location = match work {
            Work::Open(_) => Location::Standalone(SourceFrameId(0)),
            Work::Frame(frame) => Location::Standalone(*frame),
            Work::Project { workspace, view } => Location::Project {
                session: workspace.session,
                project: workspace.document.project_id().clone(),
                revision: workspace.document.revision_id().clone(),
                view: view.clone(),
                empty_sequence: matches!(view, ProjectView::Sequence { .. })
                    && workspace.plan.duration().frames() == 0,
            },
        };
        Self { ticket, location }
    }

    fn label(&self) -> Option<String> {
        match &self.location {
            Location::Standalone(frame)
            | Location::Project {
                view: ProjectView::Source { frame, .. },
                ..
            } => Some(format!("Showing source frame {}", u128::from(frame.0) + 1)),
            Location::Project {
                empty_sequence: true,
                ..
            } => None,
            Location::Project {
                view: ProjectView::Sequence { frame },
                ..
            } => Some(format!(
                "Showing sequence frame {}",
                i128::from(frame.0) + 1
            )),
        }
    }
}

struct DecodedPicture {
    request: RequestedPicture,
    picture: Picture,
}

struct DisplayedPicture {
    request: RequestedPicture,
    source_frame: Option<SourceFrameId>,
    canvas: Option<(u32, u32)>,
}

#[derive(Default)]
pub struct Presentation {
    requested: Option<RequestedPicture>,
    decoded: Option<DecodedPicture>,
    displayed: Option<DisplayedPicture>,
    loading: bool,
    error: Option<String>,
    render_failed: bool,
}

impl Presentation {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Revoke work from a stopped transport while retaining the last submitted
    /// display identity and texture. No pending decode may cross this boundary.
    pub fn invalidate_pending(&mut self) {
        self.requested = None;
        self.decoded = None;
        self.loading = false;
        self.error = None;
        self.render_failed = false;
    }

    pub fn request(&mut self, ticket: Ticket, work: &Work) {
        self.requested = Some(RequestedPicture::new(ticket, work));
        self.loading = true;
        self.error = None;
        self.render_failed = false;
        // An already accepted picture remains usable while the next request
        // decodes, even if it is still waiting for the GPU. Its identity does
        // not change. A late completion for an older request is rejected below.
    }

    /// None means stale: no presentation state or caller metadata may change.
    pub fn receive(&mut self, reply: Reply) -> Option<Result<Option<SourceSummary>, String>> {
        let request = self.requested.as_ref()?;
        if request.ticket != reply.ticket {
            return None;
        }
        self.loading = false;
        self.error = None;
        self.render_failed = false;
        Some(match reply.picture {
            Ok(mut picture) => {
                let summary = picture.summary.take();
                self.decoded = Some(DecodedPicture {
                    request: request.clone(),
                    picture,
                });
                Ok(summary)
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.decoded = None;
                self.displayed = None;
                Err(error)
            }
        })
    }

    pub fn picture(&self) -> Option<&Picture> {
        self.decoded.as_ref().map(|decoded| &decoded.picture)
    }

    pub fn canvas(&self) -> Option<(u32, u32)> {
        self.picture()
            .and_then(|picture| picture.canvas)
            .or_else(|| {
                self.displayed
                    .as_ref()
                    .and_then(|displayed| displayed.canvas)
            })
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn needs_render(&self) -> bool {
        !self.render_failed
            && self.decoded.as_ref().is_some_and(|decoded| {
                self.displayed
                    .as_ref()
                    .is_none_or(|displayed| displayed.request != decoded.request)
            })
    }

    pub fn render_failed(&mut self, error: String) {
        self.render_failed = true;
        self.error = Some(error);
    }

    pub fn can_render(&self) -> bool {
        !self.render_failed
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Call only after successful GPU submission, or when a decoded background
    /// replaces the old texture. Submission is not a physical display timestamp.
    pub fn presented(&mut self) {
        self.error = None;
        self.displayed = self.decoded.as_ref().map(|decoded| DisplayedPicture {
            request: decoded.request.clone(),
            source_frame: decoded.picture.frame.as_ref().map(|_| decoded.picture.id),
            canvas: decoded.picture.canvas,
        });
    }

    pub fn has_displayed(&self) -> bool {
        self.displayed.is_some()
    }

    pub fn displayed_label(&self) -> Option<String> {
        self.displayed.as_ref()?.request.label()
    }

    pub fn displayed_source_frame(&self) -> Option<SourceFrameId> {
        self.displayed.as_ref()?.source_frame
    }
}
