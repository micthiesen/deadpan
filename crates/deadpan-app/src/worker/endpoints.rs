//! Independent, replaceable endpoint pair. The main picture queue is untouched.

use super::*;
use crate::project::slice::CopiedViewId;
use deadpan_core::{ProjectFrame, ProjectId, RevisionId};
use deadpan_playback::ContentIdentity;

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;

/// One half-open source selection, captured in a committed destination context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointIdentity {
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub draft: u64,
    pub change: u64,
    pub source: EndpointSourceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndpointSourceId {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        in_frame: SourceFrameId,
        out_frame: SourceFrameId,
    },
    Copied(CopiedViewId),
}

pub enum EndpointInput {
    Original(Arc<Workspace>),
    Copied(Arc<CopiedView>),
}

pub struct EndpointPictures {
    pub first: Picture,
    pub last: Picture,
}

pub struct EndpointReply {
    pub identity: EndpointIdentity,
    pub pictures: Result<EndpointPictures, String>,
}

/// Which comparison side supplies the composition shown at an Edit junction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionSide {
    Before,
    Proposed,
}

/// The authored operation whose affected boundary is being inspected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionRole {
    In,
    Out,
    SlipIn,
    SlipOut,
    Roll,
}

/// Exact request identity for one side of a proposed Edit junction.
///
/// `content` names the candidate under the project's monotonic service contract;
/// it is not a hash of the document. The supplied base and Snapshot Arcs are still
/// checked by `proposed::admit`. `proposal_revision` additionally binds the value
/// identity, including for a Before comparison. It is `None` only when the active
/// draft has no proposed document (the explicit zero/no-op case).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditJunctionIdentity {
    pub session: u64,
    pub project: ProjectId,
    pub base_revision: RevisionId,
    pub draft: u64,
    pub change: u64,
    pub content: ContentIdentity,
    pub proposal_revision: Option<RevisionId>,
    pub inspection: u64,
    pub side: JunctionSide,
    pub role: JunctionRole,
    pub boundary: ProjectFrame,
    pub outgoing: Option<ProjectFrame>,
    pub incoming: Option<ProjectFrame>,
}

/// The committed base and the optional exact service-issued proposed Snapshot.
/// Before requests with a nonzero candidate carry the Snapshot too, so the
/// comparison remains authenticated against the same candidate as Proposed.
pub struct EditJunctionInput {
    pub base: Arc<Workspace>,
    pub snapshot: Option<Arc<deadpan_playback::Snapshot>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionExterior {
    NoOutgoing,
    NoIncoming,
}

/// Exterior absence is a successful slot and is distinct from an authored
/// Background, which remains a `Picture` whose `frame` is `None`.
pub enum EditJunctionPicture {
    Exterior(JunctionExterior),
    Picture(Box<Picture>),
}

pub struct EditJunctionPictures {
    pub outgoing: EditJunctionPicture,
    pub incoming: EditJunctionPicture,
    pub canvas: (u32, u32),
}

pub struct EditJunctionReply {
    pub identity: EditJunctionIdentity,
    pub pictures: Result<EditJunctionPictures, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EndpointWorkIdentity {
    Source(EndpointIdentity),
    EditJunction(EditJunctionIdentity),
}

enum EndpointWork {
    Source(EndpointRequest),
    EditJunction(EditJunctionRequest),
}

impl EndpointWork {
    fn identity(&self) -> EndpointWorkIdentity {
        match self {
            Self::Source(request) => EndpointWorkIdentity::Source(request.identity.clone()),
            Self::EditJunction(request) => {
                EndpointWorkIdentity::EditJunction(request.identity.clone())
            }
        }
    }

    fn cancelled(&self) -> &Arc<AtomicBool> {
        match self {
            Self::Source(request) => &request.cancelled,
            Self::EditJunction(request) => &request.cancelled,
        }
    }
}

struct EditJunctionRequest {
    identity: EditJunctionIdentity,
    input: EditJunctionInput,
    cancelled: Arc<AtomicBool>,
}

struct EndpointRequest {
    identity: EndpointIdentity,
    input: EndpointInput,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct EndpointMailbox {
    latest: Option<EndpointWorkIdentity>,
    pending: Option<EndpointWork>,
    active: Option<Arc<AtomicBool>>,
    reply: Option<EndpointReply>,
    junction_reply: Option<EditJunctionReply>,
    clear_requested: bool,
    shutdown: bool,
}

impl EndpointMailbox {
    fn cancel(&mut self) {
        if let Some(active) = &self.active {
            active.store(true, Ordering::Release);
        }
        if let Some(pending) = &self.pending {
            pending.cancelled().store(true, Ordering::Release);
        }
        self.latest = None;
        self.pending = None;
        self.reply = None;
        self.junction_reply = None;
    }

