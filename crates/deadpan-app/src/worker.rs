//! One background source session with replaceable, bounded request/result slots.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
use deadpan_core::SourceTimestamp;
use deadpan_core::{AssetId, ProjectFrame, SourceFrameId, SourceFrameIndex, SourceQualificationId};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_session::{SourceSession, SourceSessionError, SourceSessionLimits};
use deadpan_plan::RenderPlan;
use deadpan_render::Rgba8Frame;
#[cfg(test)]
use deadpan_render::{Primaries, Transfer};
use deadpan_source::{DecodedRgbaFrame, SourceStreamInfo};
use deadpan_store::original_media::OriginalMediaLimits;
use eframe::egui;
use sha2::{Digest, Sha256};

use crate::project::slice::{CopiedView, MediaView};
use crate::project::{RegisteredSource, Workspace};

const HASH_TIMEOUT: Duration = Duration::from_secs(300);
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the cursor must rest on a proxy picture before the worker
/// replaces it with the exact Original picture.
pub const REFINE_DELAY: Duration = Duration::from_millis(150);

mod endpoints;
mod proposed;
mod proxy;
mod slice_view;
pub use endpoints::{
    EditJunctionIdentity, EditJunctionInput, EditJunctionPicture, EditJunctionPictures,
    EditJunctionReply, EndpointIdentity, EndpointInput, EndpointPictures, EndpointReply,
    EndpointSourceId, EndpointWorker, JunctionExterior, JunctionRole, JunctionSide,
};
pub use proxy::PictureTier;
use proxy::ProxySlot;
use slice_view::{PictureMedia, PlanCache};

#[cfg(test)]
mod project_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub transport: Option<deadpan_output::Generation>,
    pub source: u64,
    pub request: u64,
}

pub enum Work {
    Open(PathBuf),
    Frame(SourceFrameId),
    Project {
        workspace: Arc<Workspace>,
        view: ProjectView,
    },
    /// Media authority belongs to the captured committed workspace. The worker
    /// compiles the genuine proposal itself; a caller cannot supply another plan.
    Proposed {
        base: Arc<Workspace>,
        snapshot: Arc<deadpan_playback::Snapshot>,
        view: ProjectView,
    },
    EditedProposed {
        base: Arc<Workspace>,
        snapshot: Arc<deadpan_playback::Snapshot>,
        media: Arc<MediaView>,
        frame: ProjectFrame,
    },
    Copied {
        view: Arc<CopiedView>,
        frame: ProjectFrame,
    },
    /// An AI pause candidate's acceptance preview. Media authority belongs to
    /// the exact committed base the service issued it for; the worker compiles
    /// the service-issued document itself.
    Candidate {
        base: Arc<Workspace>,
        candidate: Arc<crate::project::generation::CandidatePreview>,
        frame: ProjectFrame,
    },
    /// The middle picture of one Ready AI variant's sampled master, for its
    /// inspector thumbnail. Read through the workspace's generated-media
    /// handle; never part of the edit's picture.
    CandidateThumbnail {
        workspace: Arc<Workspace>,
        thumbnail: Arc<CandidateThumbnail>,
    },
}

