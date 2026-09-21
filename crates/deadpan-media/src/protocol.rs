//! Small versioned wire contract for the app-managed codec helper.
//!
//! Deserialization checks structure; both endpoints must call `validate` before
//! using numeric fields. The contract deliberately supports only the qualified
//! full-range RGB8 / sRGB / BT.709 generated-video route.

use deadpan_core::{BridgeSamplingMap, SourceSpan, SourceTimeBase, SourceTimestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
/// Adds host-authoritative interior sampling; version-1 conversion is unchanged.
pub const BRIDGE_PROTOCOL_VERSION: u32 = 2;
/// Adds the measured duration of the final decoded output frame to reports.
pub const REPORT_PROTOCOL_VERSION: u32 = 2;
pub const MAX_REPLY_BYTES: usize = 8192;
pub const MAX_REQUEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoContract {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub rate_num: u32,
    pub rate_den: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid media contract: {0}")]
pub struct ContractError(pub &'static str);

impl VideoContract {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.width == 0 || self.height == 0 || self.width > 4096 || self.height > 4096 {
            return Err(ContractError("dimensions must be between 1 and 4096"));
        }
        if self.frames == 0 || self.frames > 10_000 {
            return Err(ContractError("frame count must be between 1 and 10000"));
        }
        if self.rate_num == 0
            || self.rate_den == 0
            || self.rate_num > i32::MAX as u32
            || self.rate_den > i32::MAX as u32
            || self.rate_num < self.rate_den
            || u64::from(self.rate_num) > 240 * u64::from(self.rate_den)
        {
            return Err(ContractError(
                "frame rate must be representable and between 1 and 240",
            ));
        }
        let mut a = self.rate_num;
        let mut b = self.rate_den;
        while b != 0 {
            (a, b) = (b, a % b);
        }
        if a != 1 {
            return Err(ContractError("frame rate must be reduced"));
        }
        Ok(())
    }

    pub fn scratch_bytes(&self) -> Result<u64, ContractError> {
        self.validate()?;
        Ok(u64::from(self.width) * u64::from(self.height) * 3 * u64::from(self.frames))
    }

    /// Exact CFR positions rounded once to Matroska's millisecond clock.
    pub fn matroska_pts(&self, ordinal: u32) -> Result<i64, ContractError> {
        self.validate()?;
        if ordinal >= self.frames {
            return Err(ContractError("frame ordinal is outside the sequence"));
        }
        let numerator = u64::from(ordinal) * u64::from(self.rate_den) * 1000;
        i64::try_from((numerator + u64::from(self.rate_num) / 2) / u64::from(self.rate_num))
            .map_err(|_| ContractError("timestamp overflow"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionLimits {
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_scratch_bytes: u64,
    pub timeout_ms: u64,
}

impl ConversionLimits {
    pub fn validate(&self) -> Result<(), ContractError> {
        if [
            self.max_input_bytes,
            self.max_output_bytes,
            self.max_scratch_bytes,
        ]
        .into_iter()
        .any(|bytes| bytes == 0 || bytes > MAX_FILE_BYTES)
        {
            return Err(ContractError(
                "file budgets must be positive and at most 16 GiB",
            ));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 24 * 60 * 60 * 1000 {
            return Err(ContractError(
                "deadline must be positive and at most 24 hours",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionRequest {
    pub protocol: u32,
    pub video: VideoContract,
    pub input_byte_length: u64,
    pub limits: ConversionLimits,
}

impl ConversionRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ContractError("unsupported protocol version"));
        }
        self.video.validate()?;
        self.limits.validate()?;
        if self.input_byte_length == 0 || self.input_byte_length > self.limits.max_input_bytes {
            return Err(ContractError("input length exceeds budget or is empty"));
        }
        if self.video.scratch_bytes()? > self.limits.max_scratch_bytes {
            return Err(ContractError("decoded sequence exceeds scratch budget"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeOperation {
    SampleBridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConversionRequest {
    pub protocol: u32,
    pub operation: BridgeOperation,
    pub native: VideoContract,
    pub sampling: BridgeSamplingMap,
    pub input_byte_length: u64,
    pub limits: ConversionLimits,
}

impl BridgeConversionRequest {
    pub fn output_video(&self) -> Result<VideoContract, ContractError> {
        let video = VideoContract {
            width: self.native.width,
            height: self.native.height,
            frames: u32::try_from(self.sampling.output_frame_count().frames())
                .map_err(|_| ContractError("output frame count is not representable"))?,
            rate_num: self.sampling.project_rate().numerator(),
            rate_den: self.sampling.project_rate().denominator(),
        };
        video.validate()?;
        Ok(video)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.protocol != BRIDGE_PROTOCOL_VERSION {
            return Err(ContractError("unsupported bridge protocol version"));
        }
        self.native.validate()?;
        self.output_video()?;
        if self.sampling.native_frame_count().frames() != i64::from(self.native.frames)
            || self.sampling.native_rate().numerator() != self.native.rate_num
            || self.sampling.native_rate().denominator() != self.native.rate_den
        {
            return Err(ContractError("sampling map does not match native video"));
        }
        // The decoder stores only native frames. Output frames are computed one
        // at a time, so upsampling does not multiply the scratch requirement.
        ConversionRequest {
            protocol: PROTOCOL_VERSION,
            video: self.native,
            input_byte_length: self.input_byte_length,
            limits: self.limits,
        }
        .validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkerRequest {
    Convert(ConversionRequest),
    Bridge(BridgeConversionRequest),
}

impl WorkerRequest {
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Convert(request) => request.validate(),
            Self::Bridge(request) => request.validate(),
        }
    }

    pub fn native_video(&self) -> VideoContract {
        match self {
            Self::Convert(request) => request.video,
            Self::Bridge(request) => request.native,
        }
    }

    pub fn output_video(&self) -> Result<VideoContract, ContractError> {
        match self {
            Self::Convert(request) => Ok(request.video),
            Self::Bridge(request) => request.output_video(),
        }
    }

    pub fn input_byte_length(&self) -> u64 {
        match self {
            Self::Convert(request) => request.input_byte_length,
            Self::Bridge(request) => request.input_byte_length,
        }
    }

    pub fn limits(&self) -> ConversionLimits {
        match self {
            Self::Convert(request) => request.limits,
            Self::Bridge(request) => request.limits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionReport {
    pub protocol: u32,
    pub video: VideoContract,
    pub output_bytes: u64,
    pub input_rgb_sha256: String,
    pub output_rgb_sha256: String,
    pub input_time_base_num: u32,
    pub input_time_base_den: u32,
    pub output_time_base_num: u32,
    pub output_time_base_den: u32,
    pub first_output_pts: i64,
    pub last_output_pts: i64,
    pub last_output_duration: i64,
    pub ffv1_version: u32,
    pub slice_crc: bool,
    pub discarded_audio_streams: u32,
}

impl ConversionReport {
    /// Returns the exact half-open bounds observed by the verification decoder.
    ///
    /// This is deliberately independent from the requested project duration.
    /// The final boundary is the checked sum of the final decoded PTS and its
    /// measured positive duration in the reported output time base.
    pub fn output_span(&self) -> Result<SourceSpan, ContractError> {
        self.validate_observed_output_clock()?;
        let time_base = SourceTimeBase::new(self.output_time_base_num, self.output_time_base_den)
            .map_err(|_| ContractError("invalid observed media clocks"))?;
        let end = self
            .last_output_pts
            .checked_add(self.last_output_duration)
            .ok_or(ContractError("output span overflow"))?;
        SourceSpan::new(
            SourceTimestamp {
                ticks: self.first_output_pts,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .map_err(|_| ContractError("invalid observed output span"))
    }

    pub fn validate(&self, request: &ConversionRequest) -> Result<(), ContractError> {
        self.validate_worker(&WorkerRequest::Convert(request.clone()))
    }

    pub fn validate_worker(&self, request: &WorkerRequest) -> Result<(), ContractError> {
        request.validate()?;
        if self.protocol != REPORT_PROTOCOL_VERSION {
            return Err(ContractError("unsupported report protocol version"));
        }
        if self.video != request.output_video()? {
            return Err(ContractError("worker changed the requested video contract"));
        }
        if self.output_bytes == 0 || self.output_bytes > request.limits().max_output_bytes {
            return Err(ContractError("output size is empty or exceeds budget"));
        }
        if !is_sha256(&self.input_rgb_sha256)
            || !is_sha256(&self.output_rgb_sha256)
            || (matches!(request, WorkerRequest::Convert(_))
                && self.input_rgb_sha256 != self.output_rgb_sha256)
        {
            return Err(ContractError(
                "decoded pixel identities differ or are invalid",
            ));
        }
        if self.input_time_base_num == 0
            || self.input_time_base_den == 0
            || self.input_time_base_num > i32::MAX as u32
            || self.input_time_base_den > i32::MAX as u32
        {
            return Err(ContractError("invalid observed media clocks"));
        }
        self.output_span()?;
        if self.ffv1_version != 3 || !self.slice_crc || self.discarded_audio_streams > 7 {
            return Err(ContractError("unexpected codec profile or stream count"));
        }
        Ok(())
    }

    fn validate_observed_output_clock(&self) -> Result<(), ContractError> {
        if self.protocol != REPORT_PROTOCOL_VERSION {
            return Err(ContractError("unsupported report protocol version"));
        }
        self.video.validate()?;
        let expected_duration = i64::try_from(
            u64::from(self.video.rate_den)
                .checked_mul(1000)
                .ok_or(ContractError("output duration overflow"))?
                / u64::from(self.video.rate_num),
        )
        .map_err(|_| ContractError("output duration overflow"))?;
        if self.output_time_base_num != 1
            || self.output_time_base_den != 1000
            || self.first_output_pts != 0
            || self.last_output_pts != self.video.matroska_pts(self.video.frames - 1)?
            || self.last_output_duration <= 0
            || self.last_output_duration != expected_duration
        {
            return Err(ContractError("invalid observed media clocks"));
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerReply {
    Success { report: ConversionReport },
    Failure { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{BridgeInterpolation, FrameDuration, FrameRate};

    fn report(video: VideoContract) -> ConversionReport {
        ConversionReport {
            protocol: REPORT_PROTOCOL_VERSION,
            video,
            output_bytes: 1024,
            input_rgb_sha256: "a".repeat(64),
            output_rgb_sha256: "a".repeat(64),
            input_time_base_num: 1,
            input_time_base_den: video.rate_num,
            output_time_base_num: 1,
            output_time_base_den: 1000,
            first_output_pts: 0,
            last_output_pts: video.matroska_pts(video.frames - 1).unwrap(),
            last_output_duration: i64::from((video.rate_den * 1000) / video.rate_num),
            ffv1_version: 3,
            slice_crc: true,
            discarded_audio_streams: 0,
        }
    }

    fn conversion(video: VideoContract) -> ConversionRequest {
        ConversionRequest {
            protocol: PROTOCOL_VERSION,
            video,
            input_byte_length: 1024,
            limits: ConversionLimits {
                max_input_bytes: 1024,
                max_output_bytes: 1024,
                max_scratch_bytes: video.scratch_bytes().unwrap(),
                timeout_ms: 5000,
            },
        }
    }

    fn bridge() -> BridgeConversionRequest {
        BridgeConversionRequest {
            protocol: BRIDGE_PROTOCOL_VERSION,
            operation: BridgeOperation::SampleBridge,
            native: VideoContract {
                frames: 25,
                rate_num: 24,
                rate_den: 1,
                ..video()
            },
            sampling: BridgeSamplingMap::new(
                FrameRate::new(30000, 1001).unwrap(),
                FrameRate::new(24, 1).unwrap(),
                FrameDuration::new(25).unwrap(),
                FrameDuration::new(30).unwrap(),
                BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
            )
            .unwrap(),
            input_byte_length: 1024,
            limits: ConversionLimits {
                max_input_bytes: 1024,
                max_output_bytes: 1024 * 1024,
                max_scratch_bytes: 768 * 320 * 3 * 25,
                timeout_ms: 5000,
            },
        }
    }

    fn video() -> VideoContract {
        VideoContract {
            width: 768,
            height: 320,
            frames: 30,
            rate_num: 30000,
            rate_den: 1001,
        }
    }

    #[test]
    fn exact_clock_rounds_each_origin_based_position_once() {
        let video = video();
        assert_eq!(video.matroska_pts(0).unwrap(), 0);
        assert_eq!(video.matroska_pts(29).unwrap(), 968);
        assert_eq!(video.scratch_bytes().unwrap(), 22_118_400);
        assert!(video.matroska_pts(30).is_err());
        assert!(
            VideoContract {
                rate_num: 60,
                rate_den: 2,
                ..video
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoContract {
                rate_num: 0,
                ..video
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoContract {
                width: 4097,
                ..video
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn wire_rejects_unknown_and_duplicate_fields() {
        let wire = serde_json::to_string(&video()).unwrap();
        assert!(
            serde_json::from_str::<VideoContract>(&wire.replacen('{', "{\"width\":1,", 1)).is_err()
        );
        assert!(
            serde_json::from_str::<VideoContract>(&wire.replacen('{', "{\"path\":\"evil\",", 1))
                .is_err()
        );
    }

    #[test]
    fn bridge_validates_native_mapping_and_only_native_scratch() {
        let request = bridge();
        request.validate().unwrap();
        assert_eq!(request.output_video().unwrap(), video());
        assert!(
            request.output_video().unwrap().scratch_bytes().unwrap()
                > request.limits.max_scratch_bytes
        );
        let mut mismatch = request.clone();
        mismatch.native.frames = 24;
        assert!(mismatch.validate().is_err());
        mismatch = request.clone();
        mismatch.native.rate_num = 25;
        assert!(mismatch.validate().is_err());
        mismatch = request;
        mismatch.limits.max_scratch_bytes -= 1;
        assert!(mismatch.validate().is_err());
    }

    #[test]
    fn versioned_requests_reject_ambiguous_or_mismatched_wire_fields() {
        let request = WorkerRequest::Bridge(bridge());
        let wire = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&wire).unwrap(),
            request
        );
        for extra in ["\"native\":{},", "\"video\":{},", "\"protocol\":2,"] {
            assert!(
                serde_json::from_str::<WorkerRequest>(&wire.replacen(
                    '{',
                    &format!("{{{extra}"),
                    1
                ))
                .is_err()
            );
        }
        let mut wrong_version = bridge();
        wrong_version.protocol = PROTOCOL_VERSION;
        assert!(WorkerRequest::Bridge(wrong_version).validate().is_err());
        let conversion = ConversionRequest {
            protocol: PROTOCOL_VERSION,
            video: bridge().native,
            input_byte_length: 1024,
            limits: bridge().limits,
        };
        let wire = serde_json::to_string(&conversion).unwrap();
        assert_eq!(
            serde_json::from_str::<WorkerRequest>(&wire).unwrap(),
            WorkerRequest::Convert(conversion)
        );
    }

    #[test]
    fn report_span_uses_measured_final_duration_instead_of_next_rounded_boundary() {
        let contract = VideoContract {
            width: 4,
            height: 2,
            frames: 25,
            rate_num: 24,
            rate_den: 1,
        };
        let measured = report(contract);
        measured.validate(&conversion(contract)).unwrap();
        assert_eq!(measured.last_output_pts, 1000);
        assert_eq!(measured.last_output_duration, 41);
        let span = measured.output_span().unwrap();
        assert_eq!(span.start().ticks, 0);
        assert_eq!(span.end().ticks, 1041);
        assert_eq!(span.end().time_base.numerator(), 1);
        assert_eq!(span.end().time_base.denominator(), 1000);
        // The independently rounded boundary at ordinal 25 would be 1042.
        assert_ne!(span.end().ticks, 1042);

        let fractional = video();
        let fractional_report = report(fractional);
        fractional_report.validate(&conversion(fractional)).unwrap();
        assert_eq!(fractional_report.last_output_pts, 968);
        assert_eq!(fractional_report.last_output_duration, 33);
        assert_eq!(fractional_report.output_span().unwrap().end().ticks, 1001);
    }

    #[test]
    fn report_wire_and_validation_require_measured_duration_and_version_two() {
        let video = VideoContract {
            width: 4,
            height: 2,
            frames: 3,
            rate_num: 24,
            rate_den: 1,
        };
        let request = conversion(video);
        let valid = report(video);
        assert_eq!(valid.last_output_pts, 83);
        assert_eq!(valid.last_output_duration, 41);
        assert_eq!(valid.output_span().unwrap().end().ticks, 124);

        let mut old = valid.clone();
        old.protocol = 1;
        assert_eq!(
            old.validate(&request).unwrap_err(),
            ContractError("unsupported report protocol version")
        );
        assert_eq!(
            old.output_span().unwrap_err(),
            ContractError("unsupported report protocol version")
        );

        let mut wrong_duration = valid.clone();
        wrong_duration.last_output_duration = 42;
        assert!(wrong_duration.validate(&request).is_err());
        wrong_duration.last_output_duration = 0;
        assert!(wrong_duration.output_span().is_err());

        let mut missing = serde_json::to_value(valid).unwrap();
        missing
            .as_object_mut()
            .unwrap()
            .remove("last_output_duration");
        assert!(serde_json::from_value::<ConversionReport>(missing).is_err());
    }
}