    fn submit(&mut self, identity: EndpointIdentity, input: EndpointInput) -> bool {
        self.submit_work(EndpointWork::Source(EndpointRequest {
            identity,
            input,
            cancelled: Arc::new(AtomicBool::new(false)),
        }))
    }

    fn submit_junction(
        &mut self,
        identity: EditJunctionIdentity,
        input: EditJunctionInput,
    ) -> bool {
        self.submit_work(EndpointWork::EditJunction(EditJunctionRequest {
            identity,
            input,
            cancelled: Arc::new(AtomicBool::new(false)),
        }))
    }

    fn submit_work(&mut self, work: EndpointWork) -> bool {
        if self.shutdown {
            return false;
        }
        self.cancel();
        self.latest = Some(work.identity());
        self.pending = Some(work);
        true
    }

    fn start_next(&mut self) -> Option<EndpointWork> {
        if self.shutdown {
            return None;
        }
        let request = self.pending.take()?;
        self.active = Some(request.cancelled().clone());
        Some(request)
    }

    fn publish_source(&mut self, reply: EndpointReply, cancelled: &AtomicBool) -> bool {
        self.active = None;
        if self.shutdown
            || cancelled.load(Ordering::Acquire)
            || self.latest.as_ref() != Some(&EndpointWorkIdentity::Source(reply.identity.clone()))
        {
            return false;
        }
        self.reply = Some(reply);
        true
    }

    fn publish_junction(&mut self, reply: EditJunctionReply, cancelled: &AtomicBool) -> bool {
        self.active = None;
        if self.shutdown
            || cancelled.load(Ordering::Acquire)
            || self.latest.as_ref()
                != Some(&EndpointWorkIdentity::EditJunction(reply.identity.clone()))
        {
            return false;
        }
        self.junction_reply = Some(reply);
        true
    }
}

#[derive(Default)]
struct EndpointShared {
    mailbox: Mutex<EndpointMailbox>,
    changed: Condvar,
}

pub struct EndpointWorker {
    shared: Arc<EndpointShared>,
}

impl EndpointWorker {
    pub fn new(context: egui::Context) -> std::io::Result<Self> {
        let shared = Arc::new(EndpointShared::default());
        let background = shared.clone();
        std::thread::Builder::new()
            .name("deadpan-source-endpoints".into())
            .spawn(move || run_endpoints(background, context))?;
        Ok(Self { shared })
    }

    pub fn submit(&self, identity: EndpointIdentity, input: EndpointInput) {
        if self
            .shared
            .mailbox
            .lock()
            .expect("endpoint mailbox")
            .submit(identity, input)
        {
            self.shared.changed.notify_one();
        }
    }

    pub fn take_reply(&self) -> Option<EndpointReply> {
        self.shared
            .mailbox
            .lock()
            .expect("endpoint mailbox")
            .reply
            .take()
    }

    /// Submit one authenticated committed/proposed Edit boundary pair through
    /// the same replaceable queue and retained decoder as source endpoints.
    pub fn submit_junction(&self, identity: EditJunctionIdentity, input: EditJunctionInput) {
        if self
            .shared
            .mailbox
            .lock()
            .expect("endpoint mailbox")
            .submit_junction(identity, input)
        {
            self.shared.changed.notify_one();
        }
    }

    pub fn take_junction_reply(&self) -> Option<EditJunctionReply> {
        self.shared
            .mailbox
            .lock()
            .expect("endpoint mailbox")
            .junction_reply
            .take()
    }

    /// Drop native resources on the endpoint worker, never on the UI thread.
    pub fn clear(&self) {
        let mut mailbox = self.shared.mailbox.lock().expect("endpoint mailbox");
        mailbox.cancel();
        mailbox.clear_requested = true;
        self.shared.changed.notify_one();
    }

