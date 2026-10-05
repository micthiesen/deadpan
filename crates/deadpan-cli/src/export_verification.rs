//! Deterministic preview-versus-export content verification.
//!
//! For one committed revision, reference pictures come from the shared
//! committed picture path (`ProjectPictureSession` plus the shared Metal
//! pipeline, read back from the linear working target and converted at the
//! encoder pixel boundary of the revision's automatic output branch: SDR
//! Rec.709 I420, or ten-bit Rec.2100 PQ/HLG for an HDR branch, compared in
//! ten-bit signal code values), and reference audio comes from the limited
//! canonical bus that audition also uses. The emitted movie is decoded with the
//! qualified source decoders at the same output coordinates. The comparison is
//! a diagnostic report: it never admits, publishes or edits anything.
//!
//! Audio alignment is measured, never corrected. Decoded samples are placed by
//! their own PTS after the movie's declared AAC priming edit, and any nonzero
//! measured lag fails. See docs/PREVIEW_EXPORT_VERIFICATION.md.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use deadpan_core::{AudioSample, ColorPolicy, ContentLight, ProjectFrame, RevisionId};
use deadpan_render::FrameLight;
use deadpan_source::Mp4TrackKind;
use deadpan_store::{AccessMode, ProjectStore};
use serde::Serialize;

use crate::{
    audio::OfflineAudioSession,
    encoded_render::worker::ContentLightAccumulator,
    export_picture::{
        ExportPictureContract, ExportPictureFrame, ExportPictureSession, ExportPictureSource,
        OutputFrameOrdinal,
    },
    picture::{OutputColorDecision, ProjectPictureSession},
};

mod audio_check;
pub mod cli;
pub mod metrics;
mod movie;

pub use audio_check::{AudioCheck, OffsetStatus, compare_audio};
pub use metrics::{I420, PlaneMetrics};
pub use movie::{AudioGap, AudioStream, ColorObservation, EditSummary, PictureAnomaly};

/// Version 2 adds the output branch decision, picture depth, HDR container
/// observations and HDR light cross-check; SDR gates are unchanged.
pub const REPORT_SCHEMA_VERSION: u32 = 2;
/// Largest lag searched for audio alignment. It exceeds both measured AAC
/// timing failures (1,024 and 1,088 samples) with margin.
pub const MAX_ALIGNMENT_LAG: i64 = 2048;
/// Reference segment used for alignment inside each signal window.
pub const ALIGNMENT_SEGMENT: usize = 4096;
/// Loudest nonoverlapping segments aligned independently per window.
pub const ALIGNMENT_SEGMENTS: usize = 8;
/// Short-term level block for the envelope comparison (10 ms).
pub const ENVELOPE_BLOCK: usize = 480;
/// Largest automatic selection; explicit selections may be larger.
pub const DEFAULT_MAX_FRAMES: u64 = 600;
pub const DEFAULT_AUDIO_WINDOW: i64 = 48_000;
pub const MAX_AUDIO_WINDOW: i64 = 480_000;
pub const MAX_AUDIO_WINDOWS: usize = 600;

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("{0}")]
    Request(String),
    #[error("export verification was cancelled")]
    Cancelled,
    #[error("export verification exceeded its deadline")]
    Deadline,
    #[error("movie: {0}")]
    Movie(String),
    #[error("movie timing differs from the committed output contract: {0}")]
    Timing(String),
    #[error(transparent)]
    Store(#[from] deadpan_store::StoreError),
    #[error(transparent)]
    Plan(#[from] deadpan_plan::PlanError),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Picture(#[from] crate::picture::ProjectPictureError),
    #[error(transparent)]
    ExportPicture(#[from] crate::export_picture::ExportPictureError),
    #[error(transparent)]
    Audio(#[from] crate::audio::OfflineAudioError),
    #[error("{0}")]
    Renderer(String),
    #[error("the movie differs from its committed revision in {0} checks")]
    Mismatch(usize),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl VerifyError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Request(_) => "InvalidInput",
            Self::Cancelled => "ExportVerificationCancelled",
            Self::Deadline => "ExportVerificationDeadline",
            Self::Movie(_) | Self::Io(_) => "ExportVerificationMovie",
            Self::Timing(_) => "ExportVerificationTiming",
            Self::Renderer(_) => "ExportVerificationRenderer",
            Self::Mismatch(_) => "ExportVerificationMismatch",
            _ => "ExportVerificationReference",
        }
    }
}

/// Which output frames to compare. Ordinals are relative to the movie start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameSelection {
    /// Every frame up to DEFAULT_MAX_FRAMES, otherwise an even stride.
    Automatic,
    Every(u64),
    List(Vec<u64>),
}

