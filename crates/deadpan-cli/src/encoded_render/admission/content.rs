//! Fixed-coordinate content oracle for the bounded encoder probe.

use std::{
    fs::File,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_core::ColorPolicy;
use deadpan_source::{
    DecodeControl, DecodeLimits, SourceDecoder,
    audio::{AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
};
use serde::{Deserialize, Serialize};

use super::{ProbeGenerator, ProbeSpec};
use crate::render_worker::worker::check_control;

/// Frozen decoded-picture error limits in code values of one content schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentLimits {
    maximum: u16,
    mae_milli: u32,
    mse_milli: u32,
}

/// Schema 1: 8-bit SDR I420 codes.
const SDR_LIMITS: ContentLimits = ContentLimits {
    maximum: 48,
    mae_milli: 1_500,
    mse_milli: 16_000,
};
/// Schema 2: 10-bit HDR codes. The SDR limits scaled by 4 (codes) and 16
/// (squared codes), i.e. the same tolerance relative to full scale. Mirrored
/// in deadpan_jobs::render::admission::HDR_PROBE_CONTENT_LIMITS.
const HDR_LIMITS: ContentLimits = ContentLimits {
    maximum: 192,
    mae_milli: 6_000,
    mse_milli: 256_000,
};

fn schema(color: ColorPolicy) -> (u32, ContentLimits) {
    if color == ColorPolicy::SdrRec709 {
        (1, SDR_LIMITS)
    } else {
        (2, HDR_LIMITS)
    }
}
const MARKER_RADIUS: u64 = 64;
const MINIMUM_MARKER_PEAK: f32 = 0.15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeMarkerObservation {
    pub expected_samples: [u64; 2],
    pub observed_samples: [u64; 2],
    pub observed_peaks: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeContentReport {
    pub schema_version: u32,
    pub video_frames: u64,
    pub plane_samples: [u64; 3],
    /// Code values of the schema's bit depth (8-bit SDR, 10-bit HDR). The
    /// wider integer serializes identically for SDR evidence.
    pub maximum_plane_error: [u16; 3],
    pub absolute_plane_error: [u64; 3],
    pub squared_plane_error: [u64; 3],
    pub worst_frame_mean_absolute_error_milli: [u32; 3],
    pub worst_frame_mean_squared_error_milli: [u32; 3],
    pub audio_samples: u64,
    pub markers: [ProbeMarkerObservation; 3],
    pub unexpected_audio_peak: f32,
}

impl ProbeContentReport {
    pub fn validate(&self, spec: &ProbeSpec) -> Result<(), String> {
        let generator = spec.generator()?;
        let config = generator.config();
        let (version, limits) = schema(spec.color_policy);
        let pixels = u64::from(config.raster[0]) * u64::from(config.raster[1]);
        let plane_samples =
            [pixels, pixels / 4, pixels / 4].map(|samples| samples * config.video_frames);
        if self.schema_version != version
            || self.video_frames != config.video_frames
            || self.plane_samples != plane_samples
            || self.audio_samples != config.audio_samples
            || !self.unexpected_audio_peak.is_finite()
            || !(0.0..=MINIMUM_MARKER_PEAK).contains(&self.unexpected_audio_peak)
        {
            return Err("probe content report differs from its bounded fixture".into());
        }
        for plane in 0..3 {
            validate_plane(
                limits,
                self.plane_samples[plane],
                self.maximum_plane_error[plane],
                self.absolute_plane_error[plane],
                self.squared_plane_error[plane],
            )?;
            if self.worst_frame_mean_absolute_error_milli[plane] > limits.mae_milli
                || self.worst_frame_mean_squared_error_milli[plane] > limits.mse_milli
                || self.worst_frame_mean_absolute_error_milli[plane]
                    < milli(self.absolute_plane_error[plane], self.plane_samples[plane])?
                || self.worst_frame_mean_squared_error_milli[plane]
                    < milli(self.squared_plane_error[plane], self.plane_samples[plane])?
            {
                return Err("probe per-frame content errors exceed the fixed policy".into());
            }
        }
        for (observation, marker) in self.markers.iter().zip(generator.markers()) {
            if observation.expected_samples != marker.samples
                || observation.observed_samples != marker.samples
            {
                return Err("probe audio marker moved from its exact sample coordinate".into());
            }
            for (peak, expected) in observation.observed_peaks.iter().zip(marker.amplitudes) {
                if !peak.is_finite()
                    || peak.abs() < MINIMUM_MARKER_PEAK
                    || peak.is_sign_positive() != expected.is_sign_positive()
                {
                    return Err("probe audio marker is missing or changed channel/sign".into());
                }
            }
        }
        Ok(())
    }
}

pub(super) fn inspect(
    file: &File,
    spec: &ProbeSpec,
    maximum_bytes: u64,
    maximum_packets: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<ProbeContentReport, String> {
    let generator = spec.generator()?;
    let config = generator.config();
    let mut report = ProbeContentReport {
        schema_version: schema(spec.color_policy).0,
        video_frames: config.video_frames,
        plane_samples: [0; 3],
        maximum_plane_error: [0; 3],
        absolute_plane_error: [0; 3],
        squared_plane_error: [0; 3],
        worst_frame_mean_absolute_error_milli: [0; 3],
        worst_frame_mean_squared_error_milli: [0; 3],
        audio_samples: config.audio_samples,
        markers: std::array::from_fn(|index| ProbeMarkerObservation {
            expected_samples: generator.markers()[index].samples,
            observed_samples: [u64::MAX; 2],
            observed_peaks: [0.; 2],
        }),
        unexpected_audio_peak: 0.,
    };
    inspect_pictures(
        file,
        &generator,
        schema(spec.color_policy).1,
        maximum_bytes,
        maximum_packets,
        cancelled,
        deadline,
        &mut report,
    )?;
    inspect_audio(
        file,
        &generator,
        maximum_bytes,
        maximum_packets,
        cancelled,
        deadline,
        &mut report,
    )?;
    check_control(cancelled, deadline)?;
    report.validate(spec)?;
    Ok(report)
}

/// Declared CTA-861.3 light of a PQ probe, measured like the project host:
/// per-pixel max(R,G,B) in cd/m² before subsampling and coding, MaxCLL the
/// largest pixel and MaxFALL the largest frame mean, each rounded up to whole
/// cd/m². Every probe shape owns whole 2x2 luma cells with one Cb/Cr sample,
/// so each pixel's exact input R'G'B' is its cell's codes through the BT.2020
/// NCL matrix (clamped to [0, 1], then the PQ EOTF). The fixture's brightest
/// pixel is about 1004.2 cd/m², above the fixed HDR_PROBE_CONTENT_LIGHT
/// MaxCLL of 1000 that it would otherwise understate. SDR and HLG return None.
pub(super) fn declared_light(
    generator: &ProbeGenerator,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Option<deadpan_encode::ContentLight>, String> {
    let ProbeGenerator::Hdr(probe) = generator else {
        return Ok(None);
    };
    if probe.transfer() != deadpan_encode::HdrTransfer::Pq {
        return Ok(None);
    }
    let config = generator.config();
    let width = usize::try_from(config.raster[0]).map_err(|_| "probe width overflow")?;
    let height = usize::try_from(config.raster[1]).map_err(|_| "probe height overflow")?;
    let (columns, rows) = (width / 2, height / 2);
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(config.picture_bytes).map_err(|_| "probe bytes")?)
        .map_err(|error| error.to_string())?;
    bytes.resize(bytes.capacity(), 0);
    let code = |bytes: &[u8], index: usize| {
        f64::from(u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]))
    };
    // A probe uses only a handful of distinct cell codes; cache their light.
    let mut cache = std::collections::HashMap::<[u64; 3], f64>::new();
    let (mut max_cll, mut max_fall) = (0.0_f64, 0.0_f64);
    for ordinal in 0..config.video_frames {
        check_control(cancelled, deadline)?;
        generator
            .fill_picture(ordinal, &mut bytes)
            .map_err(|error| error.to_string())?;
        let (cb_base, cr_base) = (width * height, width * height + columns * rows);
        let mut total = 0.0_f64;
        for row in 0..rows {
            for column in 0..columns {
                let site = row * columns + column;
                let [cb, cr] = [cb_base, cr_base].map(|base| code(&bytes, base + site));
                for [x, y] in [[0, 0], [1, 0], [0, 1], [1, 1]] {
                    let luma = code(&bytes, (row * 2 + y) * width + column * 2 + x);
                    let light = *cache
                        .entry([luma, cb, cr].map(f64::to_bits))
                        .or_insert_with(|| {
                            let luma = (luma - 64.0) / 876.0;
                            let (cb, cr) = ((cb - 512.0) / 896.0, (cr - 512.0) / 896.0);
                            let red = luma + 2.0 * (1.0 - 0.2627) * cr;
                            let blue = luma + 2.0 * (1.0 - 0.0593) * cb;
                            let green =
                                (luma - 0.2627 * red - 0.0593 * blue) / (1.0 - 0.2627 - 0.0593);
                            deadpan_render::pq_eotf(red.max(green).max(blue).clamp(0.0, 1.0))
                        });
                    max_cll = max_cll.max(light);
                    total += light;
                }
            }
        }
        // Exact for raster sizes below 2^53 pixels.
        #[allow(clippy::cast_precision_loss)]
        let pixels = (width * height) as f64;
        max_fall = max_fall.max(total / pixels);
    }
    // Bounded to [0, 10000] before conversion.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = |value: f64| value.ceil().clamp(0.0, 10_000.0) as u16;
    let max_cll = whole(max_cll);
    Ok(Some(deadpan_encode::ContentLight {
        max_cll,
        max_fall: whole(max_fall).min(max_cll),
    }))
}

fn control(cancelled: &AtomicBool, deadline: Instant) -> Result<DecodeControl<'_>, String> {
    check_control(cancelled, deadline)?;
    Ok(DecodeControl {
        cancelled,
        timeout: deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
    })
}

