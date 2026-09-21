//! One background source session with replaceable, bounded request/result slots.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, SourceFrameId, SourceTimeBase, SourceTimestamp};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_render::{
    FrameMetadata, Primaries, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};
use deadpan_source::{ColorPrimaries, ColorTransfer, DecodedRgbaFrame, SourceStreamInfo};
use eframe::egui;
use sha2::{Digest, Sha256};

const HASH_TIMEOUT: Duration = Duration::from_secs(300);
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub source: u64,
    pub request: u64,
}

pub enum Work {
    Open(PathBuf),
    Frame(SourceFrameId),
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
    pub frame: Rgba8Frame,
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
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(shared: Arc<Shared>, context: egui::Context) {
    let mut session: Option<(u64, SourceSession)> = None;
    loop {
        let request = {
            let mut mailbox = shared.mailbox.lock().expect("preview mailbox");
            loop {
                if mailbox.shutdown {
                    return;
                }
                if let Some(request) = mailbox.start_next() {
                    break request;
                }
                mailbox = shared.changed.wait(mailbox).expect("preview mailbox");
            }
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

fn perform(
    request: &Request,
    retained: &mut Option<(u64, SourceSession)>,
) -> Result<Picture, String> {
    let (summary, id) = match &request.work {
        Work::Open(path) => {
            *retained = None;
            let source = open_source(path, &request.cancelled)?;
            if u64::from(source.info().width) * u64::from(source.info().height)
                > deadpan_render::MAX_PIXELS
            {
                return Err("Source picture exceeds the 16-megapixel preview limit.".into());
            }
            let index = source.index().index();
            let summary = SourceSummary {
                info: source.info().clone(),
                frame_count: index.frames().len() as u64,
                first_pts: index.frames()[0].pts,
                terminal_pts: index.terminal_end(),
            };
            *retained = Some((request.ticket.source, source));
            (Some(summary), SourceFrameId(0))
        }
        Work::Frame(id) => (None, *id),
    };
    let (_, source) = retained
        .as_mut()
        .filter(|(identity, _)| *identity == request.ticket.source)
        .ok_or("The requested source session is no longer open.")?;
    let decoded = source
        .frame(id, FRAME_TIMEOUT, &request.cancelled)
        .map_err(|error| error.to_string())?;
    Ok(Picture {
        summary,
        id,
        frame: render_frame(decoded, source.info())?,
    })
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
        assert_eq!(first.frame.metadata().pts.ticks, 0);
        assert_eq!(first.frame.metadata().color.transfer, Transfer::Rec709);
        assert_eq!(first.frame.metadata().color.primaries, Primaries::Rec709);
        for request in 2..=20 {
            worker.submit(ticket(1, request), Work::Frame(SourceFrameId(request)));
        }
        let last = await_reply(&worker);
        assert_eq!(last.ticket, ticket(1, 20));
        let last = last.picture.unwrap();
        assert!(last.summary.is_none());
        assert_eq!(last.id, SourceFrameId(20));
        assert_eq!(last.frame.metadata().pts.ticks, 20 * 1001);
        assert_ne!(first.frame.bytes(), last.frame.bytes());
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