/// What a variant thumbnail decodes: the sampled master's verified object,
/// its receipt raster and frame count, and the canvas its conditioning
/// letterbox is cropped to (as acceptance records it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateThumbnail {
    pub object: deadpan_core::GeneratedObjectRef,
    pub frames: u32,
    pub size: (u32, u32),
    pub canvas: [u32; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectView {
    Source {
        asset: AssetId,
        frame: SourceFrameId,
    },
    Sequence {
        frame: ProjectFrame,
    },
}

struct Request {
    ticket: Ticket,
    work: Work,
    cancelled: Arc<AtomicBool>,
}

pub struct SourceSummary {
    pub info: SourceStreamInfo,
    pub frame_count: u64,
    pub first_pts: i64,
    pub terminal_pts: i64,
}

pub struct Picture {
    pub summary: Option<SourceSummary>,
    pub id: SourceFrameId,
    pub frame: Option<Rgba8Frame>,
    pub canvas: Option<(u32, u32)>,
    /// Exact sampled scope order, provider to root. Never part of decoded bytes.
    pub framing: Vec<deadpan_plan::PictureFraming>,
    /// A Repeat gap has a provider before its first authored (Repeat) scope.
    pub framing_gap: bool,
    /// Static captured composition before current Hold and ancestor framing.
    pub picture_context: Option<Arc<deadpan_core::CapturedFraming>>,
    /// The asset and exact source time `Follow` layers and target overlays
    /// evaluate at; None for pictures that show no Original moment.
    pub follow_point: Option<(AssetId, deadpan_core::SourcePoint)>,
    /// Caption lines drawn over the composed picture, as in export.
    pub captions: Vec<deadpan_plan::PictureCaption>,
    /// Whether these pixels are the exact Original picture or its preview
    /// proxy. A proxy picture is a distinct presentation identity: it never
    /// satisfies Camera, Slip, Trim or any other exact-picture gate.
    pub tier: PictureTier,
}

pub struct Reply {
    pub ticket: Ticket,
    pub picture: Result<Picture, String>,
    #[cfg(feature = "ui-harness")]
    pub timing: Option<WorkerTiming>,
}

/// Worker clock observations, independent of when the UI receives the reply.
#[cfg(feature = "ui-harness")]
#[derive(Clone, Copy, Debug)]
pub struct WorkerTiming {
    pub started: Instant,
    pub finished: Instant,
    pub published: Option<Instant>,
}

#[derive(Default)]
struct Mailbox {
    latest: Option<Ticket>,
    pending: Option<Request>,
    active: Option<Arc<AtomicBool>>,
    reply: Option<Reply>,
    clear_requested: bool,
    shutdown: bool,
}

impl Mailbox {
    fn submit(&mut self, ticket: Ticket, work: Work) -> bool {
        if self.shutdown {
            return false;
        }
        self.cancel_active();
        self.latest = Some(ticket);
        self.pending = Some(Request {
            ticket,
            work,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        self.reply = None;
        true
    }

    fn start_next(&mut self) -> Option<Request> {
        if self.shutdown {
            return None;
        }
        let request = self.pending.take()?;
        self.active = Some(Arc::clone(&request.cancelled));
        Some(request)
    }

    fn publish(&mut self, reply: Reply) -> bool {
        self.active = None;
        if self.shutdown || self.latest != Some(reply.ticket) {
            return false;
        }
        #[cfg(feature = "ui-harness")]
        let reply = {
            let mut reply = reply;
            if let Some(timing) = &mut reply.timing {
                timing.published = Some(Instant::now());
            }
            reply
        };
        self.reply = Some(reply);
        true
    }

    /// Begin refining the published proxy picture of `request` if it is still
    /// the newest request and nothing newer waits. A later submission cancels
    /// the refinement through the same flag.
    fn start_refinement(&mut self, request: &Request) -> bool {
        if self.shutdown
            || self.clear_requested
            || self.pending.is_some()
            || self.latest != Some(request.ticket)
            || request.cancelled.load(Ordering::Acquire)
        {
            return false;
        }
        self.active = Some(Arc::clone(&request.cancelled));
        true
    }

    fn cancel_active(&self) {
        if let Some(active) = &self.active {
            active.store(true, Ordering::Release);
        }
        if let Some(pending) = &self.pending {
            pending.cancelled.store(true, Ordering::Release);
        }
    }

    fn stop(&mut self) {
        self.shutdown = true;
        self.cancel_active();
        self.pending = None;
        self.reply = None;
    }

    fn clear(&mut self) {
        self.cancel();
        self.clear_requested = true;
    }

    fn cancel(&mut self) {
        self.cancel_active();
        self.latest = None;
        self.pending = None;
        self.reply = None;
    }
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    changed: Condvar,
    /// The per-user preview-proxy cache the main viewer may read.
    proxy_cache: Mutex<Option<deadpan_cli::proxy::cache::ProxyCache>>,
}

pub struct PreviewWorker {
    shared: Arc<Shared>,
}

impl PreviewWorker {
    /// The main viewer's worker. Stopped committed pictures may come from a
    /// preview proxy first and are refined to the Original once the cursor
    /// rests for [`REFINE_DELAY`].
    pub fn new(context: egui::Context) -> std::io::Result<Self> {
        Self::spawn(context, "deadpan-source-preview", true)
    }

    /// An independent worker, such as the card thumbnail service, with its
    /// own decoder and request slot. It always decodes the Original.
    pub fn named(context: egui::Context, name: &str) -> std::io::Result<Self> {
        Self::spawn(context, name, false)
    }

    fn spawn(context: egui::Context, name: &str, proxies: bool) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let background = Arc::clone(&shared);
        // The single thread owns all file I/O, hashing, indexing and decoding.
        // Dropping the handle deliberately avoids a blocking GUI shutdown join.
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || run(background, context, proxies))?;
        Ok(Self { shared })
    }

    pub fn submit(&self, ticket: Ticket, work: Work) {
        if self
            .shared
            .mailbox
            .lock()
            .expect("preview mailbox")
            .submit(ticket, work)
        {
            self.shared.changed.notify_one();
        }
    }

    pub fn take_reply(&self) -> Option<Reply> {
        self.shared
            .mailbox
            .lock()
            .expect("preview mailbox")
            .reply
            .take()
    }

    /// Revoke outstanding pictures while retaining the verified source decoder.
    pub fn cancel(&self) {
        self.shared
            .mailbox
            .lock()
            .expect("preview mailbox")
            .cancel();
    }

    /// The preview-proxy cache this worker may read for stopped seeks. Only
    /// the main viewer's worker (`new`) uses it; others ignore it.
    pub fn set_proxy_cache(&self, cache: Option<deadpan_cli::proxy::cache::ProxyCache>) {
        *self.shared.proxy_cache.lock().expect("proxy cache") = cache;
        self.shared.changed.notify_one();
    }

    pub fn shutdown(&self) {
        self.shared.mailbox.lock().expect("preview mailbox").stop();
        self.shared.changed.notify_one();
    }

    /// Releases preview resources on the service thread without stopping it or
    /// joining from the UI. Subsequent requests remain self-contained.
    pub fn clear(&self) {
        self.shared.mailbox.lock().expect("preview mailbox").clear();
        self.shared.changed.notify_one();
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(shared: Arc<Shared>, context: egui::Context, proxies: bool) {
    let mut session = None;
    let mut proposal = None;
    let mut proxy = ProxySlot::default();
    // Prepare a proxy only after a stopped interactive request, never in the
    // gaps between playback pictures.
    let mut idle_preparation = false;
    loop {
        if proxies {
            proxy.set_cache(shared.proxy_cache.lock().expect("proxy cache").clone());
        }
        let next = {
            let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
            loop {
                if mailbox.shutdown {
                    return;
                }
                if std::mem::take(&mut mailbox.clear_requested) {
                    break Next::Clear;
                }
                if let Some(request) = mailbox.start_next() {
                    break Next::Request(request);
                }
                // Idle: open a wanted proxy, cancellable by the next request.
                if idle_preparation && proxy.wants_preparation() {
                    let cancelled = Arc::new(AtomicBool::new(false));
                    mailbox.active = Some(Arc::clone(&cancelled));
                    break Next::Prepare(cancelled);
                }
                mailbox = shared.changed.wait(mailbox).expect("preview mailbox");
            }
        };
        let request = match next {
            Next::Request(request) => request,
            Next::Clear => {
                // Native teardown and private snapshot deletion stay off the UI
                // and outside the mailbox lock used for submitting requests.
                session = None;
                proposal = None;
                proxy = ProxySlot::default();
                continue;
            }
            Next::Prepare(cancelled) => {
                proxy.prepare(&cancelled);
                let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
                if mailbox
                    .active
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, &cancelled))
                {
                    mailbox.active = None;
                }
                continue;
            }
        };
        // Only stopped committed pictures of the main viewer may start with
        // a proxy; playback, proposals, copies and candidates never do.
        let interactive = proxies
            && request.ticket.transport.is_none()
            && matches!(request.work, Work::Project { .. });
        idle_preparation = interactive;
        let slot = interactive.then_some(&mut proxy);
        let started = Instant::now();
        let picture = perform_with(&request, &mut session, &mut proposal, slot);
        let refine = matches!(&picture, Ok(picture) if picture.tier == PictureTier::Proxy);
        if !publish_reply(&shared, &context, &request, picture, started) || !refine {
            continue;
        }
        // Refine once the cursor rests: wait for a newer request, and decode
        // the exact Original picture if none arrives in time.
        let refining = {
            let deadline = Instant::now() + REFINE_DELAY;
            let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
            loop {
                if mailbox.shutdown {
                    return;
                }
                let now = Instant::now();
                if mailbox.pending.is_some()
                    || mailbox.clear_requested
                    || mailbox.latest != Some(request.ticket)
                {
                    break false;
                }
                if now >= deadline {
                    break mailbox.start_refinement(&request);
                }
                mailbox = shared
                    .changed
                    .wait_timeout(mailbox, deadline - now)
                    .expect("preview mailbox")
                    .0;
            }
        };
        if !refining {
            continue;
        }
        let started = Instant::now();
        let mut picture = perform(&request, &mut session, &mut proposal);
        if picture.is_err() && !request.cancelled.load(Ordering::Acquire) {
            // One more attempt with a reopened decoder before the proxy
            // picture is left showing the error.
            picture = perform(&request, &mut session, &mut proposal);
        }
        publish_reply(&shared, &context, &request, picture, started);
    }
}

enum Next {
    Request(Request),
    Clear,
    Prepare(Arc<AtomicBool>),
}

fn publish_reply(
    shared: &Shared,
    context: &egui::Context,
    request: &Request,
    picture: Result<Picture, String>,
    #[cfg_attr(not(feature = "ui-harness"), allow(unused_variables))] started: Instant,
) -> bool {
    #[cfg(feature = "ui-harness")]
    let timing = Some(WorkerTiming {
        started,
        finished: Instant::now(),
        published: None,
    });
    let publish = shared
        .mailbox
        .lock()
        .expect("preview mailbox")
        .publish(Reply {
            ticket: request.ticket,
            picture,
            #[cfg(feature = "ui-harness")]
            timing,
        });
    if publish {
        context.request_repaint();
    }
    publish
}

#[derive(PartialEq, Eq)]
enum SessionKey {
    Raw(u64),
    Project {
        session: u64,
        asset: AssetId,
        receipt: SourceQualificationId,
    },
    Generated {
        session: u64,
        media: Arc<GeneratedMediaKey>,
    },
}

/// Decoder reuse follows immutable media interpretation, independently of
/// revision/framing changes. A changed asset record must be admitted again.
#[derive(Debug, PartialEq, Eq)]
struct GeneratedMediaKey {
    artifact: Arc<deadpan_core::GeneratedArtifact>,
    native: deadpan_core::AssetRecord,
    sampled: deadpan_core::AssetRecord,
    color: deadpan_core::ColorPolicy,
}

impl GeneratedMediaKey {
    fn new(
        document: &deadpan_core::ProjectDocument,
        artifact: &Arc<deadpan_core::GeneratedArtifact>,
    ) -> Result<Self, String> {
        let record = |id| {
            document
                .assets()
                .get(id)
                .cloned()
                .ok_or_else(|| "Accepted picture asset is absent.".to_owned())
        };
        Ok(Self {
            artifact: artifact.clone(),
            native: record(&artifact.native_asset)?,
            sampled: record(&artifact.sampled_asset)?,
            color: document.presentation_basis().color_policy,
        })
    }
}

struct RetainedSession {
    key: SessionKey,
    source: SourceSession,
    catalog: Option<Arc<RegisteredSource>>,
}

/// Always the exact picture; see [`perform_with`] for proxy pictures.
fn perform(
    request: &Request,
    retained: &mut Option<RetainedSession>,
    proposal: &mut Option<PlanCache>,
) -> Result<Picture, String> {
    perform_with(request, retained, proposal, None)
}

fn perform_with(
    request: &Request,
    retained: &mut Option<RetainedSession>,
    proposal: &mut Option<PlanCache>,
    proxy: Option<&mut ProxySlot>,
) -> Result<Picture, String> {
    if let Work::Project { workspace, view } = &request.work {
        return media_picture(
            PictureMedia::Committed(workspace),
            &workspace.document,
            &workspace.plan,
            view,
            &request.cancelled,
            retained,
            proxy,
        );
    }
    if let Work::Proposed {
        base,
        snapshot,
        view,
    } = &request.work
    {
        let plan = proposed::admit(base, snapshot, proposal, &request.cancelled)?;
        return project_picture(
            base,
            &snapshot.document,
            plan,
            view,
            &request.cancelled,
            retained,
        );
    }
    if let Work::EditedProposed {
        base,
        snapshot,
        media,
        frame,
    } = &request.work
    {
        let plan = slice_view::admit_edited(base, snapshot, media, proposal, &request.cancelled)?;
        let picture = media_picture(
            PictureMedia::Slice(media),
            &snapshot.document,
            plan,
            &ProjectView::Sequence { frame: *frame },
            &request.cancelled,
            retained,
            None,
        )?;
        media
            .admitted()
            .check_live(&request.cancelled)
            .map_err(|error| error.to_string())?;
        return Ok(picture);
    }
    if let Work::Candidate {
        base,
        candidate,
        frame,
    } = &request.work
    {
        let plan = candidate_plan(base, candidate, proposal, &request.cancelled)?;
        return project_picture(
            base,
            candidate.document(),
            plan,
            &ProjectView::Sequence { frame: *frame },
            &request.cancelled,
            retained,
        );
    }
    if let Work::Copied { view, frame } = &request.work {
        return slice_view::copied_picture(view, *frame, proposal, &request.cancelled, retained);
    }
    if let Work::CandidateThumbnail {
        workspace,
        thumbnail,
    } = &request.work
    {
        *retained = None;
        return candidate_thumbnail(workspace, thumbnail, &request.cancelled);
    }
    let (summary, id) = match &request.work {
        Work::Open(path) => {
            *retained = None;
            *proposal = None;
            let source = open_source(path, &request.cancelled)?;
            check_picture_limits(&source)?;
            let summary = source_summary(&source);
            *retained = Some(RetainedSession {
                key: SessionKey::Raw(request.ticket.source),
                source,
                catalog: None,
            });
            (Some(summary), SourceFrameId(0))
        }
        Work::Frame(id) => (None, *id),
        Work::Project { .. }
        | Work::Proposed { .. }
        | Work::EditedProposed { .. }
        | Work::Copied { .. }
        | Work::Candidate { .. }
        | Work::CandidateThumbnail { .. } => {
            unreachable!("project requests handled above")
        }
    };
    let session = retained
        .as_mut()
        .filter(|session| session.key == SessionKey::Raw(request.ticket.source))
        .ok_or("The requested source session is no longer open.")?;
    let decoded = session
        .source
        .frame(id, FRAME_TIMEOUT, &request.cancelled)
        .map_err(|error| error.to_string())?;
    Ok(Picture {
        summary,
        id,
        frame: Some(render_frame(decoded, session.source.info())?),
        canvas: None,
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
        captions: Vec::new(),
        tier: PictureTier::Original,
    })
}

fn project_picture(
    workspace: &Workspace,
    document: &deadpan_core::ProjectDocument,
    plan: &RenderPlan,
    view: &ProjectView,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, String> {
    media_picture(
        PictureMedia::Committed(workspace),
        document,
        plan,
        view,
        cancelled,
        retained,
        None,
    )
}

fn media_picture(
    media: PictureMedia<'_>,
    document: &deadpan_core::ProjectDocument,
    plan: &RenderPlan,
    view: &ProjectView,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
    proxy: Option<&mut ProxySlot>,
) -> Result<Picture, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Project preview was cancelled.".into());
    }
    if plan.metadata().project_id != *document.project_id()
        || plan.metadata().revision_id != *document.revision_id()
    {
        return Err("The picture plan belongs to another project revision.".into());
    }
    let (asset, frame, canvas) = match view {
        ProjectView::Source { asset, frame } => (asset, *frame, None),
        ProjectView::Sequence { frame } => {
            let basis = document.presentation_basis();
            let canvas = Some((basis.width, basis.height));
            if plan.duration().frames() == 0 && frame.0 == 0 {
                return Ok(background_picture(canvas));
            }
            let sample = plan.picture(*frame).map_err(|error| error.to_string())?;
            let asset = match &sample.picture {
                deadpan_plan::Picture::Source { asset, .. }
                | deadpan_plan::Picture::Freeze { asset, .. } => asset,
                deadpan_plan::Picture::Blank | deadpan_plan::Picture::Background => {
                    let mut picture = background_picture(canvas);
                    picture.captions = sample.captions;
                    return Ok(picture);
                }
                deadpan_plan::Picture::Still { .. } => {
                    return Err("Still-image preview is not yet qualified.".into());
                }
                deadpan_plan::Picture::Accepted {
                    asset,
                    generated: Some(artifact),
                    ..
                } => {
                    if asset != &artifact.sampled_asset {
                        return Err(
                            "Generated picture asset disagrees with its accepted artifact.".into(),
                        );
                    }
                    let mut picture = generated_picture(
                        media,
                        document,
                        &sample.picture,
                        artifact,
                        canvas,
                        cancelled,
                        retained,
                    )?;
                    picture.framing = sample.framing;
                    picture.framing_gap = sample.gap_after.is_some();
                    picture.picture_context = sample.picture_context;
                    picture.captions = sample.captions;
                    return Ok(picture);
                }
                deadpan_plan::Picture::Accepted {
                    generated: None, ..
                } => {
                    return Err(
                        "Legacy accepted-media preview has no qualified generated evidence.".into(),
                    );
                }
            };
            let registered = media_source(media, document, asset)?;
            let index = registered
                .video_index
                .as_ref()
                .ok_or("This source has no qualified picture index.")?;
            let frame = sample
                .picture
                .select_source_frame(index)
                .map_err(|error| error.to_string())?
                .identity;
            let mut picture =
                registered_picture(media, registered, frame, canvas, cancelled, retained, proxy)?;
            picture.follow_point = sample
                .picture
                .follow_point()
                .map(|(asset, point)| (asset.clone(), point));
            picture.framing = sample.framing;
            picture.framing_gap = sample.gap_after.is_some();
            picture.picture_context = sample.picture_context;
            picture.captions = sample.captions;
            return Ok(picture);
        }
    };
    let registered = media_source(media, document, asset)?;
    registered_picture(media, registered, frame, canvas, cancelled, retained, proxy)
}

