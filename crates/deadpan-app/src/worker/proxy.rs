//! Preview proxy pictures for the main viewer's stopped seeks.
//!
//! This private module is the only proxy picture reader. The worker keeps at
//! most one proxy reader beside its Original session. A proxy picture stands
//! in for the exact Original picture with the same ordinal until the cursor
//! rests, when the worker refines it. The reader decodes the published,
//! read-only movie in place through a descriptor whose shared lock keeps
//! eviction away; the cache hashes each file state once, so reopening is
//! cheap. The first opening of a new entry still hashes it, so a request
//! never waits for an opening: the worker serves the Original and opens the
//! proxy while otherwise idle. Any proxy failure falls back to the Original
//! silently: the proxy is a cache, and the Original is always the truth.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_cli::proxy::cache::{ProxyCache, ProxyStamp};
use deadpan_core::{AssetId, SourceFrameId, SourceQualificationId};
use deadpan_media::proxy::ProxySidecar;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_render::Rgba8Frame;
use deadpan_source::{DecodedRgbaFrame, SourceStreamInfo};

use super::slice_view::PictureMedia;
use super::{FRAME_TIMEOUT, Picture, SourceSummary};
use crate::project::RegisteredSource;

/// Which pixels a picture shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureTier {
    /// The exact decoded Original picture.
    Original,
    /// The reduced preview proxy of that Original picture.
    Proxy,
}

/// Codec threads of the proxy's serving decoder. Every proxy picture is
/// intra-coded, so a seek decodes one picture; frame threading would only
/// add pipeline delay.
pub const PROXY_SERVING_THREADS: u32 = 1;

fn proxy_limits() -> SourceSessionLimits {
    let mut limits = SourceSessionLimits::interactive();
    limits.decode.threads = PROXY_SERVING_THREADS;
    limits
}

/// A decoded proxy picture. Deliberately not a `DecodedRgbaFrame` outside
/// this module: its only conversion is to the preview upload format, here.
struct PreviewFrame(DecodedRgbaFrame);

impl PreviewFrame {
    fn into_preview_upload(self, info: &SourceStreamInfo) -> Result<Rgba8Frame, String> {
        super::render_frame(self.0, info)
    }
}

/// An open proxy: the published movie's descriptor (shared-locked against
/// eviction) and a decoder over it, checked picture by picture against the
/// sidecar index, itself checked against the Original's receipt.
struct ProxyReader {
    session: SourceSession,
    _sidecar: Arc<ProxySidecar>,
}

impl ProxyReader {
    fn open(
        cache: &ProxyCache,
        registered: &RegisteredSource,
        cancelled: &AtomicBool,
    ) -> Result<Option<Self>, String> {
        let video = registered
            .receipt
            .snapshot()
            .video()
            .ok_or("no picture stream")?;
        let Some((file, sidecar)) =
            deadpan_cli::proxy::open_proxy_file(cache, &registered.original, video, cancelled)
                .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let input = VerifiedSourceInput::from_verified_file(file, sidecar.content())
            .map_err(|error| error.to_string())?;
        let session = SourceSession::open_input_indexed(
            input,
            Arc::new(sidecar.index.clone()),
            &sidecar.info,
            proxy_limits(),
            cancelled,
        )
        .map_err(|error| error.to_string())?;
        Ok(Some(Self {
            session,
            _sidecar: sidecar,
        }))
    }

    fn frame(
        &mut self,
        id: SourceFrameId,
        cancelled: &AtomicBool,
    ) -> Result<(PreviewFrame, SourceStreamInfo), String> {
        let frame = self
            .session
            .frame(id, FRAME_TIMEOUT, cancelled)
            .map_err(|error| error.to_string())?;
        Ok((PreviewFrame(frame), self.session.info().clone()))
    }
}

#[derive(PartialEq, Eq)]
struct SlotKey {
    session: u64,
    asset: AssetId,
    receipt: SourceQualificationId,
}

/// The retained proxy state of the main viewer's worker.
#[derive(Default)]
pub(super) struct ProxySlot {
    cache: Option<ProxyCache>,
    key: Option<SlotKey>,
    reader: Option<ProxyReader>,
    /// The entry stamp at which no usable proxy was found; looked up again
    /// only after the entry is published, replaced or removed.
    absent_at: Option<ProxyStamp>,
    /// The Original a recent request wanted a proxy for, to open when idle.
    wanted: Option<Wanted>,
}