#[allow(clippy::too_many_arguments)]
fn inspect_pictures(
    file: &File,
    generator: &ProbeGenerator,
    limits_policy: ContentLimits,
    maximum_bytes: u64,
    maximum_packets: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
    report: &mut ProbeContentReport,
) -> Result<(), String> {
    let config = generator.config();
    let pixels = crate::encoded_render::verification::decode_pixel_budget(
        config.raster,
        matches!(generator, ProbeGenerator::Hdr(_)),
    );
    let limits = DecodeLimits {
        progressive_only: true,
        max_input_bytes: maximum_bytes,
        max_frames: config.video_frames + 1,
        max_packets: maximum_packets + 1,
        max_pixels: pixels,
        ..DecodeLimits::default()
    };
    let mut decoder = SourceDecoder::open(
        file.try_clone().map_err(|error| error.to_string())?,
        limits,
        control(cancelled, deadline)?,
    )
    .map_err(|error| error.to_string())?;
    let length =
        usize::try_from(config.picture_bytes).map_err(|_| "probe picture exceeds address space")?;
    let mut input = Vec::new();
    input
        .try_reserve_exact(length)
        .map_err(|error| error.to_string())?;
    input.resize(length, 0);
    let luma = usize::try_from(u64::from(config.raster[0]) * u64::from(config.raster[1]))
        .map_err(|_| "probe raster exceeds address space")?;
    let samples_per_picture = luma + luma / 2;
    let boundaries = [0, luma, luma + luma / 4, samples_per_picture];
    let hdr = matches!(generator, ProbeGenerator::Hdr(_));
    let mut expected: Vec<u16> = Vec::new();
    expected
        .try_reserve_exact(samples_per_picture)
        .map_err(|error| error.to_string())?;
    let mut actual: Vec<u16> = Vec::new();
    actual
        .try_reserve_exact(samples_per_picture)
        .map_err(|error| error.to_string())?;
    for ordinal in 0..config.video_frames {
        check_control(cancelled, deadline)?;
        generator
            .fill_picture(ordinal, &mut input)
            .map_err(|error| error.to_string())?;
        expected.clear();
        if hdr {
            expected.extend(
                input
                    .chunks_exact(2)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]])),
            );
        } else {
            expected.extend(input.iter().copied().map(u16::from));
        }
        actual.clear();
        let (width, height, metadata) = if hdr {
            let frame = decoder
                .next_yuv420p10(control(cancelled, deadline)?)
                .map_err(|error| error.to_string())?
                .ok_or("probe picture decode ended early")?;
            actual.extend_from_slice(&frame.samples);
            (frame.width, frame.height, frame.metadata)
        } else {
            let frame = decoder
                .next_i420(control(cancelled, deadline)?)
                .map_err(|error| error.to_string())?
                .ok_or("probe picture decode ended early")?;
            actual.extend(frame.i420.iter().copied().map(u16::from));
            (frame.width, frame.height, frame.metadata)
        };
        let pts = i64::try_from(ordinal * u64::from(config.frame_rate[1]))
            .map_err(|_| "probe picture clock overflow")?;
        if [width, height] != config.raster
            || expected.len() != samples_per_picture
            || actual.len() != samples_per_picture
            || metadata.source.pts != pts
            || metadata.source.reported_duration != Some(i64::from(config.frame_rate[1]))
            || metadata.decode_error_flags != 0
            || metadata.corrupt
        {
            return Err("probe content decode changed picture identity".into());
        }
        for plane in 0..3 {
            let range = boundaries[plane]..boundaries[plane + 1];
            let (maximum, absolute, squared) = errors(
                &expected[range.clone()],
                &actual[range],
                cancelled,
                deadline,
            )?;
            let samples = u64::try_from(boundaries[plane + 1] - boundaries[plane])
                .map_err(|_| "probe plane exceeds address space")?;
            validate_plane(limits_policy, samples, maximum, absolute, squared)
                .map_err(|error| format!("{error}; frame={ordinal}, plane={plane}, max={maximum}, absolute={absolute}, squared={squared}, samples={samples}"))?;
            report.plane_samples[plane] += samples;
            report.maximum_plane_error[plane] = report.maximum_plane_error[plane].max(maximum);
            report.absolute_plane_error[plane] += absolute;
            report.squared_plane_error[plane] += squared;
            report.worst_frame_mean_absolute_error_milli[plane] =
                report.worst_frame_mean_absolute_error_milli[plane].max(milli(absolute, samples)?);
            report.worst_frame_mean_squared_error_milli[plane] =
                report.worst_frame_mean_squared_error_milli[plane].max(milli(squared, samples)?);
        }
    }
    let extra = if hdr {
        decoder
            .next_yuv420p10(control(cancelled, deadline)?)
            .map_err(|error| error.to_string())?
            .is_some()
    } else {
        decoder
            .next_i420(control(cancelled, deadline)?)
            .map_err(|error| error.to_string())?
            .is_some()
    };
    if extra {
        return Err("probe content decode has extra pictures".into());
    }
    Ok(())
}