/// Admit a candidate preview against its exact committed base and compile its
/// plan once per issued preview.
fn candidate_plan<'a>(
    base: &Arc<Workspace>,
    candidate: &Arc<crate::project::generation::CandidatePreview>,
    retained: &'a mut Option<PlanCache>,
    cancelled: &AtomicBool,
) -> Result<&'a RenderPlan, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("AI preview picture was cancelled.".into());
    }
    if candidate.session() != base.session
        || candidate.project() != base.document.project_id()
        || candidate.base() != base.document.revision_id()
    {
        return Err("The AI preview belongs to another project revision.".into());
    }
    let cached = retained.as_ref().is_some_and(|previous| {
        matches!(&previous.identity, slice_view::PlanIdentity::Candidate(old)
            if Arc::ptr_eq(old, candidate))
    });
    if !cached {
        let plan = RenderPlan::compile(candidate.document()).map_err(|error| error.to_string())?;
        if plan.duration() != base.plan.duration() {
            return Err("The AI preview changes the edit's timing.".into());
        }
        *retained = Some(PlanCache {
            identity: slice_view::PlanIdentity::Candidate(candidate.clone()),
            plan,
        });
    }
    Ok(&retained.as_ref().expect("admitted candidate plan").plan)
}

/// Decode one Ready variant's middle picture for its thumbnail.
fn candidate_thumbnail(
    workspace: &Workspace,
    thumbnail: &CandidateThumbnail,
    cancelled: &AtomicBool,
) -> Result<Picture, String> {
    let mut source = deadpan_cli::picture::open_candidate_master(
        &workspace.generated,
        &thumbnail.object,
        thumbnail.frames,
        thumbnail.size,
        cancelled,
    )
    .map_err(|error| error.to_string())?;
    let id = source
        .index()
        .index()
        .frames()
        .get(thumbnail.frames as usize / 2)
        .ok_or("The AI variant has no pictures.")?
        .identity;
    let decoded = source
        .frame(id, FRAME_TIMEOUT, cancelled)
        .map_err(|error| error.to_string())?;
    let frame = render_frame(decoded, source.info())?;
    let frame = deadpan_cli::picture::fill_canvas_aspect(frame, thumbnail.canvas)
        .map_err(|error| error.to_string())?;
    Ok(Picture {
        summary: None,
        id,
        frame: Some(frame),
        canvas: None,
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
        captions: Vec::new(),
        tier: PictureTier::Original,
    })
}

