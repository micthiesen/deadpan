//! Fixed-coordinate content oracle for the bounded encoder probe.

use std::{
    fs::File,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_encode::probe::EncoderProbe;
use deadpan_source::{
    DecodeControl, DecodeLimits, SourceDecoder,
    audio::{AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
};
use serde::{Deserialize, Serialize};

use super::ProbeSpec;
use crate::render_worker::worker::check_control;

const MAXIMUM_PLANE_ERROR: u8 = 48;
const MAXIMUM_MAE_MILLI: u32 = 1_500;
const MAXIMUM_MSE_MILLI: u32 = 16_000;
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
    pub maximum_plane_error: [u8; 3],
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
        let pixels = u64::from(config.raster[0]) * u64::from(config.raster[1]);
        let plane_samples =
            [pixels, pixels / 4, pixels / 4].map(|samples| samples * config.video_frames);
        if self.schema_version != 1
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
                self.plane_samples[plane],
                self.maximum_plane_error[plane],
                self.absolute_plane_error[plane],
                self.squared_plane_error[plane],
            )?;
            if self.worst_frame_mean_absolute_error_milli[plane] > MAXIMUM_MAE_MILLI
                || self.worst_frame_mean_squared_error_milli[plane] > MAXIMUM_MSE_MILLI
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
        schema_version: 1,
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

fn control(cancelled: &AtomicBool, deadline: Instant) -> Result<DecodeControl<'_>, String> {
    check_control(cancelled, deadline)?;
    Ok(DecodeControl {
        cancelled,
        timeout: deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(60)),
    })
}

fn inspect_pictures(
    file: &File,
    generator: &EncoderProbe,
    maximum_bytes: u64,
    maximum_packets: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
    report: &mut ProbeContentReport,
) -> Result<(), String> {
    let config = generator.config();
    let pixels = u64::from(config.raster[0]).div_ceil(16)
        * 16
        * u64::from(config.raster[1]).div_ceil(16)
        * 16;
    let limits = DecodeLimits {
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
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(length)
        .map_err(|error| error.to_string())?;
    expected.resize(length, 0);
    let luma = usize::try_from(u64::from(config.raster[0]) * u64::from(config.raster[1]))
        .map_err(|_| "probe raster exceeds address space")?;
    let boundaries = [0, luma, luma + luma / 4, length];
    for ordinal in 0..config.video_frames {
        check_control(cancelled, deadline)?;
        generator
            .fill_picture(ordinal, &mut expected)
            .map_err(|error| error.to_string())?;
        let frame = decoder
            .next_i420(control(cancelled, deadline)?)
            .map_err(|error| error.to_string())?
            .ok_or("probe picture decode ended early")?;
        let pts = i64::try_from(ordinal * u64::from(config.frame_rate[1]))
            .map_err(|_| "probe picture clock overflow")?;
        if [frame.width, frame.height] != config.raster
            || frame.i420.len() != length
            || frame.metadata.source.pts != pts
            || frame.metadata.source.reported_duration != Some(i64::from(config.frame_rate[1]))
            || frame.metadata.decode_error_flags != 0
            || frame.metadata.corrupt
        {
            return Err("probe content decode changed picture identity".into());
        }
        for plane in 0..3 {
            let range = boundaries[plane]..boundaries[plane + 1];
            let (maximum, absolute, squared) = errors(
                &expected[range.clone()],
                &frame.i420[range],
                cancelled,
                deadline,
            )?;
            let samples = u64::try_from(boundaries[plane + 1] - boundaries[plane])
                .map_err(|_| "probe plane exceeds address space")?;
            validate_plane(samples, maximum, absolute, squared)
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
    if decoder
        .next_i420(control(cancelled, deadline)?)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("probe content decode has extra pictures".into());
    }
    Ok(())
}

fn errors(
    expected: &[u8],
    actual: &[u8],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(u8, u64, u64), String> {
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

fn validate_plane(samples: u64, maximum: u8, absolute: u64, squared: u64) -> Result<(), String> {
    if maximum > MAXIMUM_PLANE_ERROR
        || milli(absolute, samples)? > MAXIMUM_MAE_MILLI
        || milli(squared, samples)? > MAXIMUM_MSE_MILLI
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
    generator: &EncoderProbe,
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
    generator: &EncoderProbe,
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
        assert!(validate_plane(1000, 4, 1500, 4000).is_ok());
        assert!(validate_plane(1000, 49, 49, 2401).is_err());
        assert!(validate_plane(1000, 4, 1501, 4000).is_err());
        assert!(validate_plane(1000, 48, 1000, 16001).is_err());
        assert!(validate_plane(1000, 0, 1, 1).is_err());
        assert!(validate_plane(1000, 3, 1000, 999).is_err());
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
        report.worst_frame_mean_squared_error_milli[2] = MAXIMUM_MSE_MILLI + 1;
        assert!(report.validate(&spec).is_err());
        let (spec, mut report) = fixture();
        report.worst_frame_mean_absolute_error_milli[0] = MAXIMUM_MAE_MILLI + 1;
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