fn errors(
    expected: &[u16],
    actual: &[u16],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(u16, u64, u64), String> {
    if expected.len() != actual.len() || expected.is_empty() {
        return Err("probe plane lengths differ".into());
    }
    let mut maximum = 0;
    let mut absolute = 0;
    let mut squared = 0;
    for (expected, actual) in expected.chunks(64 * 1024).zip(actual.chunks(64 * 1024)) {
        check_control(cancelled, deadline)?;
        for (&expected, &actual) in expected.iter().zip(actual) {
            let difference = expected.abs_diff(actual);
            maximum = maximum.max(difference);
            absolute += u64::from(difference);
            squared += u64::from(difference).pow(2);
        }
    }
    Ok((maximum, absolute, squared))
}

fn milli(error: u64, samples: u64) -> Result<u32, String> {
    if samples == 0 {
        return Err("probe plane has no samples".into());
    }
    u32::try_from((u128::from(error) * 1_000).div_ceil(u128::from(samples)))
        .map_err(|_| "probe error statistic overflow".into())
}

fn validate_plane(
    limits: ContentLimits,
    samples: u64,
    maximum: u16,
    absolute: u64,
    squared: u64,
) -> Result<(), String> {
    if maximum > limits.maximum
        || milli(absolute, samples)? > limits.mae_milli
        || milli(squared, samples)? > limits.mse_milli
        || u128::from(absolute) > u128::from(samples) * u128::from(maximum)
        || u128::from(squared) > u128::from(absolute) * u128::from(maximum)
        || squared < absolute
        || absolute < u64::from(maximum)
        || squared < u64::from(maximum).pow(2)
        || u128::from(absolute).pow(2) > u128::from(samples) * u128::from(squared)
    {
        return Err("probe decoded pixels exceed fixed content-error limits".into());
    }
    Ok(())
}

fn inspect_audio(
    file: &File,
    generator: &ProbeGenerator,
    maximum_bytes: u64,
    maximum_packets: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
    report: &mut ProbeContentReport,
) -> Result<(), String> {
    let samples = generator.config().audio_samples;
    let limits = AudioDecodeLimits {
        max_input_bytes: maximum_bytes,
        max_frames: samples.div_ceil(1024) + 2,
        max_packets: maximum_packets,
        max_decoded_samples: (samples.div_ceil(1024) + 1) * 1024,
        max_samples_per_frame: 2048,
        max_channels: 2,
        max_sample_rate: 48_000,
        ..AudioDecodeLimits::default()
    };
    let mut decoder = AudioDecoder::open_first_with_mode(
        file.try_clone().map_err(|error| error.to_string())?,
        AudioDecodeMode::Ordinary,
        limits,
        control(cancelled, deadline)?,
    )
    .map_err(|error| error.to_string())?;
    let mut next = 0_u64;
    while let Some(metadata) = decoder
        .next_metadata(control(cancelled, deadline)?)
        .map_err(|error| error.to_string())?
    {
        if u64::try_from(metadata.pts).ok() != Some(next) || metadata.nb_samples != 1024 {
            return Err("probe audio content decoder changed its sample clock".into());
        }
        let frame = decoder
            .copy_current_interleaved_f32(control(cancelled, deadline)?)
            .map_err(|error| error.to_string())?;
        if frame.metadata != metadata
            || frame.samples.len() != 2048
            || !frame.samples.iter().all(|sample| sample.is_finite())
        {
            return Err("probe audio content decoder changed finite stereo samples".into());
        }
        for (offset, sample) in frame.samples.chunks_exact(2).enumerate() {
            let coordinate = next + u64::try_from(offset).map_err(|_| "probe sample overflow")?;
            if coordinate >= samples {
                continue;
            }
            for (channel, &value) in sample.iter().enumerate() {
                observe_sample(report, generator, coordinate, channel, value);
            }
        }
        next += 1024;
    }
    if next != samples.div_ceil(1024) * 1024 {
        return Err("probe content decode omitted audio or its drain".into());
    }
    Ok(())
}

fn observe_sample(
    report: &mut ProbeContentReport,
    generator: &ProbeGenerator,
    coordinate: u64,
    channel: usize,
    value: f32,
) {
    let mut in_marker = false;
    for (observation, marker) in report.markers.iter_mut().zip(generator.markers()) {
        if coordinate.abs_diff(marker.samples[channel]) <= MARKER_RADIUS {
            in_marker = true;
            if value.abs() > observation.observed_peaks[channel].abs() {
                observation.observed_peaks[channel] = value;
                observation.observed_samples[channel] = coordinate;
            }
        }
    }
    if !in_marker {
        report.unexpected_audio_peak = report.unexpected_audio_peak.max(value.abs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (ProbeSpec, ProbeContentReport) {
        let spec = ProbeSpec {
            raster: [320, 180],
            frame_rate: [60, 1],
            choice: crate::encoded_render::protocol::EncoderChoice {
                mode: deadpan_encode::EncoderMode::Hardware,
                b_frames: deadpan_encode::BFramePolicy::None,
            },
            color_policy: ColorPolicy::SdrRec709,
        };
        let generator = spec.generator().unwrap();
        let config = generator.config();
        let pixels = u64::from(config.raster[0]) * u64::from(config.raster[1]);
        let report = ProbeContentReport {
            schema_version: 1,
            video_frames: config.video_frames,
            plane_samples: [pixels, pixels / 4, pixels / 4]
                .map(|count| count * config.video_frames),
            maximum_plane_error: [0; 3],
            absolute_plane_error: [0; 3],
            squared_plane_error: [0; 3],
            worst_frame_mean_absolute_error_milli: [0; 3],
            worst_frame_mean_squared_error_milli: [0; 3],
            audio_samples: config.audio_samples,
            markers: std::array::from_fn(|index| ProbeMarkerObservation {
                expected_samples: generator.markers()[index].samples,
                observed_samples: generator.markers()[index].samples,
                observed_peaks: generator.markers()[index].amplitudes,
            }),
            unexpected_audio_peak: 0.,
        };
        (spec, report)
    }

    #[test]
    fn content_error_limits_reject_bad_single_planes_and_inconsistent_claims() {
        let limits = SDR_LIMITS;
        assert!(validate_plane(limits, 1000, 4, 1500, 4000).is_ok());
        assert!(validate_plane(limits, 1000, 49, 49, 2401).is_err());
        assert!(validate_plane(limits, 1000, 4, 1501, 4000).is_err());
        assert!(validate_plane(limits, 1000, 48, 1000, 16001).is_err());
        assert!(validate_plane(limits, 1000, 0, 1, 1).is_err());
        assert!(validate_plane(limits, 1000, 3, 1000, 999).is_err());
        // 10-bit HDR codes: four times the code range, sixteen times squared.
        assert!(validate_plane(HDR_LIMITS, 1000, 192, 6000, 256_000).is_ok());
        assert!(validate_plane(HDR_LIMITS, 1000, 193, 193, 37_249).is_err());
        assert!(validate_plane(HDR_LIMITS, 1000, 16, 6001, 96_016).is_err());
        assert!(validate_plane(HDR_LIMITS, 1000, 192, 1400, 256_001).is_err());
    }

    #[test]
    fn exact_plane_comparison_observes_cancellation() {
        let cancelled = AtomicBool::new(true);
        assert!(
            errors(
                &[16; 4],
                &[16; 4],
                &cancelled,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
    }

    #[test]
    fn content_report_rejects_shifted_missing_swapped_and_unexpected_audio() {
        let (spec, report) = fixture();
        report.validate(&spec).unwrap();
        let mut shifted = report.clone();
        shifted.markers[0].observed_samples[0] += 1;
        assert!(shifted.validate(&spec).is_err());
        let mut missing = report.clone();
        missing.markers[2].observed_peaks[1] = 0.;
        assert!(missing.validate(&spec).is_err());
        let mut swapped = report.clone();
        swapped.markers[1].observed_peaks.swap(0, 1);
        assert!(swapped.validate(&spec).is_err());
        let mut unexpected = report.clone();
        unexpected.unexpected_audio_peak = 0.16;
        assert!(unexpected.validate(&spec).is_err());
        let mut nonfinite = report;
        nonfinite.markers[0].observed_peaks[0] = f32::NAN;
        assert!(nonfinite.validate(&spec).is_err());
    }

    #[test]
    fn worst_frame_errors_cannot_be_hidden_by_whole_movie_averages() {
        let (spec, mut report) = fixture();
        report.worst_frame_mean_squared_error_milli[2] = SDR_LIMITS.mse_milli + 1;
        assert!(report.validate(&spec).is_err());
        let (spec, mut report) = fixture();
        report.worst_frame_mean_absolute_error_milli[0] = SDR_LIMITS.mae_milli + 1;
        assert!(report.validate(&spec).is_err());
    }

    #[test]
    fn marker_observation_never_realigns_the_expected_coordinate() {
        let (spec, mut report) = fixture();
        let generator = spec.generator().unwrap();
        report.markers[0].observed_peaks[0] = 0.;
        let expected = generator.markers()[0].samples[0];
        observe_sample(&mut report, &generator, expected + 1, 0, 0.75);
        assert_eq!(report.markers[0].expected_samples[0], expected);
        assert_eq!(report.markers[0].observed_samples[0], expected + 1);
        assert!(report.validate(&spec).is_err());
    }
}
