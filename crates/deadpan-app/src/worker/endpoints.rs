//! Independent, replaceable endpoint pair. The main picture queue is untouched.

use super::*;
use crate::project::slice::CopiedViewId;
use deadpan_core::{ProjectId, RevisionId};

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

struct EndpointRequest {
    identity: EndpointIdentity,
    input: EndpointInput,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct EndpointMailbox {
    latest: Option<EndpointIdentity>,
    pending: Option<EndpointRequest>,
    active: Option<Arc<AtomicBool>>,
    reply: Option<EndpointReply>,
    clear_requested: bool,
    shutdown: bool,
}

impl EndpointMailbox {
    fn cancel(&mut self) {
        if let Some(active) = &self.active {
            active.store(true, Ordering::Release);
        }
        if let Some(pending) = &self.pending {
            pending.cancelled.store(true, Ordering::Release);
        }
        self.latest = None;
        self.pending = None;
        self.reply = None;
    }

    fn submit(&mut self, identity: EndpointIdentity, input: EndpointInput) -> bool {
        if self.shutdown {
            return false;
        }
        self.cancel();
        self.latest = Some(identity.clone());
        self.pending = Some(EndpointRequest {
            identity,
            input,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        true
    }

    fn start_next(&mut self) -> Option<EndpointRequest> {
        if self.shutdown {
            return None;
        }
        let request = self.pending.take()?;
        self.active = Some(request.cancelled.clone());
        Some(request)
    }

    fn publish(&mut self, reply: EndpointReply, cancelled: &AtomicBool) -> bool {
        self.active = None;
        if self.shutdown
            || cancelled.load(Ordering::Acquire)
            || self.latest.as_ref() != Some(&reply.identity)
        {
            return false;
        }
        self.reply = Some(reply);
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
        let pictures = endpoint_pictures(&request, &mut retained, &mut plan);
        let published = shared.mailbox.lock().expect("endpoint mailbox").publish(
            EndpointReply {
                identity: request.identity,
                pictures,
            },
            &request.cancelled,
        );
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