fn background_picture(canvas: Option<(u32, u32)>) -> Picture {
    Picture {
        summary: None,
        id: SourceFrameId(0),
        frame: None,
        canvas,
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
        captions: Vec::new(),
        tier: PictureTier::Original,
    }
}

fn generated_picture(
    media: PictureMedia<'_>,
    document: &deadpan_core::ProjectDocument,
    picture: &deadpan_plan::Picture,
    artifact: &Arc<deadpan_core::GeneratedArtifact>,
    canvas: Option<(u32, u32)>,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, String> {
    media
        .generated()
        .check_live(cancelled)
        .map_err(|error| error.to_string())?;
    let key = SessionKey::Generated {
        session: media.session(),
        media: Arc::new(GeneratedMediaKey::new(document, artifact)?),
    };
    if retained.as_ref().is_none_or(|session| session.key != key) {
        *retained = None;
        let source = deadpan_cli::picture::open_generated_picture(
            media.generated(),
            document,
            artifact,
            cancelled,
        )
        .map_err(|error| error.to_string())?;
        *retained = Some(RetainedSession {
            key,
            source,
            catalog: None,
        });
    }
    let session = retained
        .as_mut()
        .ok_or("The generated picture session could not be retained.")?;
    let id = picture
        .select_source_frame(session.source.index().index())
        .map_err(|error| error.to_string())?
        .identity;
    let decoded = session
        .source
        .frame(id, FRAME_TIMEOUT, cancelled)
        .map_err(|error| error.to_string())?;
    let mut frame = render_frame(decoded, session.source.info())?;
    if let Some(aspect) = artifact.content_aspect {
        // Undo the conditioning letterbox so the Hold fills the canvas.
        frame = deadpan_cli::picture::fill_canvas_aspect(frame, aspect)
            .map_err(|error| error.to_string())?;
    }
    media
        .generated()
        .check_live(cancelled)
        .map_err(|error| error.to_string())?;
    Ok(Picture {
        summary: Some(source_summary(&session.source)),
        id,
        frame: Some(frame),
        canvas,
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
        captions: Vec::new(),
        tier: PictureTier::Original,
    })
}

fn registered_source<'a>(
    workspace: &'a Workspace,
    asset: &AssetId,
) -> Result<&'a Arc<RegisteredSource>, String> {
    media_source(
        PictureMedia::Committed(workspace),
        &workspace.document,
        asset,
    )
}

