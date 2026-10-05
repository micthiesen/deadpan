//! Pure comparison measurements. No media, GPU or store access happens here,
//! so every threshold decision is unit-testable with synthetic planes and PCM.

use serde::Serialize;

/// PSNR reported for bit-identical planes, whose mathematical value is infinite.
pub const IDENTICAL_PSNR_DB: f64 = 100.0;
/// Luma thumbnail block edge in pixels for gross structural comparison.
pub const THUMBNAIL_BLOCK: u32 = 8;

/// Tight 8-bit limited-range I420 planes with even dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I420 {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
}

impl I420 {
    /// Split one tight Y, Cb, Cr buffer. Returns None for an inexact length.
    pub fn from_tight(width: u32, height: u32, bytes: &[u8]) -> Option<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return None;
        }
        let luma = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
        let chroma = luma / 4;
        if bytes.len() != luma.checked_add(chroma.checked_mul(2)?)? {
            return None;
        }
        Some(Self {
            width,
            height,
            y: bytes[..luma].to_vec(),
            cb: bytes[luma..luma + chroma].to_vec(),
            cr: bytes[luma + chroma..].to_vec(),
        })
    }

    pub fn mean_luma(&self) -> f64 {
        mean(&self.y)
    }

    /// Box-averaged luma at 1/THUMBNAIL_BLOCK scale, partial edge blocks included.
    pub fn thumbnail(&self) -> Vec<f64> {
        let block = THUMBNAIL_BLOCK as usize;
        let width = self.width as usize;
        let height = self.height as usize;
        let columns = width.div_ceil(block);
        let rows = height.div_ceil(block);
        let mut sums = vec![(0_u64, 0_u64); columns * rows];
        for (index, &value) in self.y.iter().enumerate() {
            let (row, column) = (index / width / block, index % width / block);
            let cell = &mut sums[row * columns + column];
            cell.0 += u64::from(value);
            cell.1 += 1;
        }
        sums.into_iter()
            .map(|(sum, count)| sum as f64 / count as f64)
            .collect()
    }
}