struct Wanted {
    registered: Arc<RegisteredSource>,
    stamp: ProxyStamp,
}

impl ProxySlot {
    /// Use `cache` from now on; a different cache drops the reader.
    pub(super) fn set_cache(&mut self, cache: Option<ProxyCache>) {
        let same = match (&self.cache, &cache) {
            (Some(old), Some(new)) => old.path() == new.path(),
            (None, None) => true,
            _ => false,
        };
        if !same {
            *self = Self {
                cache,
                ..Self::default()
            };
        }
    }

    /// The proxy picture for Original frame `id`, or None when no verified
    /// proxy is open, in which case the caller decodes the Original.
    pub(super) fn picture(
        &mut self,
        media: PictureMedia<'_>,
        registered: &Arc<RegisteredSource>,
        id: SourceFrameId,
        canvas: Option<(u32, u32)>,
        cancelled: &AtomicBool,
    ) -> Result<Option<Picture>, String> {
        let PictureMedia::Committed(workspace) = media else {
            return Ok(None);
        };
        let Some(cache) = self.cache.clone() else {
            return Ok(None);
        };
        let Some(video) = registered.receipt.snapshot().video() else {
            return Ok(None);
        };
        let key = SlotKey {
            session: workspace.session,
            asset: registered.asset.clone(),
            receipt: registered.receipt.id().clone(),
        };
        if self.key.as_ref() != Some(&key) {
            *self = Self {
                cache: Some(cache.clone()),
                key: Some(key),
                ..Self::default()
            };
        }
        let Ok(proxy_key) = deadpan_cli::proxy::proxy_key(&registered.original, video) else {
            return Ok(None);
        };
        let Some(reader) = self.reader.as_mut() else {
            let stamp = cache.stamp(&proxy_key);
            if self.absent_at != Some(stamp) {
                self.wanted = Some(Wanted {
                    registered: Arc::clone(registered),
                    stamp,
                });
            }
            return Ok(None);
        };
        match reader.frame(id, cancelled) {
            Ok((frame, info)) => {
                let frame = frame.into_preview_upload(&info)?;
                let index = video.index().index();
                Ok(Some(Picture {
                    summary: Some(SourceSummary {
                        info: video.interpretation().clone(),
                        frame_count: index.frames().len() as u64,
                        first_pts: index.frames()[0].pts,
                        terminal_pts: index.terminal_end(),
                    }),
                    id,
                    frame: Some(frame),
                    canvas,
                    framing: Vec::new(),
                    framing_gap: false,
                    follow_point: None,
                    picture_context: None,
                    captions: Vec::new(),
                    tier: PictureTier::Proxy,
                }))
            }
            Err(_) if cancelled.load(Ordering::Acquire) => {
                Err("Project preview was cancelled.".into())
            }
            Err(_) => {
                // A proxy that fails a picture check is dropped until its
                // cache entry changes.
                self.reader = None;
                self.absent_at = Some(cache.stamp(&proxy_key));
                Ok(None)
            }
        }
    }

    /// Whether a verified proxy reader is open. It may still belong to
    /// another Original; [`Self::picture`] checks that.
    pub(super) fn is_open(&self) -> bool {
        self.reader.is_some()
    }

    /// Whether a recent request wants a proxy that is not open yet.
    pub(super) fn wants_preparation(&self) -> bool {
        self.reader.is_none() && self.wanted.is_some() && self.cache.is_some()
    }

    /// Open the wanted proxy while the worker is idle. `cancelled` is set by
    /// the next submitted request; an interrupted opening is retried later.
    pub(super) fn prepare(&mut self, cancelled: &AtomicBool) {
        let Some(wanted) = self.wanted.take() else {
            return;
        };
        let Some(cache) = self.cache.clone() else {
            return;
        };
        match ProxyReader::open(&cache, &wanted.registered, cancelled) {
            Ok(Some(reader)) => self.reader = Some(reader),
            Err(_) if cancelled.load(Ordering::Acquire) => self.wanted = Some(wanted),
            // Absent, damaged or unverifiable: preview the Original until the
            // entry changes.
            Ok(None) | Err(_) => self.absent_at = Some(wanted.stamp),
        }
    }
}
