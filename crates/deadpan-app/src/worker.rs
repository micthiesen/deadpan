//! One background source session with replaceable, bounded request/result slots.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use deadpan_core::{
    AssetId, ProjectFrame, SourceFrameId, SourceFrameIndex, SourceQualificationId, SourceTimeBase,
    SourceTimestamp,
};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_render::{
    FrameMetadata, Primaries, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};
use deadpan_source::{ColorPrimaries, ColorTransfer, DecodedRgbaFrame, SourceStreamInfo};
use deadpan_store::original_media::OriginalMediaLimits;
use eframe::egui;
use sha2::{Digest, Sha256};

use crate::project::{RegisteredSource, Workspace};

const HASH_TIMEOUT: Duration = Duration::from_secs(300);
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(test)]
mod project_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
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
}

pub struct Reply {
    pub ticket: Ticket,
    pub picture: Result<Picture, String>,
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
        self.reply = Some(reply);
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
        self.cancel_active();
        self.latest = None;
        self.pending = None;
        self.reply = None;
        self.clear_requested = true;
    }
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    changed: Condvar,
}

pub struct PreviewWorker {
    shared: Arc<Shared>,
}

impl PreviewWorker {
    pub fn new(context: egui::Context) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let background = Arc::clone(&shared);
        // The single thread owns all file I/O, hashing, indexing and decoding.
        // Dropping the handle deliberately avoids a blocking GUI shutdown join.
        std::thread::Builder::new()
            .name("deadpan-source-preview".into())
            .spawn(move || run(background, context))?;
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

fn run(shared: Arc<Shared>, context: egui::Context) {
    let mut session = None;
    loop {
        let request = {
            let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
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
                mailbox = shared.changed.wait(mailbox).expect("preview mailbox");
            }
        };
        let Some(request) = request else {
            // Native teardown and private snapshot deletion stay off the UI and
            // outside the mailbox lock used for submitting the next request.
            session = None;
            continue;
        };
        let picture = perform(&request, &mut session);
        let publish = shared
            .mailbox
            .lock()
            .expect("preview mailbox")
            .publish(Reply {
                ticket: request.ticket,
                picture,
            });
        if publish {
            context.request_repaint();
        }
    }
}

#[derive(PartialEq, Eq)]
enum SessionKey {
    Raw(u64),
    Project {
        session: u64,
        asset: AssetId,
        receipt: SourceQualificationId,
    },
}

struct RetainedSession {
    key: SessionKey,
    source: SourceSession,
    catalog: Option<Arc<RegisteredSource>>,
}

fn perform(request: &Request, retained: &mut Option<RetainedSession>) -> Result<Picture, String> {
    if let Work::Project { workspace, view } = &request.work {
        return project_picture(workspace, view, &request.cancelled, retained);
    }
    let (summary, id) = match &request.work {
        Work::Open(path) => {
            *retained = None;
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
        Work::Project { .. } => unreachable!("project requests handled above"),
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
    })
}

fn project_picture(
    workspace: &Workspace,
    view: &ProjectView,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Project preview was cancelled.".into());
    }
    if workspace.plan.metadata().project_id != *workspace.document.project_id()
        || workspace.plan.metadata().revision_id != *workspace.document.revision_id()
    {
        return Err("The picture plan belongs to another project revision.".into());
    }
    let (asset, frame, canvas) = match view {
        ProjectView::Source { asset, frame } => (asset, *frame, None),
        ProjectView::Sequence { frame } => {
            let basis = workspace.document.presentation_basis();
            let canvas = Some((basis.width, basis.height));
            if workspace.plan.duration().frames() == 0 && frame.0 == 0 {
                return Ok(background_picture(canvas));
            }
            let sample = workspace
                .plan
                .picture(*frame)
                .map_err(|error| error.to_string())?;
            let asset = match &sample.picture {
                deadpan_plan::Picture::Source { asset, .. }
                | deadpan_plan::Picture::Freeze { asset, .. } => asset,
                deadpan_plan::Picture::Blank | deadpan_plan::Picture::Background => {
                    return Ok(background_picture(canvas));
                }
                deadpan_plan::Picture::Still { .. } => {
                    return Err("Still-image preview is not yet qualified.".into());
                }
                deadpan_plan::Picture::Accepted { .. } => {
                    return Err("Accepted generated-media preview is not yet qualified.".into());
                }
            };
            let registered = registered_source(workspace, asset)?;
            let index = registered
                .video_index
                .as_ref()
                .ok_or("This source has no qualified picture index.")?;
            let frame = sample
                .picture
                .select_source_frame(index)
                .map_err(|error| error.to_string())?
                .identity;
            return registered_picture(workspace, registered, frame, canvas, cancelled, retained);
        }
    };
    let registered = registered_source(workspace, asset)?;
    registered_picture(workspace, registered, frame, canvas, cancelled, retained)
}

fn background_picture(canvas: Option<(u32, u32)>) -> Picture {
    Picture {
        summary: None,
        id: SourceFrameId(0),
        frame: None,
        canvas,
    }
}

