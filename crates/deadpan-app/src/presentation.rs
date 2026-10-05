//! Picture presentation state, independent of the native window and GPU.

use deadpan_core::{ProjectId, RevisionId, SourceFrameId};

use crate::project::slice::CopiedViewId;
use crate::worker::{Picture, PictureTier, ProjectView, Reply, SourceSummary, Ticket, Work};

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
    Proposed {
        session: u64,
        project: ProjectId,
        revision: RevisionId,
        content: deadpan_playback::ContentIdentity,
        view: ProjectView,
    },
    Copied {
        source: CopiedViewId,
        frame: deadpan_core::ProjectFrame,
    },
    Candidate {
        session: u64,
        project: ProjectId,
        revision: RevisionId,
        request: deadpan_jobs::RequestId,
        frame: deadpan_core::ProjectFrame,
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
            Work::Proposed { snapshot, view, .. } => Location::Proposed {
                session: snapshot.session,
                project: snapshot.document.project_id().clone(),
                revision: snapshot.document.revision_id().clone(),
                content: snapshot.content.clone(),
                view: view.clone(),
            },
            Work::EditedProposed {
                snapshot, frame, ..
            } => Location::Proposed {
                session: snapshot.session,
                project: snapshot.document.project_id().clone(),
                revision: snapshot.document.revision_id().clone(),
                content: snapshot.content.clone(),
                view: ProjectView::Sequence { frame: *frame },
            },
            Work::Copied { view, frame } => Location::Copied {
                source: view.id().clone(),
                frame: *frame,
            },
            Work::Candidate {
                candidate, frame, ..
            } => Location::Candidate {
                session: candidate.session(),
                project: candidate.project().clone(),
                revision: candidate.base().clone(),
                request: candidate.request().clone(),
                frame: *frame,
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
            }
            | Location::Proposed {
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
            Location::Proposed {
                view: ProjectView::Sequence { frame },
                ..
            } => Some(format!(
                "Showing proposed edit frame {}",
                i128::from(frame.0) + 1
            )),
            Location::Candidate { frame, .. } => Some(format!(
                "Showing AI preview frame {}",
                i128::from(frame.0) + 1
            )),
            Location::Copied { source, frame } => Some(format!(
                "Showing copied Edit frame {}",
                i128::from(source.range.start().0) + i128::from(frame.0) + 1
            )),
        }
    }
}

struct DecodedPicture {
    request: RequestedPicture,
    picture: Picture,
    geometry_revision: u64,
}

struct DisplayedPicture {
    request: RequestedPicture,
    /// A proxy picture is a distinct presentation identity from the exact
    /// Original picture of the same request.
    tier: PictureTier,
    source_frame: Option<SourceFrameId>,
    canvas: Option<(u32, u32)>,
    geometry_revision: u64,
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
    #[cfg(feature = "ui-harness")]
    pub(crate) fn displayed_matches(
        &self,
        session: u64,
        project: &ProjectId,
        revision: &RevisionId,
        view: &ProjectView,
    ) -> bool {
        self.displayed.as_ref().is_some_and(|picture| {
            matches!(&picture.request.location, Location::Project {
                session: current_session, project: current_project,
                revision: current_revision, view: current_view, ..
            } if *current_session == session && current_project == project
                && current_revision == revision && current_view == view)
        })
    }