fn media_source<'a>(
    media: PictureMedia<'a>,
    document: &deadpan_core::ProjectDocument,
    asset: &AssetId,
) -> Result<&'a Arc<RegisteredSource>, String> {
    let registered = media
        .sources()
        .get(asset)
        .ok_or("This source has no registered media evidence.")?;
    let authored = document
        .assets()
        .get(asset)
        .ok_or("This source is absent from the selected project revision.")?;
    // Complete contracts are checked by the catalog/proposal constructors and
    // on a sealed slice view's first admission. Keep this hot lookup independent
    // of the receipt's measured audio-index length.
    if registered.asset != *asset
        || authored.source_qualification.as_ref() != Some(registered.receipt.id())
        || authored.content_hash != registered.receipt.original().content().to_string()
        || registered.original.object() != registered.receipt.original()
        || registered.original.sha256() != registered.receipt.snapshot().content().sha256()
    {
        return Err("Source media evidence disagrees with the selected project revision.".into());
    }
    Ok(registered)
}

/// Serve one Original picture through a progressively admitted session.
/// An interrupted background verification (deadline, cancellation or I/O)
/// drops the session and admits once more within this request, so it
/// recovers without a visible error. A mismatch is permanent for these bytes:
/// the failed session stays retained as a tombstone, so every request for
/// this source fails at once with the same clear message instead of copying
/// and re-measuring the Original again. A new project session or receipt
/// admits afresh.
fn registered_picture(
    media: PictureMedia<'_>,
    registered: &Arc<RegisteredSource>,
    id: SourceFrameId,
    canvas: Option<(u32, u32)>,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
    proxy: Option<&mut ProxySlot>,
) -> Result<Picture, String> {
    // A proxy serves a random seek; an Original decoder already positioned
    // at or just before the target is as fast and exact, so it serves steps.
    if let Some(slot) = proxy {
        let key = SessionKey::Project {
            session: media.session(),
            asset: registered.asset.clone(),
            receipt: registered.receipt.id().clone(),
        };
        // A single step (either direction) from the decoder's current picture
        // is shown exactly, without a proxy picture flashing first.
        let stepping = retained
            .as_ref()
            .is_some_and(|session| session.key == key && session.source.is_step(id));
        if !stepping
            && let Some(picture) = slot.picture(media, registered, id, canvas, cancelled)?
        {
            return Ok(picture);
        }
    }
    match registered_picture_once(media, registered, id, canvas, cancelled, retained) {
        Err(Served::Interrupted(_)) => {
            *retained = None;
            registered_picture_once(media, registered, id, canvas, cancelled, retained)
                .map_err(Served::into_message)
        }
        result => result.map_err(Served::into_message),
    }
}

