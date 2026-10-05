//! Fixed-revision picture preparation for sequential rendering workers.
//!
//! This host boundary opens a read-only store and verified media snapshots.
//! It creates no GPU, thread, process, encoder or output file. Call it off the UI
//! and audio threads. Final export still needs isolated worker supervision,
//! audio/mux qualification, verification and atomic publication.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{
    AssetId, CapturedFraming, ColorPolicy, FrameRange, FrameRate, GeneratedArtifact,
    IndexedSourceFrame, IterationId, ProjectDocument, ProjectFrame, ProjectId, RevisionId,
    SourceFrameId, SourceQualificationId,
};
use deadpan_media::source_session::{SourceSession, SourceSessionError, SourceSessionLimits};
use deadpan_plan::{Picture, PictureFraming, PlanError, RenderPlan};
use deadpan_render::{FramingLayer, RenderError, Rgba8Frame};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::{AccessMode, ProjectStore, StoreError};

mod shared;
pub use shared::{
    aspect_region, fill_canvas_aspect, render_layers, same_index_mapping, source_to_render_frame,
};
mod generated;
pub use generated::open_generated_picture;
#[cfg(test)]
mod tests;

const FRAME_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum ProjectPictureError {
    #[error("Picture preparation was cancelled.")]
    Cancelled,
    #[error("picture preparation requires a nonempty range within the committed duration")]
    Range,
    #[error("project frame {frame:?} is outside the captured picture range {range:?}")]
    FrameOutOfRange {
        frame: ProjectFrame,
        range: FrameRange,
    },
    #[error("HDR picture preparation requires a qualified tone-mapping/output path")]
    HdrUnsupported,
    #[error("still-image picture preparation is not qualified for asset {0}")]
    StillUnsupported(AssetId),
    #[error("legacy accepted picture lacks qualified generated evidence for asset {0}")]
    AcceptedUnsupported(AssetId),
    #[error("accepted picture evidence disagrees: {0}")]
    GeneratedEvidence(&'static str),
    #[error("Picture preparation exceeded its deadline.")]
    Deadline,
    #[error(transparent)]
    GeneratedObject(#[from] deadpan_store::generated_media::GeneratedMediaError),
    #[error(transparent)]
    Provenance(#[from] deadpan_models::QualificationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("source {asset} evidence disagrees with the captured revision: {reason}")]
    SourceEvidence {
        asset: AssetId,
        reason: &'static str,
    },
    #[error("picture preparation exceeds its resource limit: {0}")]
    Limits(&'static str),
    #[error("decoded picture dimensions disagree with the qualified source interpretation")]
    DecodedDimensions,
    #[error("Source orientation is unsupported.")]
    Orientation,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Source(#[from] SourceSessionError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
}

/// The background is an explicit opaque black canvas, not a fabricated source
/// frame. A decoded frame retains the original source ordinal and signed PTS.
#[derive(Debug)]
pub enum PreparedPicture {
    Frame {
        asset: AssetId,
        qualification: SourceQualificationId,
        id: SourceFrameId,
        frame: Rgba8Frame,
    },
    Generated {
        artifact: Arc<GeneratedArtifact>,
        id: SourceFrameId,
        frame: Rgba8Frame,
    },
    Background,
}

/// Immutable result identity. Output timing is project_frame/frame_rate; the
/// source PTS in a Frame's metadata is never substituted for that output clock.
/// The host bounds completed-result retention and rejects superseded job IDs.
/// Canvas is the unchanged committed geometry, including legitimate odd sizes.
/// A later automatic output policy must qualify any codec-even normalization.
#[derive(Debug)]
pub struct PreparedProjectPicture {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub project_frame: ProjectFrame,
    pub canvas: [u32; 2],
    pub frame_rate: FrameRate,
    pub picture: PreparedPicture,
    pub framing: Vec<PictureFraming>,
    pub picture_context: Option<Arc<CapturedFraming>>,
    pub gap_after: Option<IterationId>,
    /// Caption lines drawn over the composed picture.
    pub captions: Vec<deadpan_plan::PictureCaption>,
}

impl PreparedProjectPicture {
    pub fn render_layers(&self) -> Result<Vec<FramingLayer>, ProjectPictureError> {
        render_layers(&self.framing, self.gap_after.is_some())
    }

    /// The caption coverage for a `target` raster showing this canvas, or
    /// `None` when the frame shows no caption.
    pub fn caption_overlay(
        &self,
        target: [u32; 2],
    ) -> Result<Option<deadpan_render::CaptionOverlay>, deadpan_render::RenderError> {
        caption_overlay(&self.captions, self.canvas, target)
    }
}

/// Shared by preview and export: the same lines, canvas and target raster
/// give the same coverage.
pub fn caption_overlay(
    captions: &[deadpan_plan::PictureCaption],
    canvas: [u32; 2],
    target: [u32; 2],
) -> Result<Option<deadpan_render::CaptionOverlay>, deadpan_render::RenderError> {
    let lines: Vec<_> = captions
        .iter()
        .map(|caption| deadpan_render::CaptionLine {
            text: caption.text.clone(),
            placement: caption.placement,
        })
        .collect();
    deadpan_render::CaptionOverlay::rasterize(&lines, canvas, target)
}

/// The last caption raster, reused while the lines, canvas and target stay
/// the same: a caption usually spans many frames, so it is drawn once per
/// change rather than once per frame. Results are identical either way.
#[derive(Default)]
pub struct CaptionMemo {
    key: Option<(Vec<deadpan_plan::PictureCaption>, [u32; 2], [u32; 2])>,
    overlay: Option<deadpan_render::CaptionOverlay>,
}

impl CaptionMemo {
    pub fn overlay(
        &mut self,
        captions: &[deadpan_plan::PictureCaption],
        canvas: [u32; 2],
        target: [u32; 2],
    ) -> Result<Option<&deadpan_render::CaptionOverlay>, deadpan_render::RenderError> {
        if captions.is_empty() {
            return Ok(None);
        }
        let current = self
            .key
            .as_ref()
            .is_some_and(|(lines, old_canvas, old_target)| {
                lines.as_slice() == captions && *old_canvas == canvas && *old_target == target
            });
        if !current {
            self.overlay = caption_overlay(captions, canvas, target)?;
            self.key = Some((captions.to_vec(), canvas, target));
        }
        Ok(self.overlay.as_ref())
    }
}

struct RetainedSource {
    asset: AssetId,
    origin: RetainedOrigin,
    source: SourceSession,
}

#[derive(PartialEq, Eq)]
enum RetainedOrigin {
    Original(SourceQualificationId),
    Generated(Arc<GeneratedArtifact>),
}

/// Session-local picture counters for diagnostics and benchmarks. They are
/// observations of this session only, never authored or persisted state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PictureSessionStats {
    /// Cold decoder admissions: verified snapshot, measured index and open.
    pub source_opens: u64,
    /// Total wall time of those admissions, in microseconds.
    pub source_open_us: u64,
    /// Requests served by the already retained decoder.
    pub source_reuses: u64,
    /// Decoded source/generated pictures returned.
    pub decoded_frames: u64,
    /// Authored Background/Blank pictures returned without decoding.
    pub background_frames: u64,
}

/// One committed revision and one retained decoder/index/private input. Changing
/// sources releases the previous session before admitting another; this is a
/// bounded single-source cache, not a promise to retain every source offline.
/// Opening and decoding use the media layer's cooperative byte/frame/deadline
/// limits. Store validation is synchronous and cannot be interrupted internally.
/// No writer-session capability or current HEAD lookup is used after capture.
pub struct ProjectPictureSession {
    store: ProjectStore,
    document: ProjectDocument,
    plan: RenderPlan,
    range: FrameRange,
    generated: deadpan_store::generated_media::GeneratedReadHandle,
    retained: Option<RetainedSource>,
    stats: PictureSessionStats,
}

impl ProjectPictureSession {
    /// None chooses the complete committed interval. Empty projects and empty,
    /// negative or out-of-duration ranges fail rather than inventing a frame.
    pub fn open_revision(
        path: &Path,
        revision: &RevisionId,
        range: Option<FrameRange>,
        cancelled: &AtomicBool,
    ) -> Result<Self, ProjectPictureError> {
        check_cancel(cancelled)?;
        let store = ProjectStore::open(path, AccessMode::ReadOnly)?;
        check_cancel(cancelled)?;
        let document = store.snapshot_at(revision)?;
        check_cancel(cancelled)?;
        let basis = document.presentation_basis();
        if basis.color_policy != ColorPolicy::SdrRec709 {
            return Err(ProjectPictureError::HdrUnsupported);
        }
        validate_raster(basis.width, basis.height)?;
        let plan = RenderPlan::compile(&document)?;
        let range = range.unwrap_or(FrameRange::new(
            ProjectFrame(0),
            ProjectFrame(plan.duration().frames()),
        )?);
        if range.start().0 < 0
            || range.end().0 > plan.duration().frames()
            || range.duration().frames() == 0
        {
            return Err(ProjectPictureError::Range);
        }
        check_cancel(cancelled)?;
        let generated = store.generated_read_handle();
        Ok(Self {
            store,
            document,
            plan,
            range,
            generated,
            retained: None,
            stats: PictureSessionStats::default(),
        })
    }

    pub fn plan(&self) -> &RenderPlan {
        &self.plan
    }
    pub fn revision(&self) -> &RevisionId {
        self.document.revision_id()
    }
    pub fn project_id(&self) -> &ProjectId {
        self.document.project_id()
    }
    pub fn basis(&self) -> &deadpan_core::PresentationBasis {
        self.document.presentation_basis()
    }
    pub(crate) fn document(&self) -> &ProjectDocument {
        &self.document
    }
    pub const fn range(&self) -> FrameRange {
        self.range
    }
    pub const fn stats(&self) -> PictureSessionStats {
        self.stats
    }

    /// Prepare one exact project frame. Structural repeats remain compact and
    /// all source lookup uses the compiled plan's endpoint and retime semantics.
    /// Unsupported providers fail explicitly when requested, even if other
    /// frames of the selected interval have already prepared successfully.
    pub fn prepare(
        &mut self,
        frame: ProjectFrame,
        cancelled: &AtomicBool,
    ) -> Result<PreparedProjectPicture, ProjectPictureError> {
        check_cancel(cancelled)?;
        if !self.range.contains(frame) {
            return Err(ProjectPictureError::FrameOutOfRange {
                frame,
                range: self.range,
            });
        }
        let sample = self.plan.picture(frame)?;
        let picture =
            match &sample.picture {
                Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
                    let retained = self.source(asset, cancelled)?;
                    let id = sample
                        .picture
                        .select_source_frame(retained.source.index().index())?
                        .identity;
                    let decoded = retained.source.frame(id, FRAME_TIMEOUT, cancelled)?;
                    let frame = source_to_render_frame(decoded, retained.source.info())?;
                    let qualification = match &retained.origin {
                        RetainedOrigin::Original(qualification) => qualification.clone(),
                        RetainedOrigin::Generated(_) => unreachable!("Original receipt admitted"),
                    };
                    self.stats.decoded_frames += 1;
                    PreparedPicture::Frame {
                        asset: asset.clone(),
                        qualification,
                        id,
                        frame,
                    }
                }
                Picture::Blank | Picture::Background => {
                    self.stats.background_frames += 1;
                    PreparedPicture::Background
                }
                Picture::Still { asset } => {
                    return Err(ProjectPictureError::StillUnsupported(asset.clone()));
                }
                Picture::Accepted {
                    asset,
                    generated: Some(artifact),
                    ..
                } => {
                    self.generated.check_live(cancelled)?;
                    if asset != &artifact.sampled_asset {
                        return Err(ProjectPictureError::GeneratedEvidence(
                            "plan asset differs from its artifact",
                        ));
                    }
                    let origin = RetainedOrigin::Generated(artifact.clone());
                    if self.retained.as_ref().is_none_or(|retained| {
                        retained.asset != *asset || retained.origin != origin
                    }) {
                        self.retained = None;
                        let opened = std::time::Instant::now();
                        let source = open_generated_picture(
                            &self.generated,
                            &self.document,
                            artifact,
                            cancelled,
                        )?;
                        self.stats.source_opens += 1;
                        self.stats.source_open_us += elapsed_us(opened);
                        self.retained = Some(RetainedSource {
                            asset: asset.clone(),
                            origin,
                            source,
                        });
                    } else {
                        self.stats.source_reuses += 1;
                    }
                    let retained = self.retained.as_mut().expect("generated source admitted");
                    let id = sample
                        .picture
                        .select_source_frame(retained.source.index().index())?
                        .identity;
                    let decoded = retained.source.frame(id, FRAME_TIMEOUT, cancelled)?;
                    let mut frame = source_to_render_frame(decoded, retained.source.info())?;
                    self.stats.decoded_frames += 1;
                    if let Some(aspect) = artifact.content_aspect {
                        frame = shared::fill_canvas_aspect(frame, aspect)?;
                    }
                    self.generated.check_live(cancelled)?;
                    PreparedPicture::Generated {
                        artifact: artifact.clone(),
                        id,
                        frame,
                    }
                }
                Picture::Accepted {
                    asset,
                    generated: None,
                    ..
                } => {
                    return Err(ProjectPictureError::AcceptedUnsupported(asset.clone()));
                }
            };
        check_cancel(cancelled)?;
        let basis = self.document.presentation_basis();
        Ok(PreparedProjectPicture {
            project_id: sample.project_id,
            revision_id: sample.revision_id,
            project_frame: frame,
            canvas: [basis.width, basis.height],
            frame_rate: basis.frame_rate,
            picture,
            framing: sample.framing,
            picture_context: sample.picture_context,
            gap_after: sample.gap_after,
            captions: sample.captions,
        })
    }

    fn source(
        &mut self,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&mut RetainedSource, ProjectPictureError> {
        check_cancel(cancelled)?;
        let evidence = |reason| ProjectPictureError::SourceEvidence {
            asset: asset.clone(),
            reason,
        };
        let authored = self
            .document
            .assets()
            .get(asset)
            .ok_or_else(|| evidence("asset is absent"))?;
        let qualification = authored
            .source_qualification
            .as_ref()
            .ok_or_else(|| evidence("measured source qualification is absent"))?;
        if self
            .retained
            .as_ref()
            .is_some_and(|cached| &cached.asset == asset)
        {
            if self.retained.as_ref().is_none_or(|cached| {
                cached.origin != RetainedOrigin::Original(qualification.clone())
            }) {
                return Err(evidence("retained decoder qualification differs"));
            }
            self.stats.source_reuses += 1;
            return Ok(self.retained.as_mut().expect("matching retained source"));
        }
        let opened = std::time::Instant::now();
        // Bound simultaneous complete source snapshots and indexes, including a
        // failed cold admission. Previously returned owned RGBA frames remain valid.
        self.retained = None;
        let receipt = self
            .store
            .registered_source(self.document.revision_id(), asset)?;
        if receipt.id() != qualification
            || authored.content_hash != receipt.original().content().to_string()
        {
            return Err(evidence("receipt identity or content differs"));
        }
        let expected = receipt
            .snapshot()
            .video()
            .ok_or_else(|| evidence("qualified video stream is absent"))?;
        let limits = SourceSessionLimits::default();
        validate_raster(
            expected.interpretation().width,
            expected.interpretation().height,
        )?;
        if expected.index().content().byte_length() > limits.decode.max_input_bytes
            || expected.index().index().frames().len() > limits.maximum_index_frames
            || expected.index().index().frames().len()
                > limits.maximum_index_bytes / std::mem::size_of::<IndexedSourceFrame>()
        {
            return Err(ProjectPictureError::Limits(
                "qualified source exceeds decoder byte/index budget",
            ));
        }
        let original_limits =
            OriginalMediaLimits::new(limits.decode.max_input_bytes, limits.opening_timeout)
                .map_err(StoreError::from)?;
        let mut snapshot = self.store.snapshot_original(
            receipt.original().content(),
            original_limits,
            cancelled,
        )?;
        if snapshot.record().object() != receipt.original()
            || snapshot.record().sha256() != expected.index().content().sha256()
            || snapshot.record().object().byte_length() != expected.index().content().byte_length()
        {
            return Err(evidence("verified original differs from receipt"));
        }
        check_cancel(cancelled)?;
        let source = SourceSession::open_verified(
            &mut snapshot,
            expected.index().content(),
            asset.clone(),
            limits,
            cancelled,
        )?;
        if source.index().content() != expected.index().content()
            || source.index().stream_index() != expected.index().stream_index()
            || source.info() != expected.interpretation()
            || source.index().index().asset() != asset
            || !same_index_mapping(source.index().index(), expected.index().index(), || {
                cancelled.load(Ordering::Acquire)
            })?
        {
            return Err(evidence(
                "decoded stream, interpretation or measured index differs",
            ));
        }
        check_cancel(cancelled)?;
        self.retained = Some(RetainedSource {
            asset: asset.clone(),
            origin: RetainedOrigin::Original(qualification.clone()),
            source,
        });
        self.stats.source_opens += 1;
        self.stats.source_open_us += elapsed_us(opened);
        Ok(self.retained.as_mut().expect("source admitted"))
    }
}

fn elapsed_us(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), ProjectPictureError> {
    if cancelled.load(Ordering::Acquire) {
        Err(ProjectPictureError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_raster(width: u32, height: u32) -> Result<(), ProjectPictureError> {
    if width == 0
        || height == 0
        || width > deadpan_render::MAX_DIMENSION
        || height > deadpan_render::MAX_DIMENSION
        || u64::from(width) * u64::from(height) > deadpan_render::MAX_PIXELS
    {
        return Err(ProjectPictureError::Limits(
            "picture raster exceeds the shared renderer bounds",
        ));
    }
    Ok(())
}