/// Which output-relative 48 kHz windows to compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioSelection {
    /// Consecutive one-second windows covering the whole output.
    Automatic,
    Windows(Vec<Range<i64>>),
    None,
}

/// Documented defaults; see docs/PREVIEW_EXPORT_VERIFICATION.md for rationale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Thresholds {
    /// Eight-bit SDR luma PSNR (peak 255).
    pub min_luma_psnr_db: f64,
    pub min_chroma_psnr_db: f64,
    /// Ten-bit HDR luma PSNR in PQ or HLG signal codes (peak 1023).
    pub min_hdr_luma_psnr_db: f64,
    pub min_hdr_chroma_psnr_db: f64,
    /// Gross mismatch: mean absolute difference of 8x8 luma thumbnails, in
    /// eight-bit-equivalent codes (ten-bit codes divided by four).
    pub max_thumbnail_mad: f64,
    /// HDR local structure: largest difference of 4x4-cell luma means, in
    /// ten-bit PQ/HLG codes. Catches small wrong regions (a missing caption)
    /// that whole-picture PSNR dilutes. SDR reports the value without a gate.
    pub max_hdr_local_luma_error: f64,
    /// A neighboring reference frame must not beat the own frame by more.
    pub neighbor_psnr_margin_db: f64,
    /// Black detection: decoded mean luma at or below while reference is above
    /// `black_reference_min_luma`, both in eight-bit-equivalent codes.
    pub black_max_luma: f64,
    pub black_reference_min_luma: f64,
    /// Windows and blocks whose reference RMS is below this are silent.
    pub silence_dbfs: f64,
    /// A silent reference window fails if decoded peak exceeds this.
    pub silent_max_peak_dbfs: f64,
    /// A silent reference block fails if decoded RMS exceeds this.
    pub silent_block_max_dbfs: f64,
    /// Blocks at or above this reference level get SNR and level gates.
    pub block_floor_dbfs: f64,
    /// Minimum per-block (10 ms) SNR.
    pub min_block_snr_db: f64,
    /// Maximum per-block level difference.
    pub max_block_level_db: f64,
    /// A segment whose peak correlation is weaker is reported uncorrelated.
    pub min_alignment_correlation: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_luma_psnr_db: 32.0,
            min_chroma_psnr_db: 32.0,
            min_hdr_luma_psnr_db: 30.0,
            min_hdr_chroma_psnr_db: 40.0,
            max_thumbnail_mad: 4.0,
            max_hdr_local_luma_error: 40.0,
            neighbor_psnr_margin_db: 0.5,
            black_max_luma: 20.0,
            black_reference_min_luma: 32.0,
            silence_dbfs: -70.0,
            silent_max_peak_dbfs: -50.0,
            silent_block_max_dbfs: -60.0,
            block_floor_dbfs: -60.0,
            min_block_snr_db: 1.5,
            max_block_level_db: 3.0,
            min_alignment_correlation: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VerifyRequest {
    pub package: PathBuf,
    pub movie: PathBuf,
    /// None verifies against the current committed revision.
    pub revision: Option<RevisionId>,
    pub frames: FrameSelection,
    pub audio: AudioSelection,
    pub thresholds: Thresholds,
}

#[derive(Debug, Serialize)]
pub struct VerificationReport {
    pub schema_version: u32,
    pub scope: &'static str,
    pub passed: bool,
    pub failures: Vec<String>,
    pub project_id: String,
    pub revision_id: String,
    pub document_sha256: deadpan_jobs::Sha256,
    pub range: [i64; 2],
    pub frame_rate: [u32; 2],
    pub canvas: [u32; 2],
    pub raster: [u32; 2],
    /// The revision's automatic SDR/HDR branch (output policy, reason, HDR
    /// source peak used for tone mapping and the retained PQ mastering
    /// volume), shared by preview and export.
    pub output_color: OutputColorDecision,
    /// Compared code depth: 8 (SDR I420) or 10 (HDR PQ/HLG yuv420p10).
    pub picture_bits: u8,
    /// PQ only: CTA-861.3 light of the references against the movie's clli.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdr_light: Option<LightCheck>,
    pub movie: MovieSummary,
    pub thresholds: Thresholds,
    pub pictures: Vec<PictureCheck>,
    pub audio: Vec<AudioCheck>,
    pub summary: Summary,
}