    pub fn shutdown(&self) {
        let mut mailbox = self.shared.mailbox.lock().expect("endpoint mailbox");
        mailbox.cancel();
        mailbox.shutdown = true;
        self.shared.changed.notify_one();
    }
}

impl Drop for EndpointWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_endpoints(shared: Arc<EndpointShared>, context: egui::Context) {
    let mut retained = None;
    let mut plan = None;
    loop {
        let request = {
            let mut mailbox = shared.mailbox.lock().expect("endpoint mailbox");
            loop {
                if mailbox.shutdown {
                    return;
                }
                if std::mem::take(&mut mailbox.clear_requested) {
                    break None;
                }
                if let Some(request) = mailbox.start_next() {
                    break Some(request);
                }
                mailbox = shared.changed.wait(mailbox).expect("endpoint mailbox");
            }
        };
        let Some(request) = request else {
            retained = None;
            plan = None;
            continue;
        };
        let published = match request {
            EndpointWork::Source(request) => {
                let cancelled = Arc::clone(&request.cancelled);
                let pictures = endpoint_pictures(&request, &mut retained, &mut plan);
                shared
                    .mailbox
                    .lock()
                    .expect("endpoint mailbox")
                    .publish_source(
                        EndpointReply {
                            identity: request.identity,
                            pictures,
                        },
                        &cancelled,
                    )
            }
            EndpointWork::EditJunction(request) => {
                let cancelled = Arc::clone(&request.cancelled);
                let pictures = edit_junction_pictures(&request, &mut retained, &mut plan);
                shared
                    .mailbox
                    .lock()
                    .expect("endpoint mailbox")
                    .publish_junction(
                        EditJunctionReply {
                            identity: request.identity,
                            pictures,
                        },
                        &cancelled,
                    )
            }
        };
        if published {
            context.request_repaint();
        }
    }
}

fn endpoint_pictures(
    request: &EndpointRequest,
    retained: &mut Option<RetainedSession>,
    plan: &mut Option<PlanCache>,
) -> Result<EndpointPictures, String> {
    let identity = &request.identity;
    if identity.session == 0 || identity.draft == 0 || identity.change == 0 {
        return Err("Endpoints require an active captured placement.".into());
    }
    match (&identity.source, &request.input) {
        (EndpointSourceId::Original { .. }, EndpointInput::Original(workspace)) => {
            original_endpoints(request, workspace, retained)
        }
        (EndpointSourceId::Copied(id), EndpointInput::Copied(view)) => {
            if id != view.id()
                || identity.session != id.copy.session
                || identity.project != id.copy.project
            {
                return Err("Endpoints belong to another copied source view.".into());
            }
            let duration = slice_view::admit_copied(view, plan, &request.cancelled)?.duration();
            let first = slice_view::copied_picture(
                view,
                ProjectFrame(0),
                plan,
                &request.cancelled,
                retained,
            )?;
            let last = slice_view::copied_picture(
                view,
                ProjectFrame(duration.frames() - 1),
                plan,
                &request.cancelled,
                retained,
            )?;
            Ok(EndpointPictures { first, last })
        }
        _ => Err("Endpoint source kind differs from its admitted input.".into()),
    }
}

fn edit_junction_pictures(
    request: &EditJunctionRequest,
    retained: &mut Option<RetainedSession>,
    plan_cache: &mut Option<PlanCache>,
) -> Result<EditJunctionPictures, String> {
    let identity = &request.identity;
    let base = &request.input.base;
    if identity.session == 0
        || identity.draft == 0
        || identity.change == 0
        || identity.inspection == 0
    {
        return Err("Edit junctions require an active captured inspection.".into());
    }
    if identity.session != base.session
        || identity.project != *base.document.project_id()
        || identity.base_revision != *base.document.revision_id()
    {
        return Err("Edit junction belongs to another committed base.".into());
    }

    match (&identity.content, &request.input.snapshot) {
        (ContentIdentity::Committed, None) => {
            if identity.proposal_revision.is_some() {
                return Err("A zero proposal cannot carry a proposal revision.".into());
            }
            if request.cancelled.load(Ordering::Acquire) {
                return Err("Edit junction was cancelled.".into());
            }
            junction_pictures_for(request, &base.document, &base.plan, retained)
        }
        (
            ContentIdentity::Proposed {
                base_revision,
                draft,
                change,
            },
            Some(snapshot),
        ) => {
            if base_revision != &identity.base_revision
                || *draft != identity.draft
                || *change != identity.change
                || snapshot.content != identity.content
                || snapshot.session != identity.session
                || identity.proposal_revision.as_ref() != Some(snapshot.document.revision_id())
            {
                return Err(
                    "Edit junction proposal identity differs from its admitted Snapshot.".into(),
                );
            }
            // This validates the exact supplied base/Snapshot Arcs and source
            // receipts even when rendering the Before side of the comparison.
            let proposed_plan = proposed::admit(base, snapshot, plan_cache, &request.cancelled)?;
            match identity.side {
                JunctionSide::Before => {
                    junction_pictures_for(request, &base.document, &base.plan, retained)
                }
                JunctionSide::Proposed => {
                    junction_pictures_for(request, &snapshot.document, proposed_plan, retained)
                }
            }
        }
        _ => Err("Edit junction requires the exact active proposal Snapshot.".into()),
    }
}

fn junction_pictures_for(
    request: &EditJunctionRequest,
    document: &deadpan_core::ProjectDocument,
    plan: &deadpan_plan::RenderPlan,
    retained: &mut Option<RetainedSession>,
) -> Result<EditJunctionPictures, String> {
    let identity = &request.identity;
    let duration = plan.duration().frames();
    if duration < 0 {
        return Err("Edit duration cannot be negative.".into());
    }
    let boundary = identity.boundary.0;
    if boundary < 0 {
        return Err("Edit junction boundary cannot be negative.".into());
    }
    if boundary > duration {
        return Err("Edit junction boundary is outside the selected Edit.".into());
    }
    let outgoing = if boundary == 0 {
        None
    } else {
        Some(ProjectFrame(boundary - 1))
    };
    let incoming = if boundary == duration {
        None
    } else {
        Some(identity.boundary)
    };
    if identity.outgoing != outgoing || identity.incoming != incoming {
        return Err("Edit junction frame addresses do not match its boundary.".into());
    }
    let basis = document.presentation_basis();
    let canvas = (basis.width, basis.height);
    let picture = |frame: ProjectFrame, retained: &mut Option<RetainedSession>| {
        project_picture(
            &request.input.base,
            document,
            plan,
            &ProjectView::Sequence { frame },
            &request.cancelled,
            retained,
        )
    };
    let outgoing = match outgoing {
        Some(frame) => EditJunctionPicture::Picture(Box::new(picture(frame, retained)?)),
        None => EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing),
    };
    let incoming = match incoming {
        Some(frame) => EditJunctionPicture::Picture(Box::new(picture(frame, retained)?)),
        None => EditJunctionPicture::Exterior(JunctionExterior::NoIncoming),
    };
    if request.cancelled.load(Ordering::Acquire) {
        return Err("Edit junction was cancelled.".into());
    }
    Ok(EditJunctionPictures {
        outgoing,
        incoming,
        canvas,
    })
}

fn original_endpoints(
    request: &EndpointRequest,
    workspace: &Workspace,
    retained: &mut Option<RetainedSession>,
) -> Result<EndpointPictures, String> {
    let identity = &request.identity;
    let EndpointSourceId::Original {
        asset,
        qualification,
        in_frame,
        out_frame,
    } = &identity.source
    else {
        return Err("Original endpoints require Original source identity.".into());
    };
    if identity.session != workspace.session
        || &identity.project != workspace.document.project_id()
        || &identity.revision != workspace.document.revision_id()
        || identity.draft == 0
        || identity.change == 0
    {
        return Err("Source endpoints belong to another captured placement.".into());
    }
    let registered = registered_source(workspace, asset)?;
    if registered.receipt.id() != qualification {
        return Err("Original endpoint qualification has changed.".into());
    }
    let index = registered
        .video_index
        .as_ref()
        .ok_or("Source has no picture index.")?;
    let count = u64::try_from(index.frames().len()).map_err(|_| "Source index is too large.")?;
    if in_frame.0 >= out_frame.0 || out_frame.0 > count {
        return Err("Choose a nonempty source range within the measured picture index.".into());
    }
    let last = SourceFrameId(out_frame.0 - 1);
    let mut decode = |frame| {
        project_picture(
            workspace,
            &workspace.document,
            &workspace.plan,
            &ProjectView::Source {
                asset: asset.clone(),
                frame,
            },
            &request.cancelled,
            retained,
        )
    };
    let first = decode(*in_frame)?;
    let last = decode(last)?;
    if request.cancelled.load(Ordering::Acquire) {
        return Err("Source endpoints were cancelled.".into());
    }
    Ok(EndpointPictures { first, last })
}