enum Served {
    Interrupted(String),
    Failed(String),
}

impl Served {
    fn into_message(self) -> String {
        match self {
            Self::Interrupted(message) | Self::Failed(message) => message,
        }
    }
}

impl From<String> for Served {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for Served {
    fn from(message: &str) -> Self {
        Self::Failed(message.into())
    }
}

/// Preview decoders: threaded serving, single-threaded measurement.
fn preview_limits() -> SourceSessionLimits {
    #[allow(unused_mut)]
    let mut limits = SourceSessionLimits::interactive();
    #[cfg(test)]
    tests_support::adjust_limits(&mut limits);
    limits
}

#[cfg(test)]
pub(crate) mod tests_support {
    use std::cell::Cell;
    use std::time::Duration;

    use deadpan_media::source_session::SourceSessionLimits;

    thread_local! {
        /// Admissions whose background measurement gets a 1 ns deadline.
        pub static INTERRUPT_MEASUREMENTS: Cell<u32> = const { Cell::new(0) };
        /// Progressive admissions started on this thread.
        pub static ADMISSIONS: Cell<u32> = const { Cell::new(0) };
        /// Admit with the receipt index's last duration changed by one tick.
        pub static TAMPER_INDEX: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn adjust_index(
        index: deadpan_media::source_index::SourceIndexSnapshot,
    ) -> deadpan_media::source_index::SourceIndexSnapshot {
        if !TAMPER_INDEX.get() {
            return index;
        }
        let mut frames = index.index().frames().to_vec();
        let last = frames.last_mut().expect("nonempty index");
        last.reported_duration = last.reported_duration.map(|value| value + 1);
        deadpan_media::source_index::SourceIndexSnapshot::new(
            index.content(),
            index.stream_index(),
            deadpan_core::SourceFrameIndex::new(
                index.index().asset().clone(),
                index.index().time_base(),
                frames,
                index.index().terminal_end(),
                index.index().terminal_provenance(),
            )
            .expect("valid tampered index"),
        )
        .expect("valid tampered snapshot")
    }

    pub(super) fn adjust_limits(limits: &mut SourceSessionLimits) {
        ADMISSIONS.set(ADMISSIONS.get() + 1);
        let remaining = INTERRUPT_MEASUREMENTS.get();
        if remaining > 0 {
            INTERRUPT_MEASUREMENTS.set(remaining - 1);
            limits.measurement_timeout = Duration::from_nanos(1);
        }
    }
}

fn registered_picture_once(
    media: PictureMedia<'_>,
    registered: &Arc<RegisteredSource>,
    id: SourceFrameId,
    canvas: Option<(u32, u32)>,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, Served> {
    media.check_slice_live(cancelled)?;
    let video = registered
        .receipt
        .snapshot()
        .video()
        .ok_or("This source has no qualified picture stream.")?;
    let expected = registered
        .video_index
        .as_ref()
        .ok_or("This source has no qualified picture index.")?;
    // An immutable catalog entry is checked once when it changes, not once per
    // cursor movement through a potentially multi-million-frame source index.
    if retained.as_ref().is_none_or(|session| {
        session
            .catalog
            .as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, registered))
    }) && (expected.asset() != &registered.asset
        || !same_index_mapping(expected, video.index().index(), || {
            cancelled.load(Ordering::Acquire)
        })?)
    {
        return Err("The source picture index disagrees with its immutable receipt.".into());
    }
    let key = SessionKey::Project {
        session: media.session(),
        asset: registered.asset.clone(),
        receipt: registered.receipt.id().clone(),
    };
    if retained.as_ref().is_none_or(|session| session.key != key) {
        *retained = None;
        let limits = preview_limits();
        limits
            .decode
            .validate()
            .map_err(|error| error.to_string())?;
        if registered.receipt.snapshot().content().byte_length() > limits.decode.max_input_bytes {
            return Err("Source original exceeds the native preview byte limit.".into());
        }
        let mut snapshot = media
            .originals()
            .snapshot_original(
                &registered.original,
                OriginalMediaLimits::default(),
                cancelled,
            )
            .map_err(|error| error.to_string())?;
        // Progressive admission: every served picture matches the receipt's
        // measured index (checked against `expected` above), and a background
        // decoder completes the fresh full measurement. The session's index
        // and stream metadata are the receipt's, compared at open.
        let source = SourceSession::open_admitted(
            &mut snapshot,
            Arc::new({
                let index = video
                    .index()
                    .for_asset(registered.asset.clone())
                    .map_err(|error| error.to_string())?;
                #[cfg(test)]
                let index = tests_support::adjust_index(index);
                index
            }),
            video.interpretation(),
            limits,
            cancelled,
        )
        .map_err(|error| error.to_string())?;
        check_picture_limits(&source)?;
        *retained = Some(RetainedSession {
            key,
            source,
            catalog: Some(Arc::clone(registered)),
        });
    }
    let session = retained
        .as_mut()
        .ok_or("The source session could not be retained.")?;
    session.catalog = Some(Arc::clone(registered));
    let summary = Some(source_summary(&session.source));
    let decoded =
        session
            .source
            .frame(id, FRAME_TIMEOUT, cancelled)
            .map_err(|error| match error {
                SourceSessionError::MeasurementInterrupted(_) => {
                    Served::Interrupted(error.to_string())
                }
                SourceSessionError::MeasurementMismatch(_) => {
                    Served::Failed(format!("Source verification failed: {error}"))
                }
                _ => Served::Failed(error.to_string()),
            })?;
    media.check_slice_live(cancelled)?;
    Ok(Picture {
        summary,
        id,
        frame: Some(render_frame(decoded, session.source.info())?),
        canvas,
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
        captions: Vec::new(),
        tier: PictureTier::Original,
    })
}

fn same_index_mapping(
    left: &SourceFrameIndex,
    right: &SourceFrameIndex,
    cancelled: impl FnMut() -> bool,
) -> Result<bool, String> {
    deadpan_cli::picture::same_index_mapping(left, right, cancelled).map_err(|error| match error {
        deadpan_cli::picture::ProjectPictureError::Cancelled => {
            "Project preview was cancelled.".into()
        }
        _ => error.to_string(),
    })
}

fn source_summary(source: &SourceSession) -> SourceSummary {
    let index = source.index().index();
    SourceSummary {
        info: source.info().clone(),
        frame_count: index.frames().len() as u64,
        first_pts: index.frames()[0].pts,
        terminal_pts: index.terminal_end(),
    }
}

fn check_picture_limits(source: &SourceSession) -> Result<(), String> {
    if u64::from(source.info().width) * u64::from(source.info().height) > deadpan_render::MAX_PIXELS
    {
        return Err("Source picture exceeds the 16-megapixel preview limit.".into());
    }
    Ok(())
}

fn open_source(path: &PathBuf, cancelled: &AtomicBool) -> Result<SourceSession, String> {
    let limits = SourceSessionLimits::interactive();
    let started = Instant::now();
    // Nonblocking open prevents a FIFO path from trapping this service before
    // the descriptor's regular-file check. Reads of regular files are unchanged.
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| format!("Could not open source: {error}"))?;
    let mut input = File::from(descriptor);
    let metadata = input.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limits.decode.max_input_bytes
    {
        return Err(format!(
            "Choose a nonempty regular file no larger than {} GiB.",
            limits.decode.max_input_bytes / (1024 * 1024 * 1024)
        ));
    }
    let mut remaining = metadata.len();
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        check_hash_control(started, cancelled)?;
        let capacity = remaining.min(buffer.len() as u64) as usize;
        let count = input
            .read(&mut buffer[..capacity])
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err(
                "Source length changed while reading it. Open it again when writing finishes."
                    .into(),
            );
        }
        digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    check_hash_control(started, cancelled)?;
    if input
        .read(&mut buffer[..1])
        .map_err(|error| error.to_string())?
        != 0
    {
        return Err(
            "Source length changed while reading it. Open it again when writing finishes.".into(),
        );
    }
    let identity = SourceContentIdentity::new(digest.finalize().into(), metadata.len())
        .map_err(|error| error.to_string())?;
    input.rewind().map_err(|error| error.to_string())?;
    SourceSession::open_verified(
        &mut input,
        identity,
        AssetId::new("source-preview").map_err(|error| error.to_string())?,
        limits,
        cancelled,
    )
    .map_err(|error| error.to_string())
}