#[derive(Debug, Serialize)]
pub struct MovieSummary {
    pub path: PathBuf,
    pub bytes: u64,
    pub video_frames: u64,
    pub expected_video_frames: u64,
    pub video_edits: Option<EditSummary>,
    pub picture_anomalies: Vec<PictureAnomaly>,
    pub color: ColorObservation,
    pub audio_track: Option<u32>,
    /// The audio edit; its media time is the declared priming.
    pub audio_edits: Option<EditSummary>,
    pub audio: Option<AudioStream>,
    pub expected_audio_samples: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    Original {
        asset: String,
        source_frame: u64,
        source_pts: i64,
    },
    Generated {
        source_frame: u64,
        source_pts: i64,
    },
    Background,
}

#[derive(Debug, Serialize)]
pub struct PictureCheck {
    pub output_frame: u64,
    pub project_frame: i64,
    pub provenance: Provenance,
    /// Y, Cb, Cr.
    pub planes: [PlaneMetrics; 3],
    pub reference_mean_luma: f64,
    pub decoded_mean_luma: f64,
    pub thumbnail_mad: f64,
    /// Largest 4x4-cell luma mean difference, in codes of the compared depth.
    pub local_luma_error: f64,
    pub previous_luma_psnr_db: Option<f64>,
    pub next_luma_psnr_db: Option<f64>,
    pub flags: Vec<&'static str>,
    pub passed: bool,
}

/// MaxCLL/MaxFALL recomputed from the reference pictures' clipped linear
/// light, rounded up to whole cd/m² as the encoder host does. Compared with
/// the container `clli` only when every output frame was rendered.
#[derive(Debug, Serialize)]
pub struct LightCheck {
    pub frames_measured: u64,
    pub complete: bool,
    pub reference: ContentLight,
    pub container: Option<ContentLight>,
    pub matches: Option<bool>,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub pictures_checked: usize,
    pub pictures_failed: usize,
    pub min_luma_psnr_db: Option<f64>,
    pub min_chroma_psnr_db: Option<f64>,
    pub max_thumbnail_mad: Option<f64>,
    pub max_local_luma_error: Option<f64>,
    /// Smallest margin between a picture's own luma PSNR and its best neighbor.
    pub min_neighbor_margin_db: Option<f64>,
    pub audio_windows_checked: usize,
    pub audio_windows_failed: usize,
    pub signal_windows: usize,
    pub blocks_compared: usize,
    pub min_block_snr_db: Option<f64>,
    pub max_block_level_db: Option<f64>,
    pub offset_verified_windows: usize,
    pub offset_unobservable_windows: usize,
    /// Windows with a measured nonzero offset.
    pub nonzero_offsets: usize,
}

pub(crate) fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), VerifyError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(VerifyError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(VerifyError::Deadline);
    }
    Ok(())
}

