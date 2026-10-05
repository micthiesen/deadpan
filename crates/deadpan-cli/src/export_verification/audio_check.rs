//! One audio window: per-block content gates and multi-segment alignment.

use std::ops::Range;

use deadpan_core::AudioSample;
use serde::Serialize;

use super::{
    ALIGNMENT_SEGMENT, ALIGNMENT_SEGMENTS, ENVELOPE_BLOCK, MAX_ALIGNMENT_LAG, Thresholds,
    metrics::{self, Alignment, AlignmentStatus, BlockGates, BlockSummary},
};

/// Offset outcome of one window, never collapsed into "0" when unobserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetStatus {
    /// Silent reference: no offset can be measured; silence is checked instead.
    NotApplicable,
    /// At least one segment has a unique zero-lag maximum and none disagrees.
    VerifiedZero,
    /// At least one segment measured a nonzero lag.
    Offset,
    /// Every aligned segment is exactly periodic; the offset is unobservable.
    Unobservable,
    /// No segment correlated with the decoded file.
    Uncorrelated,
}

#[derive(Debug, Serialize)]
pub struct AudioCheck {
    /// Output-relative half-open 48 kHz sample window.
    pub window: [i64; 2],
    pub project_samples: [i64; 2],
    pub kind: &'static str,
    pub reference_rms_dbfs: f64,
    pub decoded_rms_dbfs: f64,
    pub reference_peak_dbfs: f64,
    pub decoded_peak_dbfs: f64,
    pub error_rms_dbfs: f64,
    /// Whole-window SNR (informational; per-block SNR is gated).
    pub snr_db: Option<f64>,
    pub blocks: BlockSummary,
    /// Window samples the file did not present.
    pub uncovered_samples: usize,
    pub segments: Vec<Alignment>,
    pub offset_status: OffsetStatus,
    /// Present only for VerifiedZero (0) and Offset (the measured lag).
    /// Reported exactly; never compensated.
    pub measured_offset_samples: Option<i64>,
    pub flags: Vec<&'static str>,
    pub passed: bool,
}

/// Compare one window. `decoded` holds file samples from output sample
/// `decoded_start` (which may be negative), and `covered` marks the samples the
/// file actually presented.
pub fn compare_audio(
    window: Range<i64>,
    origin: AudioSample,
    reference: &[[f32; 2]],
    decoded_start: i64,
    decoded: &[[f32; 2]],
    covered: &[bool],
    thresholds: &Thresholds,
) -> AudioCheck {
    let mut flags = Vec::new();
    let length = reference.len();
    let offset = usize::try_from(window.start - decoded_start).unwrap_or(usize::MAX);
    let silence = vec![[0.0_f32; 2]; length];
    let (aligned, uncovered) = match (
        decoded.get(offset..offset.saturating_add(length)),
        covered.get(offset..offset.saturating_add(length)),
    ) {
        (Some(aligned), Some(covered)) => (aligned, covered.iter().filter(|&&seen| !seen).count()),
        _ => (&silence[..], length),
    };
    if uncovered > 0 {
        flags.push("decoded_audio_gap");
    }
    let reference_rms = metrics::rms(reference);
    let error = metrics::error_rms(reference, aligned);
    let silent = metrics::dbfs(reference_rms) < thresholds.silence_dbfs;
    let decoded_peak = metrics::peak(aligned);
    let blocks = metrics::compare_blocks(
        reference,
        aligned,
        window.start,
        &BlockGates {
            block: ENVELOPE_BLOCK,
            floor_dbfs: thresholds.block_floor_dbfs,
            silence_dbfs: thresholds.silence_dbfs,
            min_snr_db: thresholds.min_block_snr_db,
            max_level_db: thresholds.max_block_level_db,
            silent_max_dbfs: thresholds.silent_block_max_dbfs,
        },
    );
    if blocks.low_snr_blocks > 0 {
        flags.push("audio_block_snr");
    }
    if blocks.level_blocks > 0 {
        flags.push("audio_level");
    }
    if blocks.sound_in_silence_blocks > 0 {
        flags.push("unexpected_sound_in_silence");
    }
    let mut segments = Vec::new();
    let mut snr = None;
    let offset_status = if silent {
        if metrics::dbfs(decoded_peak) > thresholds.silent_max_peak_dbfs {
            flags.push("unexpected_sound_in_silence");
        }
        OffsetStatus::NotApplicable
    } else {
        snr = Some(
            (20.0 * (reference_rms / error.max(f64::MIN_POSITIVE)).log10())
                .min(metrics::IDENTICAL_PSNR_DB),
        );
        for start in metrics::loud_segments(
            reference,
            ALIGNMENT_SEGMENT,
            ALIGNMENT_SEGMENTS,
            thresholds.silence_dbfs,
        ) {
            let end = (start + ALIGNMENT_SEGMENT).min(length);
            if let Some(found) = metrics::align(
                &reference[start..end],
                window.start + start as i64,
                decoded,
                decoded_start,
                MAX_ALIGNMENT_LAG,
                thresholds.min_alignment_correlation,
            ) {
                segments.push(found);
            }
        }
        let has = |status| segments.iter().any(|segment| segment.status == status);
        if has(AlignmentStatus::Offset) {
            OffsetStatus::Offset
        } else if has(AlignmentStatus::VerifiedZero) {
            OffsetStatus::VerifiedZero
        } else if has(AlignmentStatus::Periodic) {
            OffsetStatus::Unobservable
        } else {
            OffsetStatus::Uncorrelated
        }
    };
    if segments
        .iter()
        .any(|segment| segment.status == AlignmentStatus::Uncorrelated)
    {
        flags.push("audio_uncorrelated");
    }
    let measured = match offset_status {
        OffsetStatus::Offset => {
            flags.push("audio_offset");
            segments
                .iter()
                .find(|segment| segment.status == AlignmentStatus::Offset)
                .map(|segment| segment.peak_lag)
        }
        OffsetStatus::VerifiedZero => Some(0),
        OffsetStatus::Unobservable => {
            flags.push("audio_offset_unobservable");
            None
        }
        OffsetStatus::Uncorrelated => {
            if !flags.contains(&"audio_uncorrelated") {
                flags.push("audio_uncorrelated");
            }
            None
        }
        OffsetStatus::NotApplicable => None,
    };
    AudioCheck {
        window: [window.start, window.end],
        project_samples: [origin.0 + window.start, origin.0 + window.end],
        kind: if silent { "silent" } else { "signal" },
        reference_rms_dbfs: metrics::dbfs(reference_rms),
        decoded_rms_dbfs: metrics::dbfs(metrics::rms(aligned)),
        reference_peak_dbfs: metrics::dbfs(metrics::peak(reference)),
        decoded_peak_dbfs: metrics::dbfs(decoded_peak),
        error_rms_dbfs: metrics::dbfs(error),
        snr_db: snr,
        blocks,
        uncovered_samples: uncovered,
        segments,
        offset_status,
        measured_offset_samples: measured,
        passed: flags.is_empty(),
        flags,
    }
}