fn check_hash_control(started: Instant, cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Source opening was cancelled.".into());
    }
    if started.elapsed() >= HASH_TIMEOUT {
        return Err("Source hashing exceeded its five-minute time limit.".into());
    }
    Ok(())
}

fn render_frame(decoded: DecodedRgbaFrame, info: &SourceStreamInfo) -> Result<Rgba8Frame, String> {
    deadpan_cli::picture::source_to_render_frame(decoded, info).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn await_reply(worker: &PreviewWorker) -> Reply {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(reply) = worker.take_reply() {
                return reply;
            }
            assert!(
                Instant::now() < deadline,
                "preview worker response deadline"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn actual_preview_worker_opens_then_coalesces_source_frame_requests() {
        let worker = PreviewWorker::new(egui::Context::default()).unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4");
        worker.submit(ticket(1, 1), Work::Open(path));
        let first = await_reply(&worker).picture.unwrap();
        assert_eq!(first.summary.as_ref().unwrap().frame_count, 120);
        assert_eq!(first.id, SourceFrameId(0));
        let first_frame = first.frame.as_ref().unwrap();
        assert_eq!(first_frame.metadata().pts.ticks, 0);
        assert_eq!(first_frame.metadata().color.transfer, Transfer::Rec709);
        assert_eq!(first_frame.metadata().color.primaries, Primaries::Rec709);
        for request in 2..=20 {
            worker.submit(ticket(1, request), Work::Frame(SourceFrameId(request)));
        }
        let last = await_reply(&worker);
        assert_eq!(last.ticket, ticket(1, 20));
        let last = last.picture.unwrap();
        assert!(last.summary.is_none());
        assert_eq!(last.id, SourceFrameId(20));
        let last_frame = last.frame.as_ref().unwrap();
        assert_eq!(last_frame.metadata().pts.ticks, 20 * 1001);
        assert_ne!(first_frame.bytes(), last_frame.bytes());
        worker.clear();
        worker.submit(ticket(1, 21), Work::Frame(SourceFrameId(21)));
        assert!(await_reply(&worker).picture.is_err());
        worker.shutdown();
    }

    #[test]
    fn nonregular_input_is_rejected_before_hashing() {
        assert!(open_source(&std::env::temp_dir(), &AtomicBool::new(false)).is_err());
    }

    fn ticket(source: u64, request: u64) -> Ticket {
        Ticket {
            source,
            request,
            transport: None,
        }
    }

    #[test]
    fn rapid_navigation_keeps_only_latest_work_and_cancels_inflight_decode() {
        let mut mailbox = Mailbox::default();
        mailbox.submit(ticket(1, 1), Work::Frame(SourceFrameId(2)));
        let active = mailbox.start_next().unwrap();
        for id in 2..100 {
            mailbox.submit(ticket(1, id), Work::Frame(SourceFrameId(id)));
        }
        assert!(active.cancelled.load(Ordering::Acquire));
        assert!(!mailbox.publish(Reply {
            ticket: active.ticket,
            picture: Err("cancelled old decode".into()),
            #[cfg(feature = "ui-harness")]
            timing: None,
        }));
        assert!(mailbox.reply.is_none());
        let next = mailbox.start_next().unwrap();
        assert_eq!(next.ticket, ticket(1, 99));
        assert!(matches!(next.work, Work::Frame(SourceFrameId(99))));
        assert!(mailbox.pending.is_none());
    }

    #[test]
    fn opening_another_source_invalidates_even_completed_old_reply() {
        let mut mailbox = Mailbox::default();
        mailbox.submit(ticket(1, 1), Work::Frame(SourceFrameId(0)));
        mailbox.start_next().unwrap();
        assert!(mailbox.publish(Reply {
            ticket: ticket(1, 1),
            picture: Err("old source error".into()),
            #[cfg(feature = "ui-harness")]
            timing: None,
        }));
        mailbox.submit(ticket(2, 2), Work::Open(PathBuf::from("new.mp4")));
        assert!(mailbox.reply.is_none());
        assert!(!mailbox.publish(Reply {
            ticket: ticket(1, 1),
            picture: Err("late reply".into()),
            #[cfg(feature = "ui-harness")]
            timing: None,
        }));
    }

    #[test]
    fn shutdown_cancels_active_and_pending_and_prevents_publication() {
        let mut mailbox = Mailbox::default();
        mailbox.submit(ticket(1, 1), Work::Frame(SourceFrameId(0)));
        let active = mailbox.start_next().unwrap();
        mailbox.submit(ticket(1, 2), Work::Frame(SourceFrameId(1)));
        let pending = Arc::clone(&mailbox.pending.as_ref().unwrap().cancelled);
        mailbox.stop();
        assert!(active.cancelled.load(Ordering::Acquire));
        assert!(pending.load(Ordering::Acquire));
        assert!(mailbox.start_next().is_none());
        assert!(!mailbox.publish(Reply {
            ticket: ticket(1, 2),
            picture: Err("late".into()),
            #[cfg(feature = "ui-harness")]
            timing: None,
        }));
        assert!(!mailbox.submit(ticket(2, 3), Work::Open(PathBuf::from("ignored"))));
    }
}