/// Compare one emitted movie with its committed revision. Content mismatches
/// are returned in the report; only unusable inputs return an error.
pub fn verify(
    request: &VerifyRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<VerificationReport, VerifyError> {
    check(cancelled, deadline)?;
    let revision = match &request.revision {
        Some(revision) => revision.clone(),
        None => ProjectStore::open(&request.package, AccessMode::ReadOnly)?
            .snapshot()?
            .revision_id()
            .clone(),
    };
    let pictures =
        ProjectPictureSession::open_revision(&request.package, &revision, None, cancelled)?;
    let output_color = pictures.color_decision();
    let document_sha256 =
        deadpan_jobs::render::document_sha256(pictures.document(), cancelled, deadline)
            .map_err(|error| VerifyError::Request(error.to_string()))?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let renderer = crate::render_worker::worker::metal_renderer(cancelled, deadline)
        .map_err(VerifyError::Renderer)?;
    let mut session = ExportPictureSession::new(pictures, renderer, cancelled, deadline)?;
    let frame_count = contract.frame_count();
    let raster = contract.raster();
    let rate = contract.frame_rate();
    let sample_range = contract.project_audio_start()..contract.project_audio_end();
    let total_samples = sample_range.end.0 - sample_range.start.0;
    let thresholds = request.thresholds;
    let mut failures = Vec::new();

    let selected = select_frames(&request.frames, frame_count)?;
    let windows = select_windows(&request.audio, total_samples)?;

    let expected_color = movie::ExpectedColor {
        policy: contract.color_policy(),
        mastering: contract.mastering_display(),
    };
    let hdr = expected_color.hdr().is_some();
    let picture_bits = if hdr { 10 } else { 8 };
    let movie_bytes = std::fs::metadata(&request.movie)?.len();
    let inspection = movie::inspect(
        &request.movie,
        raster,
        frame_count,
        hdr,
        cancelled,
        deadline,
    )?;
    let video_track = movie::track(&inspection, Mp4TrackKind::Video);
    let audio_track = movie::track(&inspection, Mp4TrackKind::Audio);
    let video_edits = video_track.map(|track| movie::edits(&inspection, track));
    let audio_edits = audio_track.map(|track| movie::edits(&inspection, track));
    // Container edits: exactly one non-empty edit each, presenting exactly the
    // committed duration. The priming value itself is only cross-checked with
    // the decoder (which applies the same edit); content alignment is the real
    // timing guarantee.
    match (video_track, video_edits) {
        (Some(track), Some(edits)) => {
            // presented ticks / timescale == frame_count * D / N.
            let expected = i128::from(rate.denominator())
                * i128::from(frame_count)
                * i128::from(track.media_timescale);
            let presented = edits
                .presented_media_ticks
                .map(|ticks| i128::from(ticks) * i128::from(rate.numerator()));
            if edits.edits != 1
                || edits.empty_edits != 0
                || edits.media_time.is_none_or(|time| time < 0)
            {
                failures.push(format!(
                    "video edit list is not one non-empty edit: {edits:?}"
                ));
            } else if presented != Some(expected) {
                failures.push(format!(
                    "video edit presents {:?} media ticks, not {frame_count} frames",
                    edits.presented_media_ticks
                ));
            }
        }
        _ => failures.push("movie has no video track".into()),
    }
    if let Some(edits) = audio_edits {
        if edits.edits != 1
            || edits.empty_edits != 0
            || edits.media_time.is_none_or(|time| time < 0)
        {
            failures.push(format!(
                "audio edit list is not one non-empty edit: {edits:?}"
            ));
        } else if edits.presented_media_ticks != Some(total_samples) {
            failures.push(format!(
                "audio edit presents {:?} samples; the committed range has {total_samples}",
                edits.presented_media_ticks
            ));
        }
    }

    // Pictures: stream decoded frames, rendering references for n-1, n, n+1.
    let mut references: BTreeMap<u64, (I420, Provenance)> = BTreeMap::new();
    let mut checks = Vec::new();
    let mut light = ContentLightAccumulator::default();
    let mut lit: BTreeSet<u64> = BTreeSet::new();
    let mut reference = |session: &mut ExportPictureSession,
                         references: &mut BTreeMap<u64, (I420, Provenance)>,
                         ordinal: u64|
     -> Result<(), VerifyError> {
        if references.contains_key(&ordinal) {
            return Ok(());
        }
        let frame = session.prepare(OutputFrameOrdinal(ordinal), cancelled, deadline)?;
        let (planes, frame_light) = reference_planes(&frame, hdr)?;
        if let Some(frame_light) = frame_light
            && lit.insert(ordinal)
        {
            light.add(frame_light);
        }
        let provenance = provenance(&frame);
        drop(frame);
        references.insert(ordinal, (planes, provenance));
        Ok(())
    };
    let stream = movie::pictures(
        &request.movie,
        movie::PictureGrid {
            raster,
            frames: frame_count,
            rate: (rate.numerator(), rate.denominator()),
        },
        expected_color,
        video_track,
        cancelled,
        deadline,
        |ordinal, picture| {
            if selected.binary_search(&ordinal).is_err() {
                return Ok(());
            }
            let low = ordinal.saturating_sub(1);
            let high = (ordinal + 1).min(frame_count - 1);
            for neighbor in low..=high {
                reference(&mut session, &mut references, neighbor)?;
            }
            checks.push(compare_picture(
                contract.range().start(),
                ordinal,
                &picture,
                &mut references,
                &thresholds,
            ));
            // Retain only the previous frame for the next selected neighbor.
            references.retain(|&key, _| key >= ordinal);
            Ok(())
        },
    )?;
    let hdr_light = (contract.color_policy() == ColorPolicy::HdrRec2020Pq).then(|| {
        let measured = u64::try_from(lit.len()).unwrap_or(u64::MAX);
        let complete = measured == frame_count;
        let container = stream.color.content_light;
        LightCheck {
            frames_measured: measured,
            complete,
            reference: {
                let value = light.finish();
                ContentLight {
                    max_cll: value.max_cll,
                    max_fall: value.max_fall,
                }
            },
            container,
            matches: None,
        }
    });
    let hdr_light = hdr_light.map(|mut check| {
        if check.complete
            && let Some(container) = check.container
        {
            let matches = container == check.reference;
            check.matches = Some(matches);
            if !matches {
                failures.push(format!(
                    "container clli {container:?} differs from the reference pictures' light {:?}",
                    check.reference
                ));
            }
        }
        check
    });
    if stream.wrong_branch {
        // Reported once as a color problem below; no picture was compared.
    } else if stream.decoded != frame_count {
        failures.push(format!(
            "movie decoded {} pictures; the committed range has {frame_count}",
            stream.decoded
        ));
    }
    failures.extend(
        stream
            .anomalies
            .iter()
            .map(|anomaly| format!("picture timing: {} (PTS {})", anomaly.problem, anomaly.pts)),
    );
    failures.extend(
        stream
            .color
            .problems
            .iter()
            .map(|problem| format!("color: {problem}")),
    );
    for ordinal in &selected {
        if !checks.iter().any(|check| check.output_frame == *ordinal) {
            failures.push(format!(
                "picture {ordinal}: no decoded picture at this output frame"
            ));
        }
    }

    // Audio: windows stream through the decoder; references are read per window.
    let mut audio_checks = Vec::new();
    let mut audio_stream = None;
    if !windows.is_empty() {
        let track = audio_track
            .ok_or_else(|| VerifyError::Movie("movie has no audio track to compare".into()))?;
        // Margins reach below zero so early shifts at the output start are
        // observable against decoded priming and the implied silence before it.
        let padded: Vec<Range<i64>> = windows
            .iter()
            .map(|window| window.start - MAX_ALIGNMENT_LAG..window.end + MAX_ALIGNMENT_LAG)
            .collect();
        let mut offline = OfflineAudioSession::open_revision(
            &request.package,
            &revision,
            contract.range(),
            cancelled,
            deadline,
        )?;
        let mut results: Vec<Option<AudioCheck>> = windows.iter().map(|_| None).collect();
        let presented = audio_edits
            .and_then(|edits| edits.presented_media_ticks)
            .unwrap_or(total_samples);
        let decoded = movie::audio(
            &request.movie,
            track.index,
            presented,
            &padded,
            cancelled,
            deadline,
            |index, samples| {
                let window = windows[index].clone();
                let reference =
                    read_reference(&mut offline, sample_range.start, window.clone(), cancelled)?;
                results[index] = Some(compare_audio(
                    window,
                    sample_range.start,
                    &reference,
                    samples.range.start,
                    &samples.samples,
                    &samples.covered,
                    &thresholds,
                ));
                Ok(())
            },
        )?;
        let declared = audio_edits.and_then(|edits| edits.media_time);
        if decoded.first_pts != declared.map(|time| -time) {
            failures.push(format!(
                "first decoded AAC PTS {:?} does not match the declared priming edit {declared:?}",
                decoded.first_pts
            ));
        }
        if decoded.presented_end != total_samples {
            failures.push(format!(
                "movie presents audio through sample {}; the committed range has {total_samples}",
                decoded.presented_end
            ));
        }
        failures.extend(decoded.gaps.iter().map(|gap| {
            format!(
                "audio timing: decoded frame at {} where {} was expected",
                gap.actual, gap.expected
            )
        }));
        audio_stream = Some(decoded);
        audio_checks = results.into_iter().flatten().collect();
    }

    let summary = summarize(&checks, &audio_checks);
    failures.extend(
        checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| format!("picture {}: {}", check.output_frame, check.flags.join(","))),
    );
    failures.extend(
        audio_checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| {
                format!(
                    "audio [{}, {}): {}",
                    check.window[0],
                    check.window[1],
                    check.flags.join(",")
                )
            }),
    );
    Ok(VerificationReport {
        schema_version: REPORT_SCHEMA_VERSION,
        scope: "diagnostic comparison of one emitted movie with its committed revision through the shared picture path and limited audition bus; grants no publication authority",
        passed: failures.is_empty(),
        failures,
        project_id: contract.project_id().as_str().to_owned(),
        revision_id: revision.as_str().to_owned(),
        document_sha256,
        range: [contract.range().start().0, contract.range().end().0],
        frame_rate: [rate.numerator(), rate.denominator()],
        canvas: contract.canvas(),
        raster,
        output_color,
        picture_bits,
        hdr_light,
        movie: MovieSummary {
            path: request.movie.clone(),
            bytes: movie_bytes,
            video_frames: stream.decoded,
            expected_video_frames: frame_count,
            video_edits,
            picture_anomalies: stream.anomalies,
            color: stream.color,
            audio_track: audio_track.map(|track| track.index),
            audio_edits,
            audio: audio_stream,
            expected_audio_samples: u64::try_from(total_samples).unwrap_or(0),
        },
        thresholds,
        pictures: checks,
        audio: audio_checks,
        summary,
    })
}