fn mean(values: &[u8]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().map(|&value| u64::from(value)).sum::<u64>() as f64 / values.len() as f64
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlaneMetrics {
    /// 8-bit peak (255) PSNR; IDENTICAL_PSNR_DB when the planes are identical.
    pub psnr_db: f64,
    pub max_abs_error: u8,
    pub mean_abs_error: f64,
}

pub fn plane_metrics(reference: &[u8], actual: &[u8]) -> PlaneMetrics {
    debug_assert_eq!(reference.len(), actual.len());
    let mut squared = 0_u64;
    let mut absolute = 0_u64;
    let mut maximum = 0_u8;
    for (&left, &right) in reference.iter().zip(actual) {
        let difference = left.abs_diff(right);
        maximum = maximum.max(difference);
        absolute += u64::from(difference);
        squared += u64::from(difference) * u64::from(difference);
    }
    let count = reference.len().max(1) as f64;
    let mse = squared as f64 / count;
    PlaneMetrics {
        psnr_db: psnr(mse),
        max_abs_error: maximum,
        mean_abs_error: absolute as f64 / count,
    }
}

fn psnr(mse: f64) -> f64 {
    if mse == 0.0 {
        IDENTICAL_PSNR_DB
    } else {
        (10.0 * (255.0 * 255.0 / mse).log10()).min(IDENTICAL_PSNR_DB)
    }
}

pub fn thumbnail_mad(reference: &[f64], actual: &[f64]) -> f64 {
    if reference.is_empty() {
        return 0.0;
    }
    reference
        .iter()
        .zip(actual)
        .map(|(left, right)| (left - right).abs())
        .sum::<f64>()
        / reference.len() as f64
}

/// Decibels relative to digital full scale, floored at -200 for exact zero.
pub fn dbfs(linear: f64) -> f64 {
    if linear <= 0.0 {
        -200.0
    } else {
        (20.0 * linear.log10()).max(-200.0)
    }
}

/// Root mean square over both channels of interleaved stereo frames.
pub fn rms(samples: &[[f32; 2]]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .flatten()
        .map(|&value| f64::from(value) * f64::from(value))
        .sum();
    (sum / (samples.len() * 2) as f64).sqrt()
}

pub fn peak(samples: &[[f32; 2]]) -> f64 {
    samples
        .iter()
        .flatten()
        .map(|value| f64::from(value.abs()))
        .fold(0.0, f64::max)
}

pub fn error_rms(reference: &[[f32; 2]], actual: &[[f32; 2]]) -> f64 {
    let difference: Vec<[f32; 2]> = reference
        .iter()
        .zip(actual)
        .map(|(left, right)| [right[0] - left[0], right[1] - left[1]])
        .collect();
    rms(&difference)
}

/// Outcome of one segment's alignment search.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentStatus {
    /// A unique correlation maximum at lag zero.
    VerifiedZero,
    /// The strongest maximum is at a nonzero lag (or zero lag is not among
    /// equally strong periodic maxima). Always a failure.
    Offset,
    /// Several separated maxima within tolerance of the peak, zero lag among
    /// them: exactly periodic content. The offset is not observable here and
    /// is never reported as zero.
    Periodic,
    /// Peak correlation is below the minimum; content differs.
    Uncorrelated,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Alignment {
    /// Reference segment start in output samples.
    pub segment_start: i64,
    pub segment_samples: u32,
    /// Lag with the largest normalized correlation: decoded(t + lag) ~ reference(t).
    pub peak_lag: i64,
    pub peak_correlation: f64,
    pub zero_lag_correlation: f64,
    /// Strongest local maximum at least PERIODICITY_GUARD lags from the peak.
    pub secondary_lag: Option<i64>,
    pub secondary_correlation: Option<f64>,
    pub status: AlignmentStatus,
}

/// Lags closer than this to the peak belong to the same correlation lobe and
/// never establish periodicity; smooth content has wide lobes.
pub const PERIODICITY_GUARD: i64 = 32;
/// Correlation tolerance for "equally strong" maxima.
pub const PERIODICITY_TOLERANCE: f64 = 1e-3;

/// Normalized stereo cross-correlation (both channels in one inner product,
/// never a mono sum that can cancel opposite-polarity channels) between one
/// reference segment and the decoded track over every lag in
/// [-max_lag, max_lag]. `decoded` starts at output sample `decoded_start`;
/// decoded samples outside it count as silence, so every lag is evaluated,
/// including early shifts at the start of the output.
pub fn align(
    reference: &[[f32; 2]],
    reference_start: i64,
    decoded: &[[f32; 2]],
    decoded_start: i64,
    max_lag: i64,
    min_correlation: f64,
) -> Option<Alignment> {
    let reference_energy: f64 = reference
        .iter()
        .flatten()
        .map(|&value| f64::from(value) * f64::from(value))
        .sum();
    if reference.is_empty() || reference_energy == 0.0 || max_lag < 0 {
        return None;
    }
    let length = reference.len() as i64;
    // Padded decoded support [reference_start - max_lag, reference_start + length + max_lag).
    let support_start = reference_start - max_lag;
    let support: Vec<[f64; 2]> = (support_start..reference_start + length + max_lag)
        .map(|index| {
            usize::try_from(index - decoded_start)
                .ok()
                .and_then(|index| decoded.get(index))
                .map_or([0.0; 2], |frame| [f64::from(frame[0]), f64::from(frame[1])])
        })
        .collect();
    let reference: Vec<[f64; 2]> = reference
        .iter()
        .map(|frame| [f64::from(frame[0]), f64::from(frame[1])])
        .collect();
    let power = |frame: &[f64; 2]| frame[0] * frame[0] + frame[1] * frame[1];
    let span = usize::try_from(length).ok()?;
    let mut energy: f64 = support[..span].iter().map(power).sum();
    let mut correlations = Vec::with_capacity(usize::try_from(2 * max_lag + 1).ok()?);
    for shift in 0..=usize::try_from(2 * max_lag).ok()? {
        if shift > 0 {
            // Sliding window energy; recomputed exactly every 256 lags.
            energy += power(&support[shift + span - 1]) - power(&support[shift - 1]);
            if shift % 256 == 0 {
                energy = support[shift..shift + span].iter().map(power).sum();
            }
        }
        let window = &support[shift..shift + span];
        let product: f64 = reference
            .iter()
            .zip(window)
            .map(|(left, right)| left[0] * right[0] + left[1] * right[1])
            .sum();
        correlations.push(if energy <= 1e-30 {
            0.0
        } else {
            product / (reference_energy * energy).sqrt()
        });
    }
    let at = |lag: i64| correlations[usize::try_from(lag + max_lag).unwrap_or(0)];
    // Ties prefer the smaller absolute lag.
    let peak_lag = (-max_lag..=max_lag).fold(0, |best, lag| {
        if at(lag) > at(best) || (at(lag) == at(best) && lag.abs() < best.abs()) {
            lag
        } else {
            best
        }
    });
    let peak = at(peak_lag);
    let zero = at(0);
    let local_maximum = |lag: i64| {
        (lag == -max_lag || at(lag) >= at(lag - 1)) && (lag == max_lag || at(lag) >= at(lag + 1))
    };
    let secondary = (-max_lag..=max_lag)
        .filter(|&lag| (lag - peak_lag).abs() >= PERIODICITY_GUARD && local_maximum(lag))
        .max_by(|&left, &right| at(left).total_cmp(&at(right)));
    let periodic = secondary.is_some_and(|lag| at(lag) >= peak - PERIODICITY_TOLERANCE);
    let status = if peak < min_correlation {
        AlignmentStatus::Uncorrelated
    } else if periodic {
        if zero >= peak - PERIODICITY_TOLERANCE {
            AlignmentStatus::Periodic
        } else {
            AlignmentStatus::Offset
        }
    } else if peak_lag == 0 {
        AlignmentStatus::VerifiedZero
    } else {
        AlignmentStatus::Offset
    };
    Some(Alignment {
        segment_start: reference_start,
        segment_samples: u32::try_from(length).ok()?,
        peak_lag,
        peak_correlation: peak,
        zero_lag_correlation: zero,
        secondary_lag: secondary,
        secondary_correlation: secondary.map(at),
        status,
    })
}

/// Start offsets of up to `count` nonoverlapping `length`-sample segments with
/// the most reference energy, each at or above `floor_dbfs`, in time order.
pub fn loud_segments(
    samples: &[[f32; 2]],
    length: usize,
    count: usize,
    floor_dbfs: f64,
) -> Vec<usize> {
    let length = length.min(samples.len()).max(1);
    let mut candidates: Vec<(usize, f64)> = (0..samples.len())
        .step_by(length)
        .filter_map(|start| {
            let end = (start + length).min(samples.len());
            // A short tail segment overlaps its predecessor instead.
            let start = end.saturating_sub(length);
            let level = rms(&samples[start..end]);
            (dbfs(level) >= floor_dbfs).then_some((start, level))
        })
        .collect();
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    candidates.truncate(count);
    let mut starts: Vec<usize> = candidates.into_iter().map(|(start, _)| start).collect();
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Per-block comparison of one aligned window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct BlockSummary {
    /// Blocks whose reference level is at or above the comparison floor.
    pub compared: usize,
    /// Blocks whose reference is silent.
    pub silent: usize,
    pub min_snr_db: Option<f64>,
    pub min_snr_block_start: Option<i64>,
    pub max_level_difference_db: Option<f64>,
    pub max_silent_level_dbfs: Option<f64>,
    pub low_snr_blocks: usize,
    pub level_blocks: usize,
    pub sound_in_silence_blocks: usize,
}

pub struct BlockGates {
    pub block: usize,
    pub floor_dbfs: f64,
    pub silence_dbfs: f64,
    pub min_snr_db: f64,
    pub max_level_db: f64,
    pub silent_max_dbfs: f64,
}

/// Compare every block of `reference` with `decoded` (same length and
/// coordinates). Blocks between the silence threshold and the floor are only
/// counted by neither gate; their content is still part of window SNR.
pub fn compare_blocks(
    reference: &[[f32; 2]],
    decoded: &[[f32; 2]],
    start: i64,
    gates: &BlockGates,
) -> BlockSummary {
    let mut summary = BlockSummary::default();
    for (index, (left, right)) in reference
        .chunks(gates.block)
        .zip(decoded.chunks(gates.block))
        .enumerate()
    {
        let at = start + (index * gates.block) as i64;
        let level = dbfs(rms(left));
        let decoded_level = dbfs(rms(right));
        if level >= gates.floor_dbfs {
            summary.compared += 1;
            let error = error_rms(left, right);
            let snr =
                (20.0 * (rms(left) / error.max(f64::MIN_POSITIVE)).log10()).min(IDENTICAL_PSNR_DB);
            if summary.min_snr_db.is_none_or(|old| snr < old) {
                summary.min_snr_db = Some(snr);
                summary.min_snr_block_start = Some(at);
            }
            if snr < gates.min_snr_db {
                summary.low_snr_blocks += 1;
            }
            let difference = (decoded_level - level).abs();
            summary.max_level_difference_db = Some(
                summary
                    .max_level_difference_db
                    .map_or(difference, |old| old.max(difference)),
            );
            if difference > gates.max_level_db {
                summary.level_blocks += 1;
            }
        } else if level < gates.silence_dbfs {
            summary.silent += 1;
            summary.max_silent_level_dbfs = Some(
                summary
                    .max_silent_level_dbfs
                    .map_or(decoded_level, |old| old.max(decoded_level)),
            );
            if decoded_level > gates.silent_max_dbfs {
                summary.sound_in_silence_blocks += 1;
            }
        }
    }
    summary
}