fn registered_source<'a>(
    workspace: &'a Workspace,
    asset: &AssetId,
) -> Result<&'a Arc<RegisteredSource>, String> {
    let registered = workspace
        .sources
        .get(asset)
        .ok_or("This source has no registered media evidence.")?;
    let authored = workspace
        .document
        .assets()
        .get(asset)
        .ok_or("This source is absent from the selected project revision.")?;
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

fn registered_picture(
    workspace: &Workspace,
    registered: &Arc<RegisteredSource>,
    id: SourceFrameId,
    canvas: Option<(u32, u32)>,
    cancelled: &AtomicBool,
    retained: &mut Option<RetainedSession>,
) -> Result<Picture, String> {
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
        session: workspace.session,
        asset: registered.asset.clone(),
        receipt: registered.receipt.id().clone(),
    };
    if retained.as_ref().is_none_or(|session| session.key != key) {
        *retained = None;
        let limits = SourceSessionLimits::default();
        limits
            .decode
            .validate()
            .map_err(|error| error.to_string())?;
        if registered.receipt.snapshot().content().byte_length() > limits.decode.max_input_bytes {
            return Err("Source original exceeds the native preview byte limit.".into());
        }
        let mut snapshot = workspace
            .originals
            .snapshot_original(
                &registered.original,
                OriginalMediaLimits::default(),
                cancelled,
            )
            .map_err(|error| error.to_string())?;
        let source = SourceSession::open_verified(
            &mut snapshot,
            registered.receipt.snapshot().content(),
            registered.asset.clone(),
            limits,
            cancelled,
        )
        .map_err(|error| error.to_string())?;
        check_picture_limits(&source)?;
        if source.index().content() != video.index().content()
            || source.index().stream_index() != video.index().stream_index()
            || source.info() != video.interpretation()
            || source.index().index().asset() != expected.asset()
            || !same_index_mapping(source.index().index(), expected, || {
                cancelled.load(Ordering::Acquire)
            })?
        {
            return Err(
                "Decoded source disagrees with its immutable qualification receipt.".into(),
            );
        }
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
    let decoded = session
        .source
        .frame(id, FRAME_TIMEOUT, cancelled)
        .map_err(|error| error.to_string())?;
    Ok(Picture {
        summary,
        id,
        frame: Some(render_frame(decoded, session.source.info())?),
        canvas,
    })
}

fn same_index_mapping(
    left: &SourceFrameIndex,
    right: &SourceFrameIndex,
    mut cancelled: impl FnMut() -> bool,
) -> Result<bool, String> {
    if cancelled() {
        return Err("Project preview was cancelled.".into());
    }
    if left.time_base() != right.time_base()
        || left.frames().len() != right.frames().len()
        || left.terminal_end() != right.terminal_end()
        || left.terminal_provenance() != right.terminal_provenance()
    {
        return Ok(false);
    }
    for (left, right) in left.frames().chunks(1024).zip(right.frames().chunks(1024)) {
        if cancelled() {
            return Err("Project preview was cancelled.".into());
        }
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
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
    let limits = SourceSessionLimits::default();
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
    let metadata = FrameMetadata {
        width: decoded.width,
        height: decoded.height,
        row_stride_bytes: u32::try_from(decoded.row_stride_bytes)
            .map_err(|_| "Source row stride exceeds the preview limit.")?,
        sample_aspect_ratio: SampleAspectRatio::new(info.sample_aspect_num, info.sample_aspect_den)
            .map_err(|error| error.to_string())?,
        rotation: match info.rotation_quarter_turns {
            0 => Rotation::None,
            1 => Rotation::Clockwise90,
            2 => Rotation::Clockwise180,
            3 => Rotation::Clockwise270,
            _ => return Err("Source orientation is unsupported.".into()),
        },
        color: SourceColor {
            transfer: match info.color.transfer {
                ColorTransfer::Bt709 => Transfer::Rec709,
                ColorTransfer::Srgb => Transfer::Srgb,
                ColorTransfer::Linear => Transfer::Linear,
            },
            primaries: match info.color.primaries {
                ColorPrimaries::Bt709 => Primaries::Rec709,
                ColorPrimaries::Bt2020 => Primaries::Rec2020,
                ColorPrimaries::DisplayP3 => Primaries::DisplayP3D65,
            },
        },
        pts: SourceTimestamp {
            ticks: decoded.metadata.pts,
            time_base: SourceTimeBase::new(info.time_base_num, info.time_base_den)
                .map_err(|error| error.to_string())?,
        },
    };
    Rgba8Frame::new(metadata, decoded.rgba).map_err(|error| error.to_string())
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
        Ticket { source, request }
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
        }));
        mailbox.submit(ticket(2, 2), Work::Open(PathBuf::from("new.mp4")));
        assert!(mailbox.reply.is_none());
        assert!(!mailbox.publish(Reply {
            ticket: ticket(1, 1),
            picture: Err("late reply".into()),
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
            picture: Err("late".into())
        }));
        assert!(!mailbox.submit(ticket(2, 3), Work::Open(PathBuf::from("ignored"))));
    }
}