/// Reference planes at the branch boundary: eight-bit I420 for SDR, ten-bit
/// Rec.2100 (with its CTA-861.3 light) for HDR. A frame from the other
/// branch is a renderer fault, never compared.
fn reference_planes(
    frame: &ExportPictureFrame,
    hdr: bool,
) -> Result<(I420, Option<FrameLight>), VerifyError> {
    match (frame.pixels().sdr(), frame.pixels().hdr()) {
        (Some(pixels), None) if !hdr => {
            let mut bytes = Vec::with_capacity(pixels.bytes().len());
            bytes.extend_from_slice(pixels.y_plane());
            bytes.extend_from_slice(pixels.cb_plane());
            bytes.extend_from_slice(pixels.cr_plane());
            let planes =
                I420::from_tight(pixels.width(), pixels.height(), &bytes).ok_or_else(|| {
                    VerifyError::Renderer("reference picture is not tight I420".into())
                })?;
            Ok((planes, None))
        }
        (None, Some((pixels, light))) if hdr => {
            let planes = I420::from_tight_p10_le(pixels.width(), pixels.height(), pixels.bytes())
                .ok_or_else(|| {
                VerifyError::Renderer("reference picture is not tight yuv420p10".into())
            })?;
            Ok((planes, Some(light)))
        }
        _ => Err(VerifyError::Renderer(
            "reference pixels differ from the contract's output branch".into(),
        )),
    }
}