    /// The displayed picture is an AI pause candidate's preview frame.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn displayed_candidate(&self) -> bool {
        self.displayed
            .as_ref()
            .is_some_and(|picture| matches!(picture.request.location, Location::Candidate { .. }))
    }

    #[cfg(feature = "ui-harness")]
    pub(crate) fn decoded_ticket(&self) -> Option<Ticket> {
        self.decoded.as_ref().map(|picture| picture.request.ticket)
    }

    /// Developer observation only. These are app submission identities, not
    /// timestamps from a physical display or a substitute for the render plan.
    #[cfg(feature = "ui-harness")]
    pub(crate) fn diagnostic_snapshot(&self) -> serde_json::Value {
        fn request(request: &RequestedPicture) -> serde_json::Value {
            serde_json::json!({
                "ticket": format!("{:?}", request.ticket),
                "location": format!("{:?}", request.location),
                "label": request.label(),
            })
        }
        serde_json::json!({
            "requested": self.requested.as_ref().map(request),
            "decoded": self.decoded.as_ref().map(|picture| request(&picture.request)),
            "displayed": self.displayed.as_ref().map(|picture| request(&picture.request)),
            "geometry_revision": self.displayed.as_ref().map(|picture| picture.geometry_revision),
            "decoded_geometry_revision": self.decoded.as_ref().map(|picture| picture.geometry_revision),
            "decoded_framing": self.decoded.as_ref().map(|picture| format!("{:?}", picture.picture.framing)),
            "source_frame": self.displayed_source_frame().map(|frame| frame.0),
            "decoded_tier": self.decoded.as_ref().map(|picture| format!("{:?}", picture.picture.tier)),
            "displayed_tier": self.displayed.as_ref().map(|picture| format!("{:?}", picture.tier)),
            "loading": self.loading(), "needs_render": self.needs_render(),
            "error": self.error(),
        })
    }

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
        self.receive_with_display_retention(reply, false)
    }

    /// Slip owns a stopped comparison and can keep its accepted picture when
    /// decoding the replacement fails. The current request and error remain
    /// visible; a retained display never satisfies the new proposal's gate.
    pub fn receive_retaining_display(
        &mut self,
        reply: Reply,
    ) -> Option<Result<Option<SourceSummary>, String>> {
        self.receive_with_display_retention(reply, true)
    }

    fn receive_with_display_retention(
        &mut self,
        reply: Reply,
        retain_display: bool,
    ) -> Option<Result<Option<SourceSummary>, String>> {
        let request = self.requested.as_ref()?;
        if request.ticket != reply.ticket {
            return None;
        }
        // A failed refinement keeps the request's proxy picture on screen;
        // the error still says the exact picture is unavailable.
        let retain_display = retain_display || self.refining();
        self.loading = false;
        self.error = None;
        self.render_failed = false;
        Some(match reply.picture {
            Ok(mut picture) => {
                let summary = picture.summary.take();
                self.decoded = Some(DecodedPicture {
                    request: request.clone(),
                    picture,
                    geometry_revision: 0,
                });
                Ok(summary)
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.decoded = None;
                if !retain_display {
                    self.displayed = None;
                }
                Err(error)
            }
        })
    }

    pub fn picture(&self) -> Option<&Picture> {
        self.decoded.as_ref().map(|decoded| &decoded.picture)
    }

    /// Camera entry requires the exact stopped Sequence picture to have reached
    /// GPU submission. A retained image from another request is not its target.
    pub fn stable_sequence_ticket(
        &self,
        session: u64,
        revision: &RevisionId,
        frame: deadpan_core::ProjectFrame,
    ) -> Option<Ticket> {
        if self.loading || self.render_failed || self.needs_render() {
            return None;
        }
        let decoded = self.decoded.as_ref()?;
        let displayed = self.displayed.as_ref()?;
        if self.requested.as_ref() != Some(&decoded.request)
            || displayed.request != decoded.request
            || displayed.tier != PictureTier::Original
            || decoded.picture.tier != PictureTier::Original
            || decoded.picture.frame.is_none()
        {
            return None;
        }
        match &decoded.request.location {
            Location::Project {
                session: current_session,
                revision: current_revision,
                view:
                    ProjectView::Sequence {
                        frame: current_frame,
                    },
                empty_sequence: false,
                ..
            } if *current_session == session
                && current_revision == revision
                && *current_frame == frame =>
            {
                Some(decoded.request.ticket)
            }
            _ => None,
        }
    }

    /// A proposed edit is usable only after its current decode was submitted.
    /// The caller also checks the current raster, which belongs to the renderer.
    /// This does not grant committed Camera eligibility.
    pub fn stable_proposed_ticket(
        &self,
        session: u64,
        project: &ProjectId,
        revision: &RevisionId,
        content: &deadpan_playback::ContentIdentity,
        frame: deadpan_core::ProjectFrame,
    ) -> Option<Ticket> {
        if self.loading || self.render_failed || self.needs_render() {
            return None;
        }
        let decoded = self.decoded.as_ref()?;
        let displayed = self.displayed.as_ref()?;
        if self.requested.as_ref() != Some(&decoded.request)
            || displayed.request != decoded.request
            || displayed.tier != PictureTier::Original
            || decoded.picture.tier != PictureTier::Original
            || decoded.picture.frame.is_none()
        {
            return None;
        }
        match &decoded.request.location {
            Location::Proposed {
                session: current_session,
                project: current_project,
                revision: current_revision,
                content: current_content,
                view:
                    ProjectView::Sequence {
                        frame: current_frame,
                    },
            } if *current_session == session
                && current_project == project
                && current_revision == revision
                && current_content == content
                && *current_frame == frame =>
            {
                Some(decoded.request.ticket)
            }
            _ => None,
        }
    }

    /// Replace one evaluated spatial operation on the retained decoded picture.
    /// This is transient presentation state, never an authored document mutation.
    /// Old tickets cannot modify a new request, even before its decode arrives.
    #[cfg(test)]
    pub fn set_framing_pose(
        &mut self,
        ticket: Ticket,
        scope: &deadpan_core::InstancePath,
        pose: Option<deadpan_core::FramingPose>,
    ) -> Result<(), String> {
        self.set_framing_poses(ticket, &[(scope.clone(), pose)])
    }

    /// Replace several evaluated operations of the retained picture at once,
    /// such as a Camera layer and the outer follows that see it. Every scope
    /// and pose is checked before any changes, so a failure changes nothing.
    pub fn set_framing_poses(
        &mut self,
        ticket: Ticket,
        poses: &[(
            deadpan_core::InstancePath,
            Option<deadpan_core::FramingPose>,
        )],
    ) -> Result<(), String> {
        if self
            .requested
            .as_ref()
            .is_none_or(|request| request.ticket != ticket)
        {
            return Err("The Camera picture changed before the adjustment.".into());
        }
        for (_, pose) in poses {
            if let Some(pose) = pose {
                pose.validate().map_err(|error| error.to_string())?;
            }
        }
        let decoded = self
            .decoded
            .as_mut()
            .filter(|decoded| decoded.request.ticket == ticket)
            .ok_or("The Camera picture is no longer available.")?;
        let mut indices = Vec::with_capacity(poses.len());
        for (scope, _) in poses {
            indices.push(
                decoded
                    .picture
                    .framing
                    .iter()
                    .position(|layer| !layer.escalation && &layer.instance == scope)
                    .ok_or("The Camera scope is absent from this picture.")?,
            );
        }
        if indices
            .iter()
            .zip(poses)
            .all(|(index, (_, pose))| decoded.picture.framing[*index].pose == *pose)
        {
            return Ok(());
        }
        let next = decoded
            .geometry_revision
            .checked_add(1)
            .ok_or("Camera preview identities are exhausted. Reopen the project.")?;
        for (index, (_, pose)) in indices.into_iter().zip(poses) {
            decoded.picture.framing[index].pose = *pose;
        }
        decoded.geometry_revision = next;
        self.render_failed = false;
        self.error = None;
        Ok(())
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

    /// The current request's proxy picture arrived and its exact Original
    /// picture is still to come. Exact-picture gates wait rather than fail.
    pub fn refining(&self) -> bool {
        self.decoded.as_ref().is_some_and(|decoded| {
            decoded.picture.tier == PictureTier::Proxy
                && self.requested.as_ref() == Some(&decoded.request)
        })
    }

    /// The tier of the picture last submitted to the GPU.
    pub fn displayed_tier(&self) -> Option<PictureTier> {
        self.displayed.as_ref().map(|displayed| displayed.tier)
    }

    pub fn needs_render(&self) -> bool {
        !self.render_failed
            && self.decoded.as_ref().is_some_and(|decoded| {
                self.displayed.as_ref().is_none_or(|displayed| {
                    displayed.request != decoded.request
                        || displayed.tier != decoded.picture.tier
                        || displayed.geometry_revision != decoded.geometry_revision
                })
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
            tier: decoded.picture.tier,
            source_frame: decoded.picture.frame.as_ref().map(|_| decoded.picture.id),
            canvas: decoded.picture.canvas,
            geometry_revision: decoded.geometry_revision,
        });
    }

    pub fn has_displayed(&self) -> bool {
        self.displayed.is_some()
    }

    pub fn displayed_label(&self) -> Option<String> {
        let displayed = self.displayed.as_ref()?;
        let label = displayed.request.label()?;
        Some(match displayed.tier {
            PictureTier::Original => label,
            PictureTier::Proxy => format!("{label} · proxy preview"),
        })
    }

    pub fn displayed_source_frame(&self) -> Option<SourceFrameId> {
        self.displayed.as_ref()?.source_frame
    }
}