fn provenance(frame: &ExportPictureFrame) -> Provenance {
    match frame.source() {
        ExportPictureSource::Original { asset, id, pts, .. } => Provenance::Original {
            asset: asset.as_str().to_owned(),
            source_frame: id.0,
            source_pts: pts.ticks,
        },
        ExportPictureSource::Generated { id, pts, .. } => Provenance::Generated {
            source_frame: id.0,
            source_pts: pts.ticks,
        },
        ExportPictureSource::Background => Provenance::Background,
    }
}

/// One reference picture of a committed revision, exactly as `verify`
/// renders it, for diagnostics and qualification fixtures.
#[derive(Debug)]
pub struct ReferencePicture {
    pub output_color: OutputColorDecision,
    pub color_policy: ColorPolicy,
    pub planes: I420,
    /// HDR only: the frame's clipped linear light statistics.
    pub light: Option<FrameLight>,
    pub provenance: Provenance,
}

/// Render the reference for output frames `ordinals` of `revision` through
/// the shared picture session and the branch's encoder pixel boundary.
pub fn reference_pictures(
    package: &Path,
    revision: &RevisionId,
    ordinals: &[u64],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<ReferencePicture>, VerifyError> {
    let pictures = ProjectPictureSession::open_revision(package, revision, None, cancelled)?;
    let output_color = pictures.color_decision();
    let contract = ExportPictureContract::capture(&pictures)?;
    let hdr = contract.color_policy() != ColorPolicy::SdrRec709;
    let renderer = crate::render_worker::worker::metal_renderer(cancelled, deadline)
        .map_err(VerifyError::Renderer)?;
    let mut session = ExportPictureSession::new(pictures, renderer, cancelled, deadline)?;
    let mut rendered = Vec::with_capacity(ordinals.len());
    for &ordinal in ordinals {
        if ordinal >= contract.frame_count() {
            return Err(VerifyError::Request(format!(
                "frame {ordinal} is outside the committed {} output frames",
                contract.frame_count()
            )));
        }
        let frame = session.prepare(OutputFrameOrdinal(ordinal), cancelled, deadline)?;
        let (planes, light) = reference_planes(&frame, hdr)?;
        rendered.push(ReferencePicture {
            output_color,
            color_policy: contract.color_policy(),
            planes,
            light,
            provenance: provenance(&frame),
        });
    }
    Ok(rendered)
}

fn select_frames(selection: &FrameSelection, count: u64) -> Result<Vec<u64>, VerifyError> {
    let frames: Vec<u64> = match selection {
        FrameSelection::Automatic => {
            let stride = count.div_ceil(DEFAULT_MAX_FRAMES).max(1);
            (0..count)
                .step_by(usize::try_from(stride).unwrap_or(usize::MAX))
                .collect()
        }
        FrameSelection::Every(0) => {
            return Err(VerifyError::Request("--every must be positive".into()));
        }
        FrameSelection::Every(stride) => (0..count)
            .step_by(usize::try_from(*stride).unwrap_or(usize::MAX))
            .collect(),
        FrameSelection::List(list) => {
            let mut list = list.clone();
            list.sort_unstable();
            list.dedup();
            if let Some(last) = list.last()
                && *last >= count
            {
                return Err(VerifyError::Request(format!(
                    "frame {last} is outside the committed {count} output frames"
                )));
            }
            list
        }
    };
    Ok(frames)
}

fn select_windows(selection: &AudioSelection, total: i64) -> Result<Vec<Range<i64>>, VerifyError> {
    let windows: Vec<Range<i64>> = match selection {
        AudioSelection::None => Vec::new(),
        AudioSelection::Automatic => {
            let mut windows: Vec<Range<i64>> = (0..total)
                .step_by(DEFAULT_AUDIO_WINDOW as usize)
                .map(|start| start..(start + DEFAULT_AUDIO_WINDOW).min(total))
                .collect();
            // A short remainder joins its predecessor instead of forming a
            // tiny window dominated by encoder end padding.
            if windows.len() > 1
                && windows
                    .last()
                    .is_some_and(|last| last.end - last.start < DEFAULT_AUDIO_WINDOW / 2)
            {
                let last = windows.pop().map_or(total, |last| last.end);
                if let Some(previous) = windows.last_mut() {
                    previous.end = last;
                }
            }
            windows
        }
        AudioSelection::Windows(windows) => windows.clone(),
    };
    let windows =
        if matches!(selection, AudioSelection::Automatic) && windows.len() > MAX_AUDIO_WINDOWS {
            // Long outputs: an even stride of windows, always including the last.
            let count = windows.len();
            (0..MAX_AUDIO_WINDOWS)
                .map(|index| windows[index * (count - 1) / (MAX_AUDIO_WINDOWS - 1)].clone())
                .collect()
        } else {
            windows
        };
    if windows.len() > MAX_AUDIO_WINDOWS {
        return Err(VerifyError::Request(format!(
            "at most {MAX_AUDIO_WINDOWS} audio windows"
        )));
    }
    for window in &windows {
        if window.start < 0
            || window.end > total
            || window.end <= window.start
            || window.end - window.start > MAX_AUDIO_WINDOW
        {
            return Err(VerifyError::Request(format!(
                "audio window [{}, {}) must be nonempty, at most {MAX_AUDIO_WINDOW} samples and inside [0, {total})",
                window.start, window.end
            )));
        }
    }
    Ok(windows)
}

fn read_reference(
    offline: &mut OfflineAudioSession,
    origin: AudioSample,
    window: Range<i64>,
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, VerifyError> {
    let mut samples = Vec::with_capacity(usize::try_from(window.end - window.start).unwrap_or(0));
    let mut at = window.start;
    while at < window.end {
        let count = (window.end - at).min(i64::from(crate::audio::MAX_OFFLINE_AUDIO_FRAMES));
        let block = offline.read(
            AudioSample(origin.0 + at),
            u32::try_from(count).map_err(|_| VerifyError::Request("audio window".into()))?,
            cancelled,
        )?;
        samples.extend_from_slice(&block.samples);
        at += count;
    }
    Ok(samples)
}

fn compare_picture(
    range_start: ProjectFrame,
    ordinal: u64,
    decoded: &I420,
    references: &mut BTreeMap<u64, (I420, Provenance)>,
    thresholds: &Thresholds,
) -> PictureCheck {
    let peak = decoded.peak();
    let luma_psnr = |other: Option<&(I420, Provenance)>| {
        other.map(|(picture, _)| metrics::plane_metrics(&picture.y, &decoded.y, peak).psnr_db)
    };
    let previous = ordinal
        .checked_sub(1)
        .and_then(|key| luma_psnr(references.get(&key)));
    let next = luma_psnr(references.get(&(ordinal + 1)));
    let (reference, provenance) = references
        .remove(&ordinal)
        .expect("selected reference is rendered before comparison");
    let planes = [
        metrics::plane_metrics(&reference.y, &decoded.y, peak),
        metrics::plane_metrics(&reference.cb, &decoded.cb, peak),
        metrics::plane_metrics(&reference.cr, &decoded.cr, peak),
    ];
    let (min_luma, min_chroma) = if decoded.bits > 8 {
        (
            thresholds.min_hdr_luma_psnr_db,
            thresholds.min_hdr_chroma_psnr_db,
        )
    } else {
        (thresholds.min_luma_psnr_db, thresholds.min_chroma_psnr_db)
    };
    let thumbnail_mad = metrics::thumbnail_mad(&reference.thumbnail(), &decoded.thumbnail());
    let local_luma_error =
        metrics::max_local_error(&reference.local_cells(), &decoded.local_cells());
    let reference_mean_luma = reference.mean_luma();
    let decoded_mean_luma = decoded.mean_luma();
    let mut flags = Vec::new();
    if planes[0].psnr_db < min_luma {
        flags.push("luma_psnr");
    }
    if planes[1].psnr_db.min(planes[2].psnr_db) < min_chroma {
        flags.push("chroma_psnr");
    }
    if thumbnail_mad > thresholds.max_thumbnail_mad {
        flags.push("gross_structural_mismatch");
    }
    if decoded.bits > 8 && local_luma_error > thresholds.max_hdr_local_luma_error {
        flags.push("local_structure_mismatch");
    }
    if decoded_mean_luma <= thresholds.black_max_luma
        && reference_mean_luma > thresholds.black_reference_min_luma
    {
        flags.push("unexpected_black_frame");
    }
    if [previous, next]
        .into_iter()
        .flatten()
        .any(|neighbor| neighbor > planes[0].psnr_db + thresholds.neighbor_psnr_margin_db)
    {
        flags.push("frame_index_mismatch");
    }
    let provenance_copy = provenance.clone();
    // Keep the reference so the next selected ordinal can use it as a neighbor.
    references.insert(ordinal, (reference, provenance));
    PictureCheck {
        output_frame: ordinal,
        project_frame: range_start.0 + i64::try_from(ordinal).unwrap_or(i64::MAX),
        provenance: provenance_copy,
        planes,
        reference_mean_luma,
        decoded_mean_luma,
        thumbnail_mad,
        local_luma_error,
        previous_luma_psnr_db: previous,
        next_luma_psnr_db: next,
        passed: flags.is_empty(),
        flags,
    }
}

fn summarize(pictures: &[PictureCheck], audio: &[AudioCheck]) -> Summary {
    let minimum = |values: &mut dyn Iterator<Item = f64>| values.reduce(f64::min);
    let maximum = |values: &mut dyn Iterator<Item = f64>| values.reduce(f64::max);
    let status = |wanted: OffsetStatus| {
        audio
            .iter()
            .filter(|check| check.offset_status == wanted)
            .count()
    };
    Summary {
        pictures_checked: pictures.len(),
        pictures_failed: pictures.iter().filter(|check| !check.passed).count(),
        min_luma_psnr_db: minimum(&mut pictures.iter().map(|check| check.planes[0].psnr_db)),
        min_chroma_psnr_db: minimum(
            &mut pictures
                .iter()
                .map(|check| check.planes[1].psnr_db.min(check.planes[2].psnr_db)),
        ),
        max_thumbnail_mad: maximum(&mut pictures.iter().map(|check| check.thumbnail_mad)),
        max_local_luma_error: maximum(&mut pictures.iter().map(|check| check.local_luma_error)),
        min_neighbor_margin_db: minimum(&mut pictures.iter().filter_map(|check| {
            [check.previous_luma_psnr_db, check.next_luma_psnr_db]
                .into_iter()
                .flatten()
                .reduce(f64::max)
                .map(|neighbor| check.planes[0].psnr_db - neighbor)
        })),
        audio_windows_checked: audio.len(),
        audio_windows_failed: audio.iter().filter(|check| !check.passed).count(),
        signal_windows: audio.iter().filter(|check| check.kind == "signal").count(),
        blocks_compared: audio.iter().map(|check| check.blocks.compared).sum(),
        min_block_snr_db: minimum(&mut audio.iter().filter_map(|check| check.blocks.min_snr_db)),
        max_block_level_db: maximum(
            &mut audio
                .iter()
                .filter_map(|check| check.blocks.max_level_difference_db),
        ),
        offset_verified_windows: status(OffsetStatus::VerifiedZero),
        offset_unobservable_windows: status(OffsetStatus::Unobservable),
        nonzero_offsets: status(OffsetStatus::Offset),
    }
}

#[cfg(test)]
mod tests;

/// Convenience used by the CLI and tests.
pub fn verify_path(
    package: &Path,
    movie: &Path,
    revision: Option<RevisionId>,
) -> Result<VerificationReport, VerifyError> {
    verify(
        &VerifyRequest {
            package: package.to_owned(),
            movie: movie.to_owned(),
            revision,
            frames: FrameSelection::Automatic,
            audio: AudioSelection::Automatic,
            thresholds: Thresholds::default(),
        },
        &AtomicBool::new(false),
        Instant::now() + std::time::Duration::from_secs(1800),
    )
}
